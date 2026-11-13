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
pub mod keeper;
pub mod parse;
pub mod terminal;

mod channel;
mod codes;
mod reader;
mod request;
mod sys;
mod urc;
mod wire;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use f50_core::{Code, Error, Result};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use channel::{Channel, Health, Liveness, Source};
use request::{Command, Job, QUEUE_MAX, Queued, check_line, submit};

/// The longest command line [`At::send`] puts on the wire (the terminal's own limit is [`guard::AT_MAX`]).
pub const COMMAND_MAX: usize = 1024;
/// The longest body [`AtSession::send_with_body`] sends (a PDU in hexadecimal is under 400).
pub const BODY_MAX: usize = 4096;
/// The longest a command may be given to answer.
pub const TIMEOUT_MAX: Duration = Duration::from_secs(600);
/// Unsolicited lines kept for a subscriber that is behind (it then sees `Lagged`, never a stalled channel).
pub const URC_BACKLOG: usize = 1024;

/// Which channels to open. Defaults are the F50's.
#[derive(Debug, Clone)]
pub struct AtConfig {
    /// The control channel: user commands, radio sequences, SMS.
    pub control: PathBuf,
    /// The polling channel: periodic readings (signal, cell); `None` puts polls on the control channel.
    pub poll: Option<PathBuf>,
    /// The channel that carries unsolicited result codes, when the modem sends them elsewhere.
    pub urc: Option<PathBuf>,
    /// The descriptor keeper's socket ([`keeper`]): the channels come from it so they survive f50d itself. `None`,
    /// or a keeper that is not running, opens the devices directly.
    pub keeper: Option<PathBuf>,
}

impl Default for AtConfig {
    /// The F50 on OpenWrt, slot 0 (see [`AtConfig::for_slot`]).
    fn default() -> Self {
        AtConfig::for_slot(0)
    }
}

impl AtConfig {
    /// The channels of a SIM slot. Slot 0: commands on nr1, URCs on nr0, and polls on nr6, one of the two channels
    /// the vendor RIL never uses that the LuCI panel's collector took for itself so its polls never queue behind the
    /// control channel (init.d/f50-atd-dash, docs/research/FINDINGS.md 40, 74). Slot 1 (the F50's internal card):
    /// commands on nr4, URCs on nr3, polls on the control channel (nr6/nr7 belong to slot 0's pool; FINDINGS 74).
    /// nr2/nr5 (the data channels the shell gave mobile-data) are left closed: every channel held open is one more
    /// reader that can wedge (init.d/f50-atd), and a channel opened is never given back (FINDINGS 13b).
    pub fn for_slot(slot: u8) -> Self {
        let dev = |n: u8| PathBuf::from(format!("/dev/stty_nr{n}"));
        if slot == 1 {
            AtConfig { control: dev(4), poll: None, urc: Some(dev(3)), keeper: Some(keeper::KEEPER_SOCKET.into()) }
        } else {
            AtConfig {
                control: dev(1),
                poll: Some(dev(6)),
                urc: Some(dev(0)),
                keeper: Some(keeper::KEEPER_SOCKET.into()),
            }
        }
    }
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
    /// `CONNECT` (ATD, AT+CGDATA) counts as success; [`Reply::is_ok`] is `OK` alone.
    pub fn check(self) -> Result<Reply> {
        codes::check(self)
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
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    control: mpsc::Sender<Queued>,
    /// The poll channel's queue, when it has a device of its own.
    poll: Option<mpsc::Sender<Queued>>,
    /// Whether the poll channel answers; while it does not, polls go to `fallback`.
    poll_up: Arc<AtomicBool>,
    /// Polls served by the control channel's task, after every control command.
    fallback: mpsc::Sender<Queued>,
    urcs: broadcast::Sender<Urc>,
    ready: watch::Receiver<bool>,
}

fn check_timeout(timeout: Duration) -> Result<()> {
    if timeout.is_zero() || timeout > TIMEOUT_MAX {
        return Err(Error::new(Code::BadValue, "an AT timeout is more than 0 and at most 600 s"));
    }
    Ok(())
}

impl At {
    /// Opens the channels and starts their tasks on `tracker`; they stop at `shutdown`.
    ///
    /// Returns at once: each task waits for its device by itself (the modem starts its channels late at a cold
    /// boot). Until the control channel answers, [`At::ready`] is false and commands are refused `busy`.
    pub async fn start(cfg: AtConfig, tracker: &TaskTracker, shutdown: CancellationToken) -> Result<At> {
        let same = |a: &PathBuf, b: &Option<PathBuf>| b.as_ref() == Some(a);
        if same(&cfg.control, &cfg.poll) || same(&cfg.control, &cfg.urc) || (cfg.poll.is_some() && cfg.poll == cfg.urc)
        {
            // two readers on one SIPC channel each get part of every reply (docs/research/FINDINGS.md 13b)
            return Err(Error::new(Code::BadValue, "AT channels: each device may be opened once"));
        }
        keeper::configure(cfg.keeper.clone());
        let (urcs, _) = broadcast::channel(URC_BACKLOG);
        let (ready_tx, ready) = watch::channel(false);
        let live = Arc::new(Liveness::new());
        let (control, control_rx) = mpsc::channel(QUEUE_MAX);
        let (fallback, fallback_rx) = mpsc::channel(QUEUE_MAX);
        let poll_up = Arc::new(AtomicBool::new(false));

        tracker.spawn(
            Channel {
                name: "control",
                source: Source::Control,
                path: cfg.control,
                primary: control_rx,
                secondary: Some(fallback_rx),
                health: Health::Watch(ready_tx),
                urcs: urcs.clone(),
                live: live.clone(),
                tracker: tracker.clone(),
                shutdown: shutdown.clone(),
            }
            .run(),
        );
        let poll = cfg.poll.map(|path| {
            let (tx, rx) = mpsc::channel(QUEUE_MAX);
            tracker.spawn(
                Channel {
                    name: "poll",
                    source: Source::Poll,
                    path,
                    primary: rx,
                    secondary: None,
                    health: Health::Flag(poll_up.clone()),
                    urcs: urcs.clone(),
                    live: live.clone(),
                    tracker: tracker.clone(),
                    shutdown: shutdown.clone(),
                }
                .run(),
            );
            tx
        });
        if let Some(path) = cfg.urc {
            tracker.spawn(urc::UrcChannel { path, urcs: urcs.clone(), live, tracker: tracker.clone(), shutdown }.run());
        }
        Ok(At { inner: Arc::new(Inner { control, poll, poll_up, fallback, urcs, ready }) })
    }

    /// One command. `busy` when the lane stays taken longer than `opts.queue_wait`.
    pub async fn send(&self, cmd: &str, opts: SendOpts) -> Result<Reply> {
        check_line("AT command", cmd, COMMAND_MAX)?;
        check_timeout(opts.timeout)?;
        let lane = match opts.lane {
            Lane::Control => &self.inner.control,
            Lane::Poll => match &self.inner.poll {
                Some(p) if self.inner.poll_up.load(Ordering::Relaxed) => p,
                _ => &self.inner.fallback,
            },
        };
        let (reply, rx) = oneshot::channel();
        let job = Job::Command(Command { cmd: cmd.to_owned(), body: None, timeout: opts.timeout, reply });
        submit(lane, job, rx, opts.queue_wait).await?
    }

    /// The control channel to ourselves until the session is dropped. `busy` after `wait`.
    ///
    /// Nothing else reaches the modem on the control channel in between (polls that share it wait too). A session
    /// that sends nothing for two minutes is ended by the channel task; its later commands then fail `busy`.
    pub async fn session(&self, owner: &'static str, wait: Duration) -> Result<AtSession> {
        let (reply, rx) = oneshot::channel();
        let tx = submit(&self.inner.control, Job::Session { owner, reply }, rx, wait).await??;
        Ok(AtSession { owner, tx })
    }

    /// Every unsolicited line from now on.
    pub fn urcs(&self) -> broadcast::Receiver<Urc> {
        self.inner.urcs.subscribe()
    }

    /// Whether the modem answers on the control channel (false while it restarts).
    pub fn ready(&self) -> watch::Receiver<bool> {
        self.inner.ready.clone()
    }
}

/// Exclusive use of the control channel; released when dropped.
#[derive(Debug)]
pub struct AtSession {
    owner: &'static str,
    tx: mpsc::Sender<Command>,
}

impl AtSession {
    pub async fn send(&mut self, cmd: &str, timeout: Duration) -> Result<Reply> {
        check_line("AT command", cmd, COMMAND_MAX)?;
        self.run(cmd, None, timeout).await
    }

    /// A command that answers with the "> " prompt, then `body` and Ctrl-Z (AT+CMGS with a PDU).
    ///
    /// The body is sent only once the prompt came; without it within 5 s the submission is aborted with ESC, so the
    /// modem is never left waiting for a body (docs/research/FINDINGS.md 46).
    pub async fn send_with_body(&mut self, cmd: &str, body: &str, timeout: Duration) -> Result<Reply> {
        check_line("AT command", cmd, COMMAND_MAX)?;
        check_line("body", body, BODY_MAX)?;
        self.run(cmd, Some(body.to_owned()), timeout).await
    }

    /// Who holds the session.
    pub fn owner(&self) -> &'static str {
        self.owner
    }

    async fn run(&mut self, cmd: &str, body: Option<String>, timeout: Duration) -> Result<Reply> {
        check_timeout(timeout)?;
        let (reply, rx) = oneshot::channel();
        let c = Command { cmd: cmd.to_owned(), body, timeout, reply };
        self.tx.send(c).await.map_err(|_| channel::session_ended())?;
        rx.await.map_err(|_| channel::session_ended())?
    }
}
