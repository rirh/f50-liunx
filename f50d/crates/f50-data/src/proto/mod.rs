//! netifd's `f50cell` protocol, on the daemon's side.
//!
//! `/lib/netifd/proto/f50cell.sh` is thin: its setup, teardown and renew run `f50d proto setup|teardown|renew
//! <config> [<ifname>]`, and that command (a short-lived process of the same binary, which must not touch the AT
//! channels the daemon owns) asks the daemon to do the work. **What the binary must do:** register
//! [`Data::proto_object`] (`f50.proto`) on ubus from the daemon *without* putting it in any rpcd ACL (uhttpd's
//! `/ubus` then refuses it to every web login; netifd's script runs as root on the local bus), and make the CLI call
//! `ubus call f50.proto setup {"config":..,"iface":..}` (and `teardown`, `renew`) and exit non-zero when the daemon
//! cannot be reached. Nothing secret crosses that boundary: the daemon reads the APN settings from UCI itself
//! (FINDINGS 65: the password never reaches an argument list).
//!
//! - **setup** returns as soon as the dial is started. The handler declares `no_proto_task`, so netifd waits in its
//!   setup state until the daemon reports: the update (link up), or an error and then `setup_failed` after the old
//!   handler's pause (20 s; 60 s for the roaming hold), or unavailable for an empty slot, or a blocked restart when
//!   the kernel has no modem. A second setup replaces the first.
//! - **teardown** stops the dial or the relay monitor, flushes the bearer's addresses and routes (netifd removes no
//!   external state), and deactivates the context in the background. It returns within a few seconds: netifd's
//!   teardown waits for the script.
//! - **renew** asks the relay monitor to read the bearer again (a no-op outside relay mode).

mod up;

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use f50_at::SendOpts;
use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::{Error, Result};
use serde_json::json;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::Data;
use crate::dial::{self, Outcome};
use crate::kernel::Kernel;
use crate::settings::{Pdp, WanSettings};
use crate::state::Hold;

/// The pause before netifd's next attempt after a failed dial (the old handler's `sleep 20`).
const RETRY: Duration = Duration::from_secs(20);
/// Held for roaming: netifd asks again a minute later; only the registration is read then, no context activated.
const ROAM_RETRY: Duration = Duration::from_secs(60);

const NAME_RE: &str = "[A-Za-z0-9_]{1,15}";
const IFNAME_RE: &str = "[A-Za-z0-9_.@-]{0,32}";

const SETUP: &[Field] = &[
    Field::req("config", Rule::Word { re: NAME_RE, max: 15 }),
    Field::opt("iface", Rule::Word { re: IFNAME_RE, max: 32 }),
];
const RENEW: &[Field] = &[Field::req("config", Rule::Word { re: NAME_RE, max: 15 })];

#[derive(Debug)]
struct Session {
    config: String,
    cancel: CancellationToken,
    task: JoinHandle<()>,
    renew: Arc<Notify>,
}

/// The one running protocol session (a dial, then the relay monitor).
#[derive(Debug, Default)]
pub(crate) struct Sessions {
    current: Mutex<Option<Session>>,
}

impl Sessions {
    fn take(&self) -> Option<Session> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    fn put(&self, s: Session) -> Option<Session> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner).replace(s)
    }

    fn renew(&self, config: &str) -> bool {
        let g = self.current.lock().unwrap_or_else(PoisonError::into_inner);
        g.as_ref().filter(|s| s.config == config).map(|s| s.renew.notify_one()).is_some()
    }
}

/// Stops a session and waits a moment for it; a task that does not end in time is aborted.
async fn stop(s: Session) {
    s.cancel.cancel();
    let abort = s.task.abort_handle();
    if tokio::time::timeout(Duration::from_secs(3), s.task).await.is_err() {
        abort.abort();
    }
}

impl Data {
    /// netifd's setup of `config`: the dial starts in the background and reports to netifd itself.
    pub async fn proto_setup(&self, config: &str, iface: &str) -> Result<()> {
        let inner = &self.inner;
        if inner.shutdown.is_cancelled() {
            return Err(Error::busy("the daemon is stopping"));
        }
        tracing::info!(config, iface, "f50cell setup");
        if let Some(old) = inner.session.take() {
            stop(old).await;
        }
        self.state().connecting(config);
        let cancel = inner.shutdown.child_token();
        let renew = Arc::new(Notify::new());
        let task = inner.tracker.spawn(session(self.clone(), config.to_owned(), cancel.clone(), renew.clone()));
        if let Some(raced) = inner.session.put(Session { config: config.to_owned(), cancel, task, renew }) {
            stop(raced).await;
        }
        Ok(())
    }

    /// netifd's teardown of `config`: the session stops and the bearer's kernel state goes; the context is
    /// deactivated in the background (a courtesy to the network: the next dial redefines it anyway).
    pub async fn proto_teardown(&self, config: &str, iface: &str) -> Result<()> {
        tracing::info!(config, iface, "f50cell teardown");
        if let Some(s) = self.inner.session.take() {
            stop(s).await;
        }
        let dev = {
            let mut st = self.state();
            let dev = st.bearer.as_ref().map(|b| b.device.clone());
            st.down();
            dev
        };
        let dev = dev.unwrap_or_else(|| self.device());
        let k = Kernel::new(self.platform());
        k.flush_all(&dev).await;
        k.ipv6(&dev, false);
        let data = self.clone();
        let stop = self.inner.shutdown.clone();
        self.inner.tracker.spawn(async move {
            tokio::select! {
                () = stop.cancelled() => {}
                () = deactivate(&data) => {}
            }
        });
        Ok(())
    }

    /// netifd's renew: the relay monitor reads the bearer again (nothing else to renew: no CGACT cycle, no link
    /// restart).
    pub async fn proto_renew(&self, config: &str) -> Result<()> {
        if !self.inner.session.renew(config) {
            tracing::debug!(config, "renew without a session");
        }
        Ok(())
    }

    /// `f50.proto`: setup, teardown and renew for the binary's `f50d proto` command. Not among [`Data::objects`] and
    /// never in an ACL (see the module documentation).
    pub fn proto_object(&self) -> Object {
        let (a, b, c) = (self.clone(), self.clone(), self.clone());
        Object::builder("f50.proto")
            .method("setup", Access::Write, SETUP, "netifd's f50cell setup: starts the dial.", move |p: Params| {
                let d = a.clone();
                async move {
                    d.proto_setup(p.need_str("config")?, p.str("iface").unwrap_or("")).await?;
                    Ok(json!({}))
                }
            })
            .method("teardown", Access::Write, SETUP, "netifd's f50cell teardown.", move |p: Params| {
                let d = b.clone();
                async move {
                    d.proto_teardown(p.need_str("config")?, p.str("iface").unwrap_or("")).await?;
                    Ok(json!({}))
                }
            })
            .method("renew", Access::Write, RENEW, "netifd's f50cell renew.", move |p: Params| {
                let d = c.clone();
                async move {
                    d.proto_renew(p.need_str("config")?).await?;
                    Ok(json!({}))
                }
            })
            .build()
    }
}

/// AT+CGACT=0 with a short wait for the radio lock: a dial or a radio sequence that holds it owns the channel, and
/// asking now would only make both wait.
async fn deactivate(data: &Data) {
    let Ok(_guard) = data.modem().radio_guard("data.teardown", Duration::from_secs(2)).await else { return };
    let cmd = format!("AT+CGACT=0,{}", data.config().cid);
    let _ = data.at().send(&cmd, SendOpts::control(Duration::from_secs(5))).await;
}

/// One protocol session: the dial, its report to netifd, and in relay mode the monitor until the teardown.
async fn session(data: Data, config: String, cancel: CancellationToken, renew: Arc<Notify>) {
    tokio::select! {
        () = cancel.cancelled() => {}
        () = run(&data, &config, &cancel, &renew) => {}
    }
}

async fn run(data: &Data, config: &str, cancel: &CancellationToken, renew: &Notify) {
    let slot = data.modem().active_slot();
    let n = data.notifier();
    let outcome = match WanSettings::load(data.platform(), config, slot).await {
        Ok(settings) => {
            let relay = settings.relay && settings.card.pdp_or_default(slot) != Pdp::Ipv4;
            match dial::dial(data, &settings).await {
                Outcome::Up(bearer) => match up::bring_up(data, config, &bearer, relay).await {
                    Ok(()) => {
                        tracing::info!(config, address = %bearer.cidr(), device = %bearer.device, "connected");
                        data.state().connected(bearer.clone(), slot, relay);
                        if relay {
                            crate::relay::monitor::run(data, config, bearer, renew, cancel).await;
                        }
                        return;
                    }
                    Err(e) => Outcome::Failed(e),
                },
                o => o,
            }
        }
        Err(e) => Outcome::Failed(e.context("network settings")),
    };
    let report = async {
        match outcome {
            Outcome::Up(_) => Ok(()),
            Outcome::NoModem => {
                tracing::warn!("no modem stack in this kernel: the interface is not retried");
                data.state().failed("no modem stack in this kernel");
                n.error(config, "NO_MODEM").await?;
                n.block_restart(config).await?;
                n.setup_failed(config).await
            }
            Outcome::Held(hold) => {
                data.state().held(hold);
                match hold {
                    Hold::Roaming => {
                        tracing::info!("roaming with data roaming off: mobile data stays down");
                        n.error(config, "DATA_ROAMING").await?;
                        tokio::time::sleep(ROAM_RETRY).await;
                        n.setup_failed(config).await
                    }
                    // unavailable: no retry every 20 s; the watchdog makes it available when a card answers, and
                    // netifd starts it then unless somebody took it down
                    Hold::SimMissing => {
                        n.error(config, "SIM_MISSING").await?;
                        n.set_available(config, false).await
                    }
                    Hold::RadioOff => {
                        n.error(config, "RADIO_OFF").await?;
                        tokio::time::sleep(RETRY).await;
                        n.setup_failed(config).await
                    }
                }
            }
            Outcome::Failed(e) => {
                tracing::warn!(error = %e, "attach failed");
                data.state().failed(e.message());
                n.error(config, "ATTACH_FAILED").await?;
                tokio::time::sleep(RETRY).await;
                n.setup_failed(config).await
            }
        }
    };
    if let Err(e) = report.await {
        tracing::warn!(error = %e, "could not report to netifd");
    }
}
