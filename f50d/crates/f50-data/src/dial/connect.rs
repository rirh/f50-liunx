//! The steps of a dial that talk to the modem: the registration wait, the IPA delegate, the activation and its
//! address, the data mode.

use std::path::PathBuf;
use std::time::Duration;

use f50_at::{At, Final, SendOpts};
use f50_core::{Error, Result};
use f50_modem::{Modem, Slot};
use f50_platform::Platform;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, sleep, timeout_at};

use super::address::{first_contrdp, iid_of_cgpaddr};
use crate::radio::{self, reg_stat};

/// Registered (home or roaming) within `wait`. nr0 announces every change at once (`+CEREG: <stat>[,...]`, the
/// unsolicited form, whose first field is the stat); `AT+CEREG?` every 5 s is the fallback for a modem that announces
/// nothing (the query form, whose second field is). A reconnect after a lost context is registered all along: the
/// first query goes at once. A radio sequence of the modem service (a lock apply, a SIM switch) ends the wait.
pub(crate) async fn wait_registered(at: &At, modem: &Modem, wait: Duration) -> bool {
    let mut urcs = at.urcs();
    let end = Instant::now() + wait;
    let mut next_query = Instant::now();
    loop {
        if modem.restarting() {
            tracing::info!("the radio is being reconfigured: this dial ends");
            return false;
        }
        if Instant::now() >= next_query {
            if radio::registered(reg_stat(at, "AT+CEREG?").await) {
                return true;
            }
            next_query = Instant::now() + Duration::from_secs(5);
        }
        let until = next_query.min(end);
        match timeout_at(until, urcs.recv()).await {
            Ok(Ok(u)) => {
                if let Some(v) = u.line.strip_prefix("+CEREG:") {
                    let stat = v.split(',').next().and_then(|s| s.trim().parse::<u8>().ok());
                    if radio::registered(stat) {
                        return true;
                    }
                }
            }
            Ok(Err(RecvError::Lagged(_))) => next_query = Instant::now(),
            Ok(Err(RecvError::Closed)) => sleep(until.saturating_duration_since(Instant::now())).await,
            Err(_) => {}
        }
        if Instant::now() >= end {
            return false;
        }
    }
}

/// What the delegate's sysfs node says (`sipa-dele-start`'s ready: one `sipa_dele_wait` node, 1 when its channel to
/// the CP is open). A module that has no such node (an older build) cannot say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Readiness {
    Ready,
    NotReady,
    CannotSay,
}

fn readiness(platform: &Platform) -> Readiness {
    let dir = platform.paths().sys("/sys/bus/platform/drivers/sipa_delegate");
    let nodes: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path().join("sipa_dele_wait")).filter(|p| p.exists()).collect())
        .unwrap_or_default();
    match nodes.as_slice() {
        [one] => {
            if f50_core::fsx::read_line(one).as_deref() == Some("1") {
                Readiness::Ready
            } else {
                Readiness::NotReady
            }
        }
        _ => Readiness::CannotSay,
    }
}

/// The IPA delegate (`sipa_dele`), before the context makes the SIPA resources active (K47; FINDINGS 13f: without
/// it the modem never sends downlink). It has to probe once the CP's packet domain exists (`+CGATT: 1`) and before
/// AT+CGACT: loaded beside the dial it raced the activation (a reproducible CFI panic in the delegate). Once loaded it
/// stays; a reconnect only asks whether its channel is open. A kernel without the module dials without it.
pub(crate) async fn delegate(at: &At, platform: &Platform) -> Result<()> {
    let modules = platform.modules();
    if modules.loaded("sipa_dele") {
        return match readiness(platform) {
            Readiness::NotReady => Err(Error::failed("the IPA delegate's channel is not open: no PDP context yet")),
            _ => Ok(()),
        };
    }
    let rel = platform.sysfs().read("/proc/sys/kernel/osrelease").unwrap_or_default();
    let base = platform.paths().sys(format!("/lib/modules/{rel}"));
    if !base.join("sipa-dele.ko").exists() && !base.join("extra/sipa-dele.ko").exists() {
        tracing::info!("no sipa-dele.ko in this kernel ({rel}): dialling without the IPA delegate");
        return Ok(());
    }
    let end = Instant::now() + Duration::from_secs(20);
    while !radio::attached(at).await {
        if Instant::now() >= end {
            return Err(Error::failed("the packet domain is not attached: the IPA delegate is not loaded yet"));
        }
        sleep(Duration::from_secs(1)).await;
    }
    modules.load("sipa_dele", &[]).await.map_err(|e| e.context("the IPA delegate (sipa-dele) did not load"))?;
    // a freshly loaded delegate must say ready (or be one that cannot say)
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        match readiness(platform) {
            Readiness::Ready | Readiness::CannotSay => return Ok(()),
            Readiness::NotReady if Instant::now() >= end => {
                return Err(Error::failed("the IPA delegate loaded but its channel did not open"));
            }
            Readiness::NotReady => sleep(Duration::from_millis(200)).await,
        }
    }
}

/// The context's IPv4 bearer within `budget`: an active context is not an address yet (measured: CGACT up and no
/// address for 40 s on NR). No query starts after the budget.
async fn fetch_addr(at: &At, cid: u8, budget: Duration) -> Option<(String, u8, Vec<String>)> {
    let end = Instant::now() + budget;
    loop {
        if let Ok(r) = at.send(&format!("AT+CGCONTRDP={cid}"), SendOpts::control(Duration::from_secs(4))).await
            && let Some(b) = first_contrdp(r.values("+CGCONTRDP"))
        {
            return Some(b);
        }
        if Instant::now() + Duration::from_secs(2) >= end {
            return None;
        }
        sleep(Duration::from_secs(2)).await;
    }
}

/// AT+CGACT=1 and the address. No AT+CGACT? first (at boot the context is off; CGACT=1 on an active one changes
/// nothing). Without an address the activation is reasserted once - idempotent, it nudges the context - never
/// CGACT=0/1: taking down the context just created silenced the AT channel for minutes.
pub(crate) async fn activate(at: &At, cid: u8) -> Result<(String, u8, Vec<String>)> {
    at.send(&format!("AT+CGACT=1,{cid}"), SendOpts::control(Duration::from_secs(20))).await?;
    if let Some(b) = fetch_addr(at, cid, Duration::from_secs(8)).await {
        return Ok(b);
    }
    tracing::info!("context active but no address after 8 s; reasserting AT+CGACT=1 once");
    let _ = at.send(&format!("AT+CGACT=1,{cid}"), SendOpts::control(Duration::from_secs(8))).await;
    fetch_addr(at, cid, Duration::from_secs(8)).await.ok_or_else(|| Error::failed("no IPv4 address from AT+CGCONTRDP"))
}

/// The data mode, and the internal card's packet-data route. AT+CGDATA answers `^ORIG` and then CONNECT ten to
/// twenty seconds later, hence its budget (with eight seconds the next command read its reply). On the internal card
/// CONNECT can come while every packet is lost: the stock RIL selects the route with AT+SPSWDATA.
pub(crate) async fn data_mode(at: &At, cid: u8, slot: Slot) -> Result<()> {
    let r = at.send(&format!("AT+CGDATA=\"M-ETHER\",{cid}"), SendOpts::control(Duration::from_secs(45))).await;
    let connect = r.as_ref().is_ok_and(|r| {
        matches!(&r.status, Final::Other(s) if s.starts_with("CONNECT"))
            || r.lines.iter().any(|l| l.contains("CONNECT"))
    });
    if !connect {
        if slot == Slot::Two {
            return Err(Error::failed("AT+CGDATA did not answer CONNECT"));
        }
        tracing::warn!("AT+CGDATA did not answer CONNECT; going on (the tray card always did)");
    }
    if slot == Slot::Two {
        let r = at.send("AT+SPSWDATA", SendOpts::control(Duration::from_secs(8))).await?;
        if !r.is_ok() {
            return Err(Error::failed("could not select the internal SIM's packet-data route (AT+SPSWDATA)"));
        }
    }
    Ok(())
}

/// The interface identifier of the context's IPv6 address, if the network assigned one.
pub(crate) async fn iid(at: &At, cid: u8) -> Option<String> {
    let r = at.send(&format!("AT+CGPADDR={cid}"), SendOpts::control(Duration::from_secs(4))).await.ok()?;
    r.values("+CGPADDR").find_map(iid_of_cgpaddr)
}
