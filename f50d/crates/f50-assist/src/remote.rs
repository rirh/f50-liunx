//! SMS remote control (短信遥控): a message from an allowed number that is a command is run and answered by SMS.
//!
//! - Off by default; only the numbers listed (compared without `+86`) are heard, and with a `secret` set the message
//!   must start with it (`<secret> 状态`). Without a secret only exact commands count, so an ordinary message from
//!   an allowed number is left alone; with one, an unknown command is answered with the help.
//! - A message pooled more than [`MAX_AGE`] ago is not run (one that waited out a restart).
//! - Commands (Chinese or English): 状态/status, 流量/traffic, 余额/balance, 代理开/代理关 (proxy on/off),
//!   节点 <名称> (node <name>), WiFi开/WiFi关 (wifi on/off), 重启/reboot, 帮助/help.
//! - The last [`HISTORY`] commands are kept with their answers.

use std::collections::VecDeque;
use std::time::Duration;

use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::inbox::{Incoming, normalize};
use crate::{Inner, balance, store};

const FILE: &str = "remote.json";
const HISTORY: usize = 20;
/// Older commands are not run.
const MAX_AGE: u64 = 600;
const HELP: &str = "F50 短信遥控：状态、流量、余额、代理开、代理关、节点 名称、WiFi开、WiFi关、重启";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub enabled: bool,
    /// Allowed numbers, as typed.
    pub numbers: Vec<String>,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub at: u64,
    pub from: String,
    pub command: String,
    pub ok: bool,
    pub reply: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct State {
    pub settings: Settings,
    pub history: VecDeque<Entry>,
}

impl State {
    pub fn load(dir: &std::path::Path) -> State {
        store::load(dir, FILE)
    }

    /// Without the secret: whether one is set.
    pub fn view(&self) -> Value {
        json!({
            "enabled": self.settings.enabled,
            "numbers": self.settings.numbers,
            "secret_set": !self.settings.secret.is_empty(),
            "history": self.history,
            "help": HELP,
        })
    }
}

/// What a message asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Status,
    Traffic,
    Balance,
    Proxy(bool),
    Node(String),
    Wifi(bool),
    Reboot,
}

/// The command in `text` (after the secret, when one is set); `Some(None)`: the secret matched but the command is
/// unknown.
pub(crate) fn parse(text: &str, secret: &str) -> Option<Option<Command>> {
    let mut t = text.trim();
    if !secret.is_empty() {
        t = t.strip_prefix(secret)?.trim();
    }
    let lower = t.to_lowercase();
    let squeezed: String = lower.split_whitespace().collect();
    let cmd = match squeezed.as_str() {
        "帮助" | "help" | "?" | "？" => Some(Command::Help),
        "状态" | "status" => Some(Command::Status),
        "流量" | "traffic" => Some(Command::Traffic),
        "余额" | "话费" | "balance" => Some(Command::Balance),
        "代理开" | "开代理" | "开启代理" | "proxyon" => Some(Command::Proxy(true)),
        "代理关" | "关代理" | "关闭代理" | "proxyoff" => Some(Command::Proxy(false)),
        "wifi开" | "开wifi" | "开启wifi" | "wifion" => Some(Command::Wifi(true)),
        "wifi关" | "关wifi" | "关闭wifi" | "wifioff" => Some(Command::Wifi(false)),
        "重启" | "reboot" => Some(Command::Reboot),
        _ => {
            let name = t.strip_prefix("节点").or_else(|| (lower.starts_with("node ")).then(|| &t[5..]));
            name.map(str::trim).filter(|n| !n.is_empty()).map(|n| Command::Node(n.to_owned()))
        }
    };
    match (cmd, secret.is_empty()) {
        (Some(c), _) => Some(Some(c)),
        (None, true) => None,
        (None, false) => Some(None),
    }
}

fn gb(bytes: f64) -> String {
    format!("{:.1}GB", bytes / 1_073_741_824.0)
}

fn uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if d > 0 { format!("{d}天{h}小时") } else { format!("{h}小时{m}分") }
}

async fn status_text(inner: &Inner) -> Result<String> {
    let s = inner.call("f50.status", "get", json!({})).await?;
    let m = &s["modem"]["data"];
    let d = &s["data"]["data"];
    let sig = &m["signal"];
    let mut lines = vec!["F50 状态".to_owned()];
    lines.push(format!(
        "网络：{} {} {}格 RSRP {} SINR {}",
        m["operator"].as_str().unwrap_or("?"),
        m["rat"].as_str().unwrap_or("-").to_uppercase(),
        m["bars"].as_u64().unwrap_or(0),
        sig["rsrp"].as_f64().map_or("-".into(), |v| format!("{v:.0}")),
        sig["sinr"].as_f64().map_or("-".into(), |v| format!("{v:.0}")),
    ));
    let addrs: Vec<&str> = d["addresses"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    lines.push(format!(
        "数据：{}{}",
        if d["state"] == "connected" { "已连接" } else { "未连接" },
        addrs.first().map(|a| format!(" {}", a.split('/').next().unwrap_or(a))).unwrap_or_default()
    ));
    if let Ok(p) = inner.call("f50.proxy", "status", json!({})).await {
        let on = p["state"] == "running";
        lines.push(format!(
            "代理：{}{}",
            if on {
                "运行"
            } else if p["enabled"] == true {
                "启动中"
            } else {
                "关"
            },
            p["node"]["name"].as_str().filter(|_| on).map(|n| format!(" {n}")).unwrap_or_default()
        ));
    }
    lines.push(traffic_line(&s["traffic"]["data"]));
    let b = Inner::lock(&inner.balance).clone();
    if let Some(a) = b.amount {
        lines.push(format!("余额：{a:.2}元"));
    }
    let sys = &s["system"]["data"];
    lines.push(format!(
        "温度 {}℃ 终端 {} 运行 {}",
        sys["temperatures"]["hottest"].as_f64().map_or("-".into(), |v| format!("{v:.0}")),
        sys["lan_leases"].as_u64().unwrap_or(0),
        uptime(sys["uptime"].as_u64().unwrap_or(0)),
    ));
    Ok(lines.join("\n"))
}

fn traffic_line(t: &Value) -> String {
    let used = t["cycle"]["used"].as_f64().unwrap_or(0.0);
    let today = &t["days"][0];
    let today = today["rx"].as_f64().unwrap_or(0.0) + today["tx"].as_f64().unwrap_or(0.0);
    let cap = t["cap"]["bytes"].as_f64().filter(|c| *c > 0.0);
    format!("流量：本月 {}{}，今日 {}", gb(used), cap.map(|c| format!("/{}", gb(c))).unwrap_or_default(), gb(today))
}

async fn node(inner: &Inner, want: &str) -> Result<String> {
    let p = inner.call("f50.proxy", "profile", json!({})).await?;
    let want_l = want.to_lowercase();
    let nodes: Vec<&Value> = p["subscriptions"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| s["nodes"].as_array().into_iter().flatten())
        .chain(p["local_nodes"].as_array().into_iter().flatten())
        .filter(|n| n["problem"].is_null())
        .collect();
    let name = |n: &Value| n["name"].as_str().unwrap_or_default().to_owned();
    let pick = nodes
        .iter()
        .find(|n| name(n).to_lowercase() == want_l)
        .or_else(|| nodes.iter().find(|n| name(n).to_lowercase().contains(&want_l)))
        .ok_or_else(|| Error::not_found(format!("节点「{want}」")))?;
    let id = pick["id"].as_str().unwrap_or_default();
    inner.call("f50.proxy", "set", json!({ "selected": id })).await?;
    Ok(format!("已切换到节点 {}", name(pick)))
}

/// Runs a command; the answer, and whether to reboot after it is sent.
async fn run(inner: &Inner, cmd: &Command) -> Result<(String, bool)> {
    Ok(match cmd {
        Command::Help => (HELP.into(), false),
        Command::Status => (status_text(inner).await?, false),
        Command::Traffic => {
            let t = inner.call("f50.traffic", "get", json!({})).await?;
            (traffic_line(&t), false)
        }
        Command::Balance => {
            let st = balance::ask_and_wait(inner, Duration::from_secs(150)).await?;
            (format!("话费余额：{:.2}元", st.amount.unwrap_or(0.0)), false)
        }
        Command::Proxy(on) => {
            inner.call("f50.proxy", "set", json!({ "enabled": on })).await?;
            ((if *on { "代理已开启" } else { "代理已关闭" }).into(), false)
        }
        Command::Node(n) => (node(inner, n).await?, false),
        Command::Wifi(on) => {
            inner.call("f50.wifi", "power", json!({ "state": if *on { "on" } else { "off" } })).await?;
            ((if *on { "WiFi 热点已开启" } else { "WiFi 热点已关闭" }).into(), false)
        }
        Command::Reboot => ("F50 即将重启".into(), true),
    })
}

/// Sends an SMS and waits (up to 2 minutes) for its job to end.
async fn reply(inner: &Inner, to: &str, text: &str) -> Result<()> {
    let job = inner.call("f50.sms", "send", json!({ "number": to, "text": text })).await?;
    let Some(id) = job["job"].as_str() else { return Ok(()) };
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let st = inner.call("f50.job", "status", json!({ "id": id })).await?;
        match st["state"].as_str() {
            Some("done") => return Ok(()),
            Some("running" | "queued") | None => {}
            Some(other) => {
                return Err(Error::failed(st["error"].as_str().map_or_else(|| format!("send {other}"), str::to_owned)));
            }
        }
    }
    Err(Error::timeout("the reply"))
}

/// A pooled message: a command from an allowed number is run and answered. True when it was one.
pub(crate) async fn offer(inner: &Inner, m: &Incoming) -> bool {
    let settings = Inner::lock(&inner.remote).settings.clone();
    if !settings.enabled || !settings.numbers.iter().any(|n| normalize(n) == normalize(&m.peer)) {
        return false;
    }
    let Some(cmd) = parse(&m.text, &settings.secret) else { return false };
    if unix_now().saturating_sub(m.pooled) > MAX_AGE {
        tracing::warn!(id = m.id, "an SMS command waited too long; not run");
        return true;
    }
    let (answer, ok, reboot) = match &cmd {
        None => (format!("未知命令。{HELP}"), false, false),
        Some(c) => {
            tracing::info!(command = ?c, "SMS remote control");
            match run(inner, c).await {
                Ok((a, r)) => (a, true, r),
                Err(e) => (format!("失败：{}", e.message()), false, false),
            }
        }
    };
    let sent = reply(inner, &m.peer, &answer).await;
    if let Err(e) = &sent {
        tracing::warn!(error = %e, "the SMS remote control's answer was not sent");
    }
    {
        let mut st = Inner::lock(&inner.remote);
        let command: String =
            m.text.trim().strip_prefix(settings.secret.as_str()).unwrap_or(&m.text).trim().chars().take(40).collect();
        st.history.push_front(Entry { at: unix_now(), from: m.peer.clone(), command, ok, reply: answer });
        st.history.truncate(HISTORY);
    }
    let snapshot = Inner::lock(&inner.remote).clone();
    let _ = store::save(&inner.dir, FILE, &snapshot).await;
    if reboot {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if let Err(e) = inner.call("f50.system", "reboot", json!({})).await {
            tracing::warn!(error = %e, "SMS remote control: reboot failed");
        }
    }
    true
}

pub(crate) async fn set(
    inner: &Inner,
    enabled: Option<bool>,
    numbers: Option<&str>,
    secret: Option<&str>,
) -> Result<Value> {
    let next = {
        let mut st = Inner::lock(&inner.remote).clone();
        if let Some(n) = numbers {
            let mut list: Vec<String> = Vec::new();
            for raw in n.split([',', ' ', ';', '，', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
                let d = normalize(raw);
                if d.len() < 5 || d.len() > 15 {
                    return Err(Error::field(f50_core::Code::BadValue, "numbers", format!("「{raw}」不是电话号码")));
                }
                if !list.iter().any(|x| normalize(x) == d) {
                    list.push(raw.to_owned());
                }
            }
            st.settings.numbers = list;
        }
        if let Some(s) = secret {
            st.settings.secret = s.trim().to_owned();
        }
        if let Some(e) = enabled {
            st.settings.enabled = e;
        }
        if st.settings.enabled && st.settings.numbers.is_empty() {
            return Err(Error::field(f50_core::Code::Missing, "numbers", "开启前先填写允许的号码"));
        }
        st
    };
    store::save(&inner.dir, FILE, &next).await?;
    let view = next.view();
    *Inner::lock(&inner.remote) = next;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_without_a_secret() {
        assert_eq!(parse(" 状态 ", ""), Some(Some(Command::Status)));
        assert_eq!(parse("WiFi 关", ""), Some(Some(Command::Wifi(false))));
        assert_eq!(parse("proxy on", ""), Some(Some(Command::Proxy(true))));
        assert_eq!(parse("节点 香港", ""), Some(Some(Command::Node("香港".into()))));
        assert_eq!(parse("node HK-80", ""), Some(Some(Command::Node("HK-80".into()))));
        assert_eq!(parse("今晚回家吃饭", ""), None, "ordinary messages are left alone");
    }

    #[test]
    fn commands_after_the_secret() {
        assert_eq!(parse("abc 重启", "abc"), Some(Some(Command::Reboot)));
        assert_eq!(parse("abc 你好", "abc"), Some(None), "unknown after the secret");
        assert_eq!(parse("重启", "abc"), None, "no secret, nothing");
    }

    #[test]
    fn uptime_reads() {
        assert_eq!(uptime(107_508), "1天5小时");
        assert_eq!(uptime(3_700), "1小时1分");
    }
}
