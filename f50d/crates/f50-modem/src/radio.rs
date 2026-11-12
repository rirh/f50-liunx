//! The radio lock and the radio sequences: the bring-up the vendor RIL does, the card's radio off and on, the
//! protocol-stack restart a lock apply needs, and the modem reset.
//!
//! Ported from `mobile-data` (`radio_on_locked`, `rf_on_locked`, `rf_off_locked`, `sim_reset`, `radio-locked`) and
//! `unisoc-modem/lock` (`sfun_restart`). Every function here expects the caller to hold the radio lock.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use f50_at::SendOpts;
use f50_core::{Error, Result};
use serde::Serialize;

use crate::RadioGuard;
use crate::lock::replay::{self, Phase};
use crate::shared::{Inner, lock};
use crate::sim;

/// How a bring-up ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RadioOn {
    /// `+CFUN: 1`.
    On,
    /// The card's radio is switched off in its settings: it stays off and nothing dials (FINDINGS 185).
    Disabled,
}

/// Takes the radio lock for `owner`, waiting up to `wait`; `busy` naming the holder after that.
pub(crate) async fn acquire(inner: &Arc<Inner>, owner: &'static str, wait: Duration) -> Result<RadioGuard> {
    match tokio::time::timeout(wait, inner.radio.clone().lock_owned()).await {
        Ok(g) => {
            *lock(&inner.radio_owner) = Some(owner);
            Ok(RadioGuard { _guard: g, owner: inner.radio_owner.clone() })
        }
        Err(_) => {
            let holder = lock(&inner.radio_owner).unwrap_or("another radio sequence");
            Err(Error::busy(format!("the radio is busy ({holder}); try again shortly")))
        }
    }
}

/// The card in use's CFUN (0, 1, 2 or 4), `None` when it does not answer.
pub(crate) async fn cfun(inner: &Inner) -> Option<u8> {
    let r = inner.ctl("AT+CFUN?", 4).await.ok()?;
    match r.value("+CFUN")?.trim() {
        "0" => Some(0),
        "1" => Some(1),
        "2" => Some(2),
        "4" => Some(4),
        _ => None,
    }
}

async fn nap(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

/// The card's protocol stack off on its own channel, `AT+SFUN=5`, as the vendor RIL switches a card's radio: the
/// stock RIL's isRadioOn reads CFUN 0 and 2 as off, the SIM stays powered (FINDINGS 202). A radio at its cold 0, or in
/// the flight mode (4) an earlier release left (FINDINGS 185), is off already.
pub(crate) async fn rf_off(inner: &Inner) -> Result<()> {
    if matches!(cfun(inner).await, Some(0 | 2 | 4)) {
        return Ok(());
    }
    let _ = inner.ctl("AT+SFUN=5", 30).await;
    match cfun(inner).await {
        Some(0 | 2) => {
            tracing::info!("radio off (AT+SFUN=5): switched off in the SIM's settings");
            Ok(())
        }
        _ => Err(Error::failed("AT+SFUN=5 was not read back")),
    }
}

/// The radio back: from an earlier release's flight mode `AT+CFUN=1`, from 0 or 2 (SFUN=5, or the cold 0) the
/// ordinary bring-up, whose `AT+SFUN=4` is the vendor's way back (FINDINGS 202).
pub(crate) async fn rf_on(inner: &Arc<Inner>) -> Result<()> {
    match cfun(inner).await {
        Some(1) => Ok(()),
        Some(4) => {
            let _ = inner.ctl("AT+CFUN=1", 30).await;
            for _ in 0..10 {
                if cfun(inner).await == Some(1) {
                    tracing::info!("radio on (AT+CFUN=1)");
                    return Ok(());
                }
                nap(2).await;
            }
            Err(Error::failed("the radio did not come back from flight mode"))
        }
        _ => bring_up(inner).await,
    }
}

/// `radio_on_locked`: the radio on for the card in use, or [`RadioOn::Disabled`] when its setting says off.
pub(crate) async fn radio_on(inner: &Arc<Inner>, _guard: &RadioGuard) -> Result<RadioOn> {
    handshake(inner).await?;
    let now = cfun(inner).await;
    // switched off in the SIM's settings: off it stays (the cold 0 as it is, a radio that is on: AT+SFUN=5)
    if !sim::radio_allowed(inner, inner.slot) {
        if now == Some(1) {
            let _ = rf_off(inner).await;
        }
        return Ok(RadioOn::Disabled);
    }
    match now {
        Some(1) => {}
        Some(4) => {
            // flight mode with the setting on (left by an earlier release's panel, or set by hand): back on
            tracing::info!("the radio is in flight mode (CFUN 4) and its setting is on: AT+CFUN=1");
            rf_on(inner).await?;
        }
        Some(_) => bring_up(inner).await?,
        None => return Err(Error::failed("no answer to AT+CFUN?: the radio was not switched on")),
    }
    radio_came_on(inner);
    Ok(RadioOn::On)
}

/// Bookkeeping once the radio is on: the late replay needs no more waiting, and the card is asked how it is.
pub(crate) fn radio_came_on(inner: &Arc<Inner>) {
    inner.early_pending.store(false, Ordering::SeqCst);
    inner.sim_check.notify_one();
}

/// Android's slot RIL sends `AT+SMMSWAP=0` as the main channel's first command; the same, once per boot (the marker
/// lives in the runtime directory, which a reboot empties), before CFUN and before any dial-up command (K57). No
/// answer ends this round: commands sent into a channel that has not finished its handshake do not help. An ERROR
/// is an answer from a live channel: logged, and the round goes on.
async fn handshake(inner: &Inner) -> Result<()> {
    let marker = inner.paths().run().join("ril-handshake");
    if marker.exists() {
        return Ok(());
    }
    let opts = SendOpts { queue_wait: Duration::from_secs(1), ..SendOpts::control(Duration::from_secs(4)) };
    let r = inner.at.send("AT+SMMSWAP=0", opts).await;
    match &r {
        Ok(reply) if reply.is_ok() => tracing::info!("RIL handshake done (AT+SMMSWAP=0)"),
        Ok(reply) if crate::shared::refused(&r).is_some() => {
            tracing::warn!(answer = ?reply.status, "the modem refused the RIL handshake (AT+SMMSWAP=0), going on");
        }
        _ => return Err(Error::failed("no valid answer to the RIL handshake (AT+SMMSWAP=0)")),
    }
    if f50_core::fsx::private_dir(&inner.paths().run()).is_ok() {
        let _ = f50_core::fsx::write_atomic(&marker, b"", 0o600);
    }
    Ok(())
}

/// From CFUN 0 or 2: the early lock replay (the one safe window: the handshake is done, the radio still off; K65),
/// then `AT+SFUN=4` and a wait for `+CFUN: 1`. The SFUN=2 + SFUN=4 power cycle is the last resort only: sent right
/// after SFUN=4 it reset a CP that was just coming up and cost a whole CP start (K57).
async fn bring_up(inner: &Arc<Inner>) -> Result<()> {
    handshake(inner).await?;
    if !replay::done(inner)
        && let Err(e) = replay::run(inner, Phase::Early).await
    {
        tracing::warn!(error = %e, "early lock replay unverified; late fallback remains armed");
    }
    tracing::info!("the AT channel answers and the radio is off; sending AT+SFUN=4");
    let t0 = Instant::now();
    let _ = inner.ctl("AT+SFUN=4", 20).await;
    let end = t0 + Duration::from_secs(30);
    let (mut silent, mut answered) = (0, false);
    while Instant::now() < end {
        match cfun(inner).await {
            Some(1) => {
                tracing::info!(secs = t0.elapsed().as_secs(), "radio on after AT+SFUN=4");
                return Ok(());
            }
            Some(_) => {
                answered = true;
                silent = 0;
            }
            None => silent += 1,
        }
        // a channel that has gone silent cannot carry the recovery commands either: a later round retries
        if silent >= 2 {
            return Err(Error::failed("the AT channel went silent after AT+SFUN=4"));
        }
        nap(2).await;
    }
    if !answered {
        return Err(Error::failed("no answer to AT+CFUN? after AT+SFUN=4"));
    }
    tracing::warn!("radio still off 30 s after AT+SFUN=4; last resort: power cycle (AT+SFUN=2, AT+SFUN=4)");
    let _ = inner.ctl("AT+SFUN=2", 5).await;
    let _ = inner.ctl("AT+SFUN=4", 20).await;
    match cfun(inner).await {
        Some(1) => Ok(()),
        _ => Err(Error::failed("the radio did not come on")),
    }
}

/// The protocol-stack restart after a lock write (`AT+SFUN=5`, then `AT+SFUN=4`: the sequence the official web UI
/// uses), so the new setting takes effect. Not done while the card's radio is switched off in its settings: that
/// would switch it on; the setting is in the modem and applies when the radio comes on (FINDINGS 185). One quick
/// radio check afterwards; the data watchdog covers the rest. Returns whether the stack was restarted.
pub(crate) async fn sfun_restart(inner: &Arc<Inner>, phase: Phase, log: &(dyn Fn(&str) + Sync)) -> bool {
    if !sim::radio_allowed(inner, inner.slot) {
        log(
            "the radio of this SIM is switched off in its settings: no stack restart, the setting applies when it is on",
        );
        return false;
    }
    let t0 = Instant::now();
    log("restarting the protocol stack (SFUN 5 -> 4)");
    let _ = inner.ctl("AT+SFUN=5", 10).await;
    nap(1).await;
    let _ = inner.ctl("AT+SFUN=4", 30).await;
    nap(3).await;
    if cfun(inner).await != Some(1) {
        // measured once, not the norm: nudge it with the radio_on sequence
        log("radio off after restart, running the radio-on recovery");
        let _ = inner.ctl("AT+SFUN=4", 20).await;
        nap(1).await;
        let _ = inner.ctl("AT+SFUN=2", 8).await;
        nap(1).await;
        let _ = inner.ctl("AT+SFUN=4", 30).await;
    }
    if phase == Phase::Late
        && let Err(e) = inner.platform.netifd().up(&inner.cfg.data_interface).await
    {
        tracing::warn!(error = %e, "the data interface could not be restarted after the stack restart");
    }
    let on = cfun(inner).await == Some(1);
    if on {
        radio_came_on(inner);
    }
    log(&format!("stack restarted ({}s), radio {}", t0.elapsed().as_secs(), if on { "on" } else { "checking" }));
    true
}

/// The modem reset (`mobile-data sim-reset`): the SFUN 5/3/2/4 sequence, then the card's answer (CPIN READY, up to a
/// minute). Refused while the card's radio is switched off in its settings: a reset would switch it on (FINDINGS 185).
pub(crate) async fn modem_reset(inner: &Arc<Inner>, log: &(dyn Fn(&str) + Sync)) -> Result<serde_json::Value> {
    if !sim::radio_allowed(inner, inner.slot) {
        return Err(Error::forbidden("the radio of this SIM is switched off in its settings"));
    }
    log("modem reset: AT+SFUN=5, AT+SFUN=3, AT+SFUN=2, AT+SFUN=4");
    let _ = inner.ctl("AT+SFUN=5", 20).await;
    let _ = inner.ctl("AT+SFUN=3", 10).await;
    nap(2).await;
    let _ = inner.ctl("AT+SFUN=2", 10).await;
    let _ = inner.ctl("AT+SFUN=4", 30).await;
    let mut ready = false;
    for _ in 0..30 {
        if let Ok(r) = inner.ctl("AT+CPIN?", 4).await
            && r.lines.iter().any(|l| l.contains("READY"))
        {
            ready = true;
            break;
        }
        nap(2).await;
    }
    let on = cfun(inner).await == Some(1);
    if on {
        radio_came_on(inner);
    }
    log(&format!("modem reset done: SIM {}, radio {}", if ready { "ready" } else { "not ready" }, on));
    Ok(serde_json::json!({ "sim_ready": ready, "radio_on": on }))
}
