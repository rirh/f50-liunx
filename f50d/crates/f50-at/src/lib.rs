//! The modem's AT channels.
//!
//! The F50's modem is reached through SIPC character devices (`/dev/stty_nr*`). They are not serial ports: only one
//! reader gets the data, and closing one can wedge it. So f50d opens each channel once, for its whole life, and
//! everything that talks to the modem goes through [`At`]:
//!
//! - [`At::send`]: one command, queued on a lane ([`Lane::Control`] for user actions and radio sequences,
//!   [`Lane::Poll`] for the periodic readings, which never delay a control command);
//! - [`At::session`]: several commands with nothing in between (an SMS submit: the PDU mode, the prompt, the PDU);
//! - [`At::urcs`]: unsolicited lines (+CMTI, +CMT with its PDU, +CREG, ^SIMST...), broadcast to every subscriber;
//! - [`guard`]: the AT terminal's safety rules (commands that wedge the port, move the SIM, need confirmation);
//! - [`terminal`]: the `f50.at` object (the guarded terminal and its redacted history).
//!
//! CONTRACT: the items below are used by other crates. Extend them freely; do not change a signature.

pub mod guard;
pub mod parse;
pub mod terminal;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use f50_core::Result;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Which channels to open. Defaults are the F50's.
#[derive(Debug, Clone)]
pub struct AtConfig {
    /// The control channel: user commands, radio sequences, SMS.
    pub control: PathBuf,
    /// The polling channel: periodic readings (signal, cell); `None` puts polls on the control channel.
    pub poll: Option<PathBuf>,
    /// The channel that carries unsolicited result codes, when the modem sends them elsewhere.
    pub urc: Option<PathBuf>,
}

/// Which queue a command waits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Control,
    Poll,
}

/// How to send one command.
#[derive(Debug, Clone, Copy)]
pub struct SendOpts {
    pub lane: Lane,
    /// The modem's time to answer, once the command is on the wire.
    pub timeout: Duration,
    /// The longest wait for the lane before giving up with `busy`.
    pub queue_wait: Duration,
}

impl SendOpts {
    pub const fn control(timeout: Duration) -> Self {
        SendOpts { lane: Lane::Control, timeout, queue_wait: Duration::from_secs(10) }
    }
    pub const fn poll(timeout: Duration) -> Self {
        SendOpts { lane: Lane::Poll, timeout, queue_wait: Duration::from_secs(2) }
    }
}

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Final {
    Ok,
    Error,
    CmeError(u32),
    CmsError(u32),
    NoCarrier,
    /// Any other final line the modem used.
    Other(String),
}

/// A command's reply: the information lines (echo removed) and the final result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub lines: Vec<String>,
    pub status: Final,
}

impl Reply {
    pub fn is_ok(&self) -> bool {
        self.status == Final::Ok
    }

    /// The values of the lines with this prefix: `values("+CSQ")` on `+CSQ: 20,99` yields `20,99`.
    pub fn values<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.lines.iter().filter_map(move |l| {
            let rest = l.strip_prefix(prefix)?;
            Some(rest.strip_prefix(':').unwrap_or(rest).trim_start())
        })
    }

    /// The first value with this prefix.
    pub fn value<'a>(&'a self, prefix: &'a str) -> Option<&'a str> {
        self.values(prefix).next()
    }

    /// Ok(self) when the command succeeded, else a `failed` error naming the modem's answer.
    pub fn check(self) -> Result<Reply> {
        let _ = &self;
        todo!("f50-at: Reply::check")
    }
}

/// An unsolicited line from the modem; `body` is the line that follows it when the code carries one (+CMT's PDU).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Urc {
    pub line: String,
    pub body: Option<String>,
}

/// The AT channels. Cheap to clone.
#[derive(Debug, Clone)]
pub struct At {
    inner: Arc<()>,
}

impl At {
    /// Opens the channels and starts their tasks on `tracker`; they stop at `shutdown`.
    pub async fn start(cfg: AtConfig, tracker: &TaskTracker, shutdown: CancellationToken) -> Result<At> {
        let _ = (cfg, tracker, shutdown);
        todo!("f50-at: At::start")
    }

    /// One command. `busy` when the lane stays taken longer than `opts.queue_wait`.
    pub async fn send(&self, cmd: &str, opts: SendOpts) -> Result<Reply> {
        let _ = (&self.inner, cmd, opts);
        todo!("f50-at: At::send")
    }

    /// The control channel to ourselves until the session is dropped. `busy` after `wait`.
    pub async fn session(&self, owner: &'static str, wait: Duration) -> Result<AtSession> {
        let _ = (owner, wait);
        todo!("f50-at: At::session")
    }

    /// Every unsolicited line from now on.
    pub fn urcs(&self) -> broadcast::Receiver<Urc> {
        todo!("f50-at: At::urcs")
    }

    /// Whether the modem answers on the control channel (false while it restarts).
    pub fn ready(&self) -> watch::Receiver<bool> {
        todo!("f50-at: At::ready")
    }
}

/// Exclusive use of the control channel; released when dropped.
#[derive(Debug)]
pub struct AtSession {
    _priv: (),
}

impl AtSession {
    pub async fn send(&mut self, cmd: &str, timeout: Duration) -> Result<Reply> {
        let _ = (cmd, timeout);
        todo!("f50-at: AtSession::send")
    }

    /// A command that answers with the "> " prompt, then `body` and Ctrl-Z (AT+CMGS with a PDU).
    pub async fn send_with_body(&mut self, cmd: &str, body: &str, timeout: Duration) -> Result<Reply> {
        let _ = (cmd, body, timeout);
        todo!("f50-at: AtSession::send_with_body")
    }
}
