//! The IPA delegate (`sipa-dele-start`): loaded once the modem is attached to the packet domain, and never before.
//!
//! The delegate is what makes downlink happen at all: the modem asks permission over SIPC channel 120 before it sends
//! anything, and without an answer `rx_packets` stays at zero for the whole boot (FINDINGS 13f). Its thread opens the
//! channel as its first act and gives up for good if the modem is not there yet, and the module is `[permanent]`, so
//! there is no second chance. Too late is as bad as too early (K45-K47): inserted after `AT+CGACT` made the SIPA
//! resources active, its probe hit a reproducible CFI panic. So the dial loads it between the registration and the PDP
//! context, and only once `AT+CGATT?` says `1` - a bare AT answer or a dmesg line comes long before the CP's data
//! path exists. The modem's answer is the caller's (it owns the AT channel): [`load`] takes it as a closure.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use f50_core::exec::Cmd;
use f50_core::{Error, Result};
use serde::Serialize;

/// The delegate is in and its channel may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegateState {
    /// Its channel-120 handshake with the CP is done and the resource callbacks are in place (FINDINGS 241).
    Ready,
    /// Loaded, by a module that cannot say whether its channel is open (an older one, the 5.4 vendor one).
    Loaded,
    /// This kernel has no delegate module (no modem stack): nothing to wait for.
    NoModule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ready {
    Yes,
    No,
    /// No `sipa_dele_wait` (or more than one): an older module.
    Unknown,
}

/// What the delegate needs from the system.
#[derive(Debug, Clone)]
pub(crate) struct Delegate {
    pub ko: Vec<PathBuf>,
    pub sysfs: PathBuf,
    pub proc_modules: PathBuf,
    pub wait: Duration,
}

impl Delegate {
    fn loaded(&self) -> bool {
        std::fs::read_to_string(&self.proc_modules).is_ok_and(|s| s.lines().any(|l| l.starts_with("sipa_dele ")))
    }

    /// The delegate's own word on its channel: reading `sipa_dele_wait` waits on the kernel's queue until the
    /// handshake is done (at once when it is, at most 3 s otherwise).
    async fn ready(&self) -> Ready {
        let dir = self.sysfs.clone();
        tokio::task::spawn_blocking(move || {
            let nodes: Vec<PathBuf> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .filter_map(|e| Some(e.ok()?.path().join("sipa_dele_wait")))
                .filter(|p| p.is_file())
                .collect();
            let [node] = nodes.as_slice() else { return Ready::Unknown };
            if f50_core::fsx::read_line(node).as_deref() == Some("1") {
                return Ready::Yes;
            }
            let state = node.parent().and_then(|d| f50_core::fsx::read_line(&d.join("sipa_dele_ready")));
            tracing::warn!("delegate handshake not done (sipa_dele_ready: {})", state.as_deref().unwrap_or("?"));
            Ready::No
        })
        .await
        .unwrap_or(Ready::Unknown)
    }

    fn not_ready() -> Error {
        Error::busy("the IPA delegate's channel is not open: no PDP context now")
    }

    /// The module file of this kernel.
    fn ko(&self) -> Option<&Path> {
        self.ko.iter().find(|p| p.is_file()).map(PathBuf::as_path)
    }
}

async fn dmesg_has(text: &str) -> bool {
    Cmd::new("/bin/dmesg").ok().await.is_ok_and(|s| s.contains(text))
}

/// Loads the delegate once `attached()` says the modem is attached to the packet domain. `Err` (busy, or failed):
/// not loaded, or loaded with its channel not open - the PDP context must not be activated; the dial's next attempt
/// asks again.
pub(crate) async fn load<F, Fut>(d: &Delegate, mut attached: F) -> Result<DelegateState>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    if d.loaded() {
        return match d.ready().await {
            Ready::Yes => Ok(DelegateState::Ready),
            Ready::Unknown => Ok(DelegateState::Loaded),
            Ready::No => Err(Delegate::not_ready()),
        };
    }
    let Some(ko) = d.ko().map(Path::to_path_buf) else { return Ok(DelegateState::NoModule) };
    let start = Instant::now();
    loop {
        if attached().await {
            tracing::info!("packet domain attached after {}s", start.elapsed().as_secs());
            Cmd::new("/sbin/insmod").arg(&ko).ok().await.map_err(|e| e.context("sipa-dele"))?;
            match d.ready().await {
                Ready::Yes => return Ok(DelegateState::Ready),
                Ready::No => return Err(Delegate::not_ready()),
                Ready::Unknown => {}
            }
            // an older module: both halves of its handshake are pr_info; looked for every 0.1 s for up to 3 s
            const OPEN: &str = "channel 5-120 send open msg";
            for _ in 0..30 {
                if dmesg_has(OPEN).await {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if !dmesg_has(OPEN).await {
                tracing::warn!("no '{OPEN}' in dmesg - the delegate thread did not start");
            }
            return Ok(DelegateState::Loaded);
        }
        // one round is an AT answer (up to 8 s) plus the pause: a deadline, not a count
        if start.elapsed() + Duration::from_secs(2) >= d.wait {
            return Err(Error::timeout(format!(
                "the packet domain (not attached after {}s; the delegate would race the PDP context)",
                d.wait.as_secs()
            )));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
