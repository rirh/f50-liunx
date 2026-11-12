//! adb over USB to this Linux: adbd (`/opt/mu300/bin/adbd`) in the USB gadget, supervised by a task of the daemon.
//!
//! FunctionFS unbinds the whole gadget whenever adbd closes its control endpoint, and adbd does that at the end of
//! every adb connection (a computer that unplugs, sleeps or resets its USB): Android's init then binds the gadget
//! again; here the supervisor does, with adb when adbd is ready and without it otherwise, so the network and the
//! console never stay away. Not in the host role: the gadget is left alone. A look every second while something is
//! under way, every 3 s while all is as it should be (FINDINGS 235). Early in a boot the FunctionFS module cannot
//! be had yet (13 s after power-on on an F50; a minute later it works, FINDINGS 82): asked again for a minute.
//! adbd itself has no listener but USB in either auth mode. Ported from `mu300-adbd run` and `init.d/mu300-adbd`.

mod auth;
mod gadget;

use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::rpc::Params;
use f50_core::{Error, Result};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{Net, lock};

use gadget::Rebind;

/// adb in one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AdbState {
    Off,
    /// No adbd, or no FunctionFS.
    Unavailable,
    Open,
    Keys,
}

pub(crate) fn wanted(net: &Net) -> bool {
    auth::on(net)
}

pub(crate) fn linked(net: &Net) -> bool {
    gadget::linked(net)
}

pub(crate) fn state(net: &Net) -> AdbState {
    if !auth::on(net) {
        AdbState::Off
    } else if gadget::usable(net).is_err() {
        AdbState::Unavailable
    } else if auth::read(net).0 == "open" {
        AdbState::Open
    } else {
        AdbState::Keys
    }
}

/// For `f50.usb get`.
pub(crate) fn summary(net: &Net) -> Value {
    let (mode, why) = auth::read(net);
    json!({
        "state": state(net),
        "on": auth::on(net),
        "damaged": auth::damaged(net),
        "auth": mode,
        "auth_reason": why,
        "keys": auth::key_count(net),
        "in_gadget": gadget::linked(net),
        "running": lock(&net.inner.adbd).is_some() || any_adbd(&net.platform().paths().sys("/proc")),
    })
}

/// An adbd this daemon did not start (one an older init script or a hand started, with `f50d.main.adbd` off).
fn any_adbd(proc_dir: &std::path::Path) -> bool {
    let Ok(rd) = std::fs::read_dir(proc_dir) else { return false };
    rd.filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
        .any(|e| std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim_end() == "adbd"))
}

/// `f50.usb adb`: on/off (kept across reboots and updates) and whom it lets in. The USB connection drops for a
/// moment either way (the gadget is rebound), so the change comes after the reply.
pub(crate) async fn set(net: &Net, p: &Params) -> Result<Value> {
    let st = p.str("state");
    let au = p.str("auth");
    if st.is_none() && au.is_none() {
        return Err(Error::missing("state"));
    }
    if st == Some("on") {
        gadget::usable(net)?;
    }
    if let Some(a) = au {
        auth::set_auth(net, a)?;
    }
    if let Some(s) = st {
        auth::set_on(net, s == "on")?;
    }
    let restart = st.is_some() || lock(&net.inner.adbd).is_some();
    if restart {
        net.later("adbd-restart", Duration::from_millis(500), |net| async move {
            stop(&net);
            // the old supervisor takes adb out of the gadget first (under the rebind lock)
            tokio::time::sleep(Duration::from_secs(2)).await;
            start(&net);
            Ok(())
        });
    }
    let (mode, _) = auth::read(net);
    let exposure = if mode == "open" && auth::on(net) {
        "adb is open: every computer plugged into the USB port gets a root shell without a password"
    } else {
        "adb takes authorised keys only (or is off)"
    };
    Ok(json!({ "state": state(net), "auth": mode, "restart": restart, "note": exposure }))
}

/// `f50.usb adb_key_add`.
pub(crate) async fn key_add(net: &Net, p: &Params) -> Result<Value> {
    let added = auth::add_key(net, p.need_str("key")?)?;
    Ok(json!({ "added": added, "keys": auth::key_count(net), "auth": auth::read(net).0 }))
}

/// See [`Net::adbd_start`].
pub(crate) fn start(net: &Net) {
    if !auth::on(net) {
        if auth::damaged(net) {
            tracing::warn!("/etc/mu300/adb says neither on nor off: adb stays off");
        }
        return;
    }
    let stop = net.inner.shutdown.child_token();
    {
        let mut cur = lock(&net.inner.adbd);
        if cur.as_ref().is_some_and(|t| !t.is_cancelled()) {
            return;
        }
        *cur = Some(stop.clone());
    }
    let n = net.clone();
    // not net.spawn: the supervisor takes adb out of the gadget itself when it is stopped
    net.inner.tracker.spawn(async move {
        supervise(&n, &stop).await;
        gadget::teardown(&n).await;
        // ours is done: a newer supervisor has a token that is not cancelled
        stop.cancel();
        let mut cur = lock(&n.inner.adbd);
        if cur.as_ref().is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
            *cur = None;
        }
    });
}

/// See [`Net::adbd_stop`].
pub(crate) fn stop(net: &Net) {
    if let Some(t) = lock(&net.inner.adbd).take() {
        t.cancel();
    }
}

async fn supervise(net: &Net, stop: &tokio_util::sync::CancellationToken) {
    let mut waited = 0;
    while let Err(e) = gadget::usable(net) {
        waited += 1;
        if waited > 60 {
            tracing::warn!(error = %e, "adb cannot run here");
            return;
        }
        tokio::select! {
            () = stop.cancelled() => return,
            () = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
    if let Err(e) = gadget::prepare(net).await {
        tracing::warn!(error = %e, "adb: the gadget is not ready");
        return;
    }
    let (mode, why) = auth::read(net);
    if mode == "open" {
        tracing::warn!("adb auth open: every computer on the USB cable gets a root shell");
    } else {
        tracing::info!(reason = why.as_deref().unwrap_or("keys"), "adb auth keys: only the authorised keys get in");
        if auth::key_count(net) == 0 {
            tracing::warn!("adb: no key is authorised: nobody gets in");
        }
    }
    let adbd = net.path(&net.cfg().adbd);
    let mut exits: Vec<tokio::time::Instant> = Vec::new();
    loop {
        let run = Cmd::new(&adbd)
            .env("MU300_ADB_AUTH", mode)
            .timeout(Duration::from_secs(365 * 24 * 3600))
            .max_output(16 * 1024)
            .run();
        tokio::pin!(run);
        let (mut idle, mut cool) = (0u32, 0u32);
        // its first descriptors: up to 5 s
        tokio::select! {
            () = stop.cancelled() => return,
            r = &mut run => { tracing::warn!(status = ?r.ok().and_then(|o| o.status), "adbd exited at once"); }
            () = gadget::wait_ffs(net, 25) => {}
        }
        loop {
            let steady = idle == 0
                && cool == 0
                && (crate::usb::role::now(net).as_deref() == Some("host") || gadget::linked(net));
            let tick = if steady { Duration::from_secs(3) } else { Duration::from_secs(1) };
            tokio::select! {
                () = stop.cancelled() => return,
                r = &mut run => {
                    tracing::warn!(status = ?r.ok().and_then(|o| o.status), "adbd exited, restarting it");
                    break;
                }
                () = tokio::time::sleep(tick) => {}
            }
            look(net, &mut idle, &mut cool).await;
        }
        let now = tokio::time::Instant::now();
        exits.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        exits.push(now);
        if exits.len() >= 5 {
            tracing::warn!("adbd keeps exiting: adb is left out of the gadget");
            return;
        }
    }
}

/// One look at the gadget.
async fn look(net: &Net, idle: &mut u32, cool: &mut u32) {
    const GRACE: u32 = 5;
    if crate::usb::role::now(net).as_deref() == Some("host") {
        *idle = 0;
        return;
    }
    // somebody else is rebinding the gadget (the LAN hook): not now, the next look
    if net.inner.rebind.try_lock().is_err() {
        *idle = 0;
        return;
    }
    if !crate::usb::gadget::bound(net) {
        // FunctionFS took the gadget off the controller (an adb connection ended). Not at once: the LAN hook and
        // the lease check unbind it on purpose for a few seconds, and bind it again themselves.
        *idle += 1;
        if *idle >= GRACE {
            gadget::wait_ffs(net, 25).await;
            if gadget::rebind(net, true).await != Rebind::Bound {
                *cool = 30;
            }
            *idle = 0;
        }
        return;
    }
    *idle = 0;
    if *cool > 0 {
        // adb would not bind a moment ago: bound without it, and not unbound every second
        *cool -= 1;
        return;
    }
    if !gadget::linked(net) && gadget::ffs_ready(net) && gadget::rebind(net, true).await != Rebind::Bound {
        *cool = 30;
    }
}
