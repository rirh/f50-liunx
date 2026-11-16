//! Telegram push: every received message the pool takes is sent to a Telegram chat by a bot (`sendMessage`).
//!
//! The settings (`enabled`, the bot token, the chat id, the Bot API base for a self-hosted or reverse-proxied API) are
//! kept in the pool's private directory (`telegram.json`, 0600). The token is never returned: `push` says whether one
//! is set, with its bot id and last four characters.
//!
//! The request is made by curl: the token (in the URL) and the message (in the body) go on its stdin as a curl config
//! (`-K -`), never into the arguments, which any user can read in /proc. One task sends the messages one after another
//! from a bounded queue (a full queue drops and logs, it never holds the receiver); a failure is retried with a
//! backoff, `429` after the `retry_after` Telegram asks for, and a refusal that a retry cannot change (a wrong token, a
//! chat the bot cannot write to) is not retried. The last outcome is kept in memory for the console.
//!
//! `proxy` (on by default) sends through the proxy engine's local SOCKS5 inbound while the proxy runs (names
//! resolved by the proxy, `socks5h`), and directly while it does not; f50d's wiring says where the proxy is.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::fsx;
use f50_core::sanitize::{sanitize_message, truncate_bytes};
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::pool::Message;

/// Messages waiting to be pushed.
const QUEUE: usize = 64;
/// The waits before each retry; a message is given up after the last.
const BACKOFF: &[u64] = &[5, 30, 120, 600];
/// The longest `retry_after` honoured; a longer one gives the message up.
const RETRY_AFTER_MAX: u64 = 900;
/// One request's whole budget (curl's `--max-time`), and the program's a little beyond it.
const REQUEST_SECS: u64 = 20;
/// Telegram's limit for a message's text, in characters.
const TEXT_MAX_CHARS: usize = 4096;

pub(crate) const DEFAULT_API: &str = "https://api.telegram.org";

/// What the settings file holds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct PushConfig {
    pub enabled: bool,
    pub token: String,
    pub chat_id: String,
    pub api: String,
    /// Through the local proxy while it runs.
    pub proxy: bool,
}

impl Default for PushConfig {
    fn default() -> Self {
        PushConfig {
            enabled: false,
            token: String::new(),
            chat_id: String::new(),
            api: DEFAULT_API.to_owned(),
            proxy: true,
        }
    }
}

impl PushConfig {
    fn ready(&self) -> bool {
        self.enabled && !self.token.is_empty() && !self.chat_id.is_empty()
    }
}

/// A change to the settings; `None` keeps the value.
#[derive(Debug, Default)]
pub(crate) struct PushChange {
    pub enabled: Option<bool>,
    pub token: Option<String>,
    pub chat_id: Option<String>,
    pub api: Option<String>,
    pub proxy: Option<bool>,
}

impl PushChange {
    fn apply(self, mut c: PushConfig) -> PushConfig {
        if let Some(v) = self.enabled {
            c.enabled = v;
        }
        if let Some(v) = self.token {
            c.token = v;
        }
        if let Some(v) = self.chat_id {
            c.chat_id = v;
        }
        if let Some(v) = self.api {
            c.api = if v.is_empty() { DEFAULT_API.to_owned() } else { v.trim_end_matches('/').to_owned() };
        }
        if let Some(v) = self.proxy {
            c.proxy = v;
        }
        c
    }
}

/// How the last push went.
#[derive(Debug, Clone, Serialize)]
struct Outcome {
    at: u64,
    ok: bool,
    /// The message pushed (absent for a test).
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PushEvent {
    from: String,
    date: String,
    text: String,
    id: u64,
}

#[derive(Debug)]
struct State {
    cfg: PushConfig,
    last: Option<Outcome>,
}

/// Where the local proxy listens now (`None`: not running).
pub type LocalProxy = Arc<dyn Fn() -> Option<SocketAddr> + Send + Sync>;

/// The push settings, the queue and the last outcome. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Push {
    path: PathBuf,
    state: Arc<Mutex<State>>,
    tx: mpsc::Sender<PushEvent>,
    local_proxy: Arc<OnceLock<LocalProxy>>,
}

impl std::fmt::Debug for Push {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Push").field("path", &self.path).field("state", &self.state).finish_non_exhaustive()
    }
}

impl Push {
    /// Reads the settings in `dir` and starts the sender on `tracker` until `shutdown`.
    pub fn start(dir: PathBuf, tracker: &TaskTracker, shutdown: CancellationToken) -> Push {
        let path = dir.join("telegram.json");
        let cfg = match fsx::read_opt(&path) {
            Ok(Some(s)) => serde_json::from_str(&s).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "the Telegram push settings are unreadable; push is off");
                PushConfig::default()
            }),
            Ok(None) => PushConfig::default(),
            Err(e) => {
                tracing::warn!(error = %e, "the Telegram push settings could not be read; push is off");
                PushConfig::default()
            }
        };
        let (tx, rx) = mpsc::channel(QUEUE);
        let push = Push {
            path,
            state: Arc::new(Mutex::new(State { cfg, last: None })),
            tx,
            local_proxy: Arc::new(OnceLock::new()),
        };
        tracker.spawn(run(push.clone(), rx, shutdown));
        push
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn config(&self) -> PushConfig {
        self.lock().cfg.clone()
    }

    /// Says where the local proxy is; set once, by f50d's wiring.
    pub fn set_local_proxy(&self, f: LocalProxy) {
        let _ = self.local_proxy.set(f);
    }

    fn proxy_now(&self) -> Option<SocketAddr> {
        self.local_proxy.get().and_then(|f| f())
    }

    /// The proxy a send with `cfg` goes through.
    fn via(&self, cfg: &PushConfig) -> Option<SocketAddr> {
        if cfg.proxy { self.proxy_now() } else { None }
    }

    /// The settings without the token, with the queue and the last outcome.
    pub fn view(&self) -> Value {
        let st = self.lock();
        let c = &st.cfg;
        json!({
            "enabled": c.enabled,
            "token_set": !c.token.is_empty(),
            "token_hint": token_hint(&c.token),
            "chat_id": c.chat_id,
            "api": c.api,
            "default_api": DEFAULT_API,
            "proxy": c.proxy,
            "proxy_running": self.proxy_now().is_some(),
            "queued": QUEUE - self.tx.capacity(),
            "last": st.last,
        })
    }

    /// Saves a change; turning push on needs a token and a chat id.
    pub async fn set(&self, change: PushChange) -> Result<Value> {
        let next = change.apply(self.config());
        if next.enabled && next.token.is_empty() {
            return Err(Error::bad_value("token"));
        }
        if next.enabled && next.chat_id.is_empty() {
            return Err(Error::bad_value("chat_id"));
        }
        let data = serde_json::to_vec_pretty(&next).map_err(|e| Error::failed(e.to_string()))?;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || fsx::write_atomic(&path, &data, 0o600))
            .await
            .map_err(|e| Error::failed(e.to_string()))??;
        self.lock().cfg = next;
        Ok(self.view())
    }

    /// Sends a test message with the saved settings, changed by `change` (not saved): a form can be tried first.
    pub async fn test(&self, change: PushChange) -> Result<Value> {
        let cfg = change.apply(self.config());
        if cfg.token.is_empty() {
            return Err(Error::bad_value("token"));
        }
        if cfg.chat_id.is_empty() {
            return Err(Error::bad_value("chat_id"));
        }
        let text = format!("✅ F50 Telegram push works\n{}", host_line());
        let r = send(&cfg, self.via(&cfg), &text).await;
        self.record(None, r.as_ref().err().map(|e| e.msg.clone()));
        r.map(|()| json!({ "sent": true })).map_err(|e| Error::failed(e.msg))
    }

    /// Hands newly pooled received messages to the sender; nothing happens while push is off.
    pub fn offer(&self, msgs: &[Message]) {
        if !self.lock().cfg.ready() {
            return;
        }
        for m in msgs {
            let ev = PushEvent { from: m.peer.clone(), date: m.time.clone(), text: m.text.clone(), id: m.id };
            if self.tx.try_send(ev).is_err() {
                tracing::warn!(id = m.id, "the Telegram push queue is full; this message is not pushed");
            }
        }
    }

    fn record(&self, id: Option<u64>, error: Option<String>) {
        self.lock().last = Some(Outcome { at: unix_now(), ok: error.is_none(), id, error });
    }
}

async fn run(push: Push, mut rx: mpsc::Receiver<PushEvent>, shutdown: CancellationToken) {
    loop {
        let ev = tokio::select! {
            () = shutdown.cancelled() => break,
            ev = rx.recv() => match ev {
                Some(ev) => ev,
                None => break,
            },
        };
        let text = format_message(&ev);
        let mut attempt = 0;
        loop {
            // the settings are read for every attempt: push turned off (or a new token) applies to what is queued
            let cfg = push.config();
            if !cfg.ready() {
                break;
            }
            let err = match send(&cfg, push.via(&cfg), &text).await {
                Ok(()) => {
                    push.record(Some(ev.id), None);
                    break;
                }
                Err(e) => e,
            };
            let wait = match (err.retry, BACKOFF.get(attempt)) {
                (Retry::Never, _) | (_, None) => None,
                (Retry::After(s), _) if s > RETRY_AFTER_MAX => None,
                (Retry::After(s), _) => Some(s.max(1)),
                (Retry::Backoff, Some(s)) => Some(*s),
            };
            tracing::warn!(id = ev.id, attempt, error = %err.msg, retry_in = ?wait, "Telegram push failed");
            push.record(Some(ev.id), Some(err.msg));
            let Some(wait) = wait else { break };
            attempt += 1;
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(wait)) => {}
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Retry {
    Backoff,
    After(u64),
    Never,
}

#[derive(Debug)]
struct SendError {
    msg: String,
    retry: Retry,
}

impl SendError {
    fn new(msg: impl AsRef<str>, retry: Retry) -> Self {
        SendError { msg: sanitize_message(msg.as_ref()), retry }
    }
}

/// One `sendMessage`, through `via` (the local proxy) when given.
async fn send(cfg: &PushConfig, via: Option<SocketAddr>, text: &str) -> std::result::Result<(), SendError> {
    let body = json!({ "chat_id": cfg.chat_id, "text": text, "disable_web_page_preview": true }).to_string();
    let mut conf = format!(
        "url = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\nwrite-out = \"\\n%{{http_code}}\"\n",
        curl_quote(&format!("{}/bot{}/sendMessage", cfg.api, cfg.token)),
        curl_quote(&body),
    );
    // never an inherited *_proxy variable: the setting alone decides
    conf.push_str(&match via {
        Some(addr) => format!("proxy = {}\n", curl_quote(&format!("socks5h://{addr}"))),
        None => "noproxy = \"*\"\n".to_owned(),
    });
    let out = Cmd::new("curl")
        .args(["-sS", "--proto", "=https,http", "--max-time", &REQUEST_SECS.to_string(), "-K", "-"])
        .stdin(conf)
        .timeout(Duration::from_secs(REQUEST_SECS + 5))
        .max_output(64 * 1024)
        .run()
        .await
        .map_err(|e| SendError::new(format!("curl could not be started: {e}"), Retry::Never))?;
    if out.timed_out {
        return Err(SendError::new("the request timed out", Retry::Backoff));
    }
    let (resp, code) = out.stdout.rsplit_once('\n').unwrap_or(("", out.stdout.as_str()));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if out.status != Some(0) || code == 0 {
        // a network failure (no route, DNS, TLS): curl's own message, without the URL
        let m = redact(&out.message(), &cfg.token);
        let m = if m.is_empty() { "the Bot API cannot be reached".into() } else { m };
        let m = if via.is_some() { format!("{m} (through the proxy)") } else { m };
        return Err(SendError::new(m, Retry::Backoff));
    }
    let v: Value = serde_json::from_str(resp).unwrap_or(Value::Null);
    if code == 200 && v["ok"] == true {
        return Ok(());
    }
    let desc = v["description"].as_str().map_or_else(|| format!("HTTP {code}"), |d| format!("{code}: {d}"));
    let retry = match code {
        429 => Retry::After(v["parameters"]["retry_after"].as_u64().unwrap_or(30)),
        400 | 401 | 403 | 404 => Retry::Never,
        _ => Retry::Backoff,
    };
    Err(SendError::new(redact(&desc, &cfg.token), retry))
}

/// A double-quoted curl config value: backslash and quote escaped, and no line break can end it early.
fn curl_quote(s: &str) -> String {
    let mut q = String::with_capacity(s.len() + 2);
    q.push('"');
    for c in s.chars() {
        match c {
            '\\' => q.push_str("\\\\"),
            '"' => q.push_str("\\\""),
            '\n' => q.push_str("\\n"),
            '\r' => q.push_str("\\r"),
            '\t' => q.push_str("\\t"),
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

fn redact(s: &str, token: &str) -> String {
    if token.is_empty() { s.to_owned() } else { s.replace(token, "***") }
}

/// The token as `<bot id>:…<last four>`, for recognising which bot is set.
fn token_hint(token: &str) -> Option<String> {
    let (bot, secret) = token.split_once(':')?;
    let tail: String = secret.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    Some(format!("{bot}:…{tail}"))
}

fn host_line() -> String {
    let host = fsx::read_line(std::path::Path::new("/proc/sys/kernel/hostname")).unwrap_or_default();
    if host.is_empty() { "F50".to_owned() } else { host }
}

fn format_message(ev: &PushEvent) -> String {
    let head = format!("📩 {}\n🕒 {} · {}\n\n", ev.from, ev.date, host_line());
    let room = TEXT_MAX_CHARS.saturating_sub(head.chars().count());
    let text: String = if ev.text.chars().count() > room {
        let mut t: String = ev.text.chars().take(room.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        ev.text.clone()
    };
    // a byte bound too, for a text made of the widest characters
    format!("{head}{}", truncate_bytes(&text, 4 * TEXT_MAX_CHARS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_keeps_the_config_one_line() {
        assert_eq!(curl_quote(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(curl_quote("x\ny"), r#""x\ny""#);
        let q = curl_quote(&json!({"text": "line1\nline2 \"q\""}).to_string());
        assert!(!q.contains('\n'));
    }

    #[test]
    fn hint_hides_the_secret() {
        assert_eq!(token_hint("123456:ABCDEFGHijklmnop").as_deref(), Some("123456:…mnop"));
        assert_eq!(token_hint(""), None);
    }

    #[test]
    fn change_keeps_what_it_does_not_name() {
        let base = PushConfig {
            enabled: true,
            token: "1:a".into(),
            chat_id: "42".into(),
            api: DEFAULT_API.into(),
            proxy: true,
        };
        let c = PushChange { api: Some("https://tg.example.com/".into()), ..Default::default() }.apply(base.clone());
        assert_eq!(c.token, "1:a");
        assert_eq!(c.api, "https://tg.example.com");
        let c = PushChange { api: Some(String::new()), ..Default::default() }.apply(c);
        assert_eq!(c.api, DEFAULT_API);
        assert!(c.proxy);
        let c = PushChange { proxy: Some(false), ..Default::default() }.apply(c);
        assert!(!c.proxy);
    }

    #[test]
    fn proxy_is_on_for_settings_saved_before_it() {
        let c: PushConfig = serde_json::from_str(r#"{"enabled":true,"token":"1:a","chat_id":"42"}"#).unwrap();
        assert!(c.proxy);
    }

    #[test]
    fn long_text_is_cut_to_telegrams_limit() {
        let ev = PushEvent { from: "10086".into(), date: "2026-11-16 10:00".into(), text: "字".repeat(5000), id: 1 };
        let m = format_message(&ev);
        assert!(m.chars().count() <= TEXT_MAX_CHARS);
        assert!(m.ends_with('…'));
    }
}
