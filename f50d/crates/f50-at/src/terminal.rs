//! The `f50.at` object: the guarded AT terminal and its redacted history (mu300dash's `m_at`, `at_job`,
//! `at_history`).
//!
//! - `send {command, confirm, timeout}`: refused before anything is sent when malformed, chained (";"), when it would
//!   wedge the port (AT+SPENGMD=0,1,...: streaming mode, the channel dead until a reboot) or move the SIM selection
//!   ([`guard`]). A command that does more than answer needs `confirm: true` (`missing` on `confirm`, the message
//!   naming the class). A scan (minutes, holds the channel) or a timeout over 15 s runs as a job of kind `at`
//!   (docs/research/FINDINGS.md 40); everything else answers `{reply_lines, status}` within the request.
//! - `history`: the last 40 commands sent this boot, PIN and password arguments (CPIN, CPWD, CLCK, CGAUTH) cut off.
//!   Kept in RAM and mirrored to `<run>/at-history.json` (0600) so a restarted daemon keeps it until the reboot.
//!
//! Both are `Write`: an AT reply can hold the IMSI, the phone number, a PIN state.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use f50_core::jobs::{JobSpec, Jobs};
use f50_core::paths::Paths;
use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::sanitize::sanitize_text;
use f50_core::time::unix_now;
use f50_core::{Code, Error, Result, fsx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::guard::{self, Class, Refusal};
use crate::{At, Reply, SendOpts};

/// Commands kept in the history.
pub const HISTORY_MAX: usize = 40;
/// The answer budget of a command sent within the request (mu300dash's `-t 8`).
const DEFAULT_TIMEOUT: i64 = 8;
/// Longer than this and the command runs as a job (a request must end well under rpcd's 30 s).
const INLINE_MAX: i64 = 15;
/// The terminal's wait for the channel within a request (mu300dash's `AT_LOCK_WAIT`) and within a job.
const INLINE_QUEUE: Duration = Duration::from_secs(3);
const JOB_QUEUE: Duration = Duration::from_secs(10);
/// A scan's answer budget: "scans run as a background job of at most three minutes" (docs/research/FINDINGS.md 40).
const SCAN_TIMEOUT: Duration = Duration::from_secs(180);
/// What a job may take beyond the command's own budget: the wait for the channel and the drain before it.
const JOB_SLACK: Duration = Duration::from_secs(40);

const SEND: &[Field] = &[
    Field::secret("command", Rule::Text { max_bytes: guard::AT_MAX, max_chars: guard::AT_MAX }, true),
    Field::opt("confirm", Rule::Bool),
    Field::opt("timeout", Rule::Int { min: 1, max: 60 }),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    time: u64,
    command: String,
}

/// The redacted history of this boot.
#[derive(Debug)]
struct History {
    entries: Mutex<VecDeque<Entry>>,
    dir: PathBuf,
}

impl History {
    fn file(dir: &Path) -> PathBuf {
        dir.join("at-history.json")
    }

    fn load(dir: PathBuf) -> Self {
        let mut entries: VecDeque<Entry> = fsx::read_opt(&Self::file(&dir))
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        while entries.len() > HISTORY_MAX {
            entries.pop_front();
        }
        History { entries: Mutex::new(entries), dir }
    }

    fn push(&self, cmd: &str) {
        let snapshot = {
            let mut e = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            e.push_back(Entry { time: unix_now(), command: guard::redact(cmd) });
            while e.len() > HISTORY_MAX {
                e.pop_front();
            }
            serde_json::to_vec(&*e)
        };
        let Ok(bytes) = snapshot else { return };
        let file = Self::file(&self.dir);
        if let Err(e) = fsx::private_dir(&self.dir).and_then(|()| fsx::write_atomic(&file, &bytes, 0o600)) {
            tracing::warn!(error = %e, "could not keep the AT history");
        }
    }

    fn list(&self) -> Vec<Entry> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
    }
}

fn refused(r: Refusal) -> Error {
    match r {
        Refusal::Malformed => Error::field(Code::BadValue, "command", r.message()),
        Refusal::Chained | Refusal::Wedges | Refusal::MovesSim => Error::field(Code::Forbidden, "command", r.message()),
    }
}

fn reply_json(r: &Reply) -> Value {
    let lines: Vec<String> = r.lines.iter().map(|l| sanitize_text(l)).collect();
    json!({ "reply_lines": lines, "status": sanitize_text(&r.status.to_string()) })
}

/// A timeout from the terminal says what the user needs to know before sending again.
fn after_send(e: Error) -> Error {
    if e.code() == Code::Timeout {
        Error::new(
            Code::Timeout,
            "the modem did not answer in time; the command may still run, check before sending it again",
        )
    } else {
        e
    }
}

fn secs(n: i64) -> Duration {
    Duration::from_secs(u64::try_from(n).unwrap_or(1))
}

async fn send(at: At, jobs: Jobs, history: Arc<History>, p: Params) -> Result<Value> {
    let (cmd, class) = guard::check(p.need_str("command")?).map_err(refused)?;
    // a scan in the background holds the channel for minutes: say so, not just "busy"
    if let Some(id) = jobs.running("at") {
        return Err(Error::busy(format!("a long AT command is still running in the background ({id})")));
    }
    if let Some(c) = class
        && !p.flag("confirm")
    {
        return Err(Error::field(Code::Missing, "confirm", format!("this command needs confirmation: {}", c.as_str())));
    }
    let timeout = p.int("timeout").unwrap_or(DEFAULT_TIMEOUT);
    if class == Some(Class::Scan) || timeout > INLINE_MAX {
        let budget = if class == Some(Class::Scan) { SCAN_TIMEOUT } else { secs(timeout) };
        let spec = JobSpec::new("f50.at send", "at", budget + JOB_SLACK);
        let job_cmd = cmd.clone();
        let id = jobs.start(spec, move |ctx| async move {
            // never the command itself: it may carry a PIN
            ctx.log("command queued for the control channel");
            let r = at.send(&job_cmd, SendOpts { queue_wait: JOB_QUEUE, ..SendOpts::control(budget) }).await;
            let r = r.map_err(after_send)?;
            ctx.log(format!("answered: {}", r.status));
            Ok(reply_json(&r))
        })?;
        history.push(&cmd);
        return Ok(json!({ "job": id }));
    }
    let r = at.send(&cmd, SendOpts { queue_wait: INLINE_QUEUE, ..SendOpts::control(secs(timeout)) }).await;
    // in the history once it reached the modem (answered or not), never when it was refused or never sent
    if r.is_ok() || r.as_ref().is_err_and(|e| e.code() == Code::Timeout) {
        history.push(&cmd);
    }
    Ok(reply_json(&r.map_err(after_send)?))
}

/// The `f50.at` object over `at`; long commands run as jobs of `jobs`.
pub fn object(at: At, jobs: Jobs, paths: &Paths) -> Object {
    let history = Arc::new(History::load(paths.run()));
    let h = history.clone();
    Object::builder("f50.at")
        .method(
            "send",
            Access::Write,
            SEND,
            "Sends one guarded AT command on the control channel; scans and long commands run as a job.",
            move |p: Params| send(at.clone(), jobs.clone(), history.clone(), p),
        )
        .method(
            "history",
            Access::Write,
            &[],
            "The last 40 AT commands of this boot, PINs and passwords cut off.",
            move |_p: Params| {
                let list = h.list();
                async move { Ok(json!({ "history": list })) }
            },
        )
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Final;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f50-at-hist-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn history_is_redacted_bounded_and_kept() {
        let d = dir("keep");
        let h = History::load(d.clone());
        h.push("AT+CPIN=1234");
        for i in 0..HISTORY_MAX + 5 {
            h.push(&format!("AT+CSQ{i}"));
        }
        let l = h.list();
        assert_eq!(l.len(), HISTORY_MAX);
        assert!(l.iter().all(|e| !e.command.contains("1234")));
        assert_eq!(l.last().map(|e| e.command.as_str()), Some("AT+CSQ44"));
        // a restarted daemon finds it again
        assert_eq!(History::load(d.clone()).list().len(), HISTORY_MAX);
        let d2 = dir("redact");
        let h = History::load(d2.clone());
        h.push("at+cgauth=1,1,\"u\",\"pw\"");
        assert_eq!(h.list()[0].command, "at+cgauth=...");
        let _ = std::fs::remove_dir_all(d);
        let _ = std::fs::remove_dir_all(d2);
    }

    #[test]
    fn refusals_and_replies() {
        assert_eq!(refused(Refusal::Malformed).code(), Code::BadValue);
        assert_eq!(refused(Refusal::Wedges).field_name(), Some("command"));
        assert_eq!(refused(Refusal::Chained).code(), Code::Forbidden);
        let r = Reply { lines: vec!["+CSQ: 20,99\u{1b}".into()], status: Final::CmeError(3) };
        assert_eq!(reply_json(&r), json!({"reply_lines": ["+CSQ: 20,99"], "status": "+CME ERROR: 3"}));
    }
}
