//! The SIM cards: which slot this boot uses and which the next one will, each card's radio setting, and the card's
//! own state ([`state`]).
//!
//! Ported from `f50-sim` (F50 part), `sim-config.sh` and `f50-sim-radio`. The F50 picks its card at boot:
//! reopening the SIPC channels is not a hot switch, so a selection is saved for the next boot and the active slot
//! stays pinned for this one (FINDINGS 74). The U30 Air's live switch and cold start (`f50-sim2-*`), the V50's card
//! switches (FINDINGS 204) and the charger-boot CP loader (`f50-modem-coldboot`) are not carried over.

pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use f50_core::paths::Paths;
use f50_core::{Error, Result, fsx};
use serde_json::{Value, json};

pub use state::SimState;

use crate::radio;
use crate::shared::Inner;
use crate::{Slot, sim};

/// Where the slot for the next boot is kept (`1` or `2`).
fn saved_path(paths: &Paths) -> PathBuf {
    paths.state().join("sim-slot")
}

/// Where this boot's slot is pinned (RAM: a daemon restart keeps the boot's card, a reboot takes the saved one).
fn pinned_path(paths: &Paths) -> PathBuf {
    paths.run().join("sim-slot")
}

/// Each card's radio setting: one line `<SIM1> <SIM2>`, 1 on and 0 off (FINDINGS 185).
fn radio_path(paths: &Paths) -> PathBuf {
    paths.state().join("sim-radio")
}

fn parse_slot(s: &str) -> Option<Slot> {
    match s.trim() {
        "1" => Some(Slot::One),
        "2" => Some(Slot::Two),
        _ => None,
    }
}

/// The slot saved for the next boot. A value that is not 1 or 2 (hand-edited, half written) is SIM1: every AT owner
/// asks this at start, and failing here would leave the device without AT, data or SMS for the whole boot.
pub fn saved_slot(paths: &Paths) -> Slot {
    match fsx::read_opt(&saved_path(paths)) {
        Ok(Some(s)) => parse_slot(&s).unwrap_or_else(|| {
            tracing::warn!("the saved SIM slot is not 1 or 2; using SIM1 (the external card)");
            Slot::One
        }),
        _ => Slot::One,
    }
}

/// The slot this boot uses: pinned in the runtime directory the first time it is asked, from the saved choice.
/// f50d asks this before it opens the AT channels (the card's channel set) and `Modem::start` asks it again.
pub fn boot_slot(paths: &Paths) -> Slot {
    let pin = pinned_path(paths);
    if let Some(s) = fsx::read_line(&pin).as_deref().and_then(parse_slot) {
        return s;
    }
    let slot = saved_slot(paths);
    let written = fsx::private_dir(&paths.run())
        .and_then(|()| fsx::write_atomic(&pin, format!("{}\n", slot.number()).as_bytes(), 0o600));
    if let Err(e) = written {
        tracing::warn!(error = %e, "could not pin this boot's SIM slot");
    }
    slot
}

/// Saves the slot for the next boot.
pub(crate) fn save_slot(paths: &Paths, slot: Slot) -> Result<()> {
    fsx::private_dir(&paths.state())?;
    fsx::write_atomic(&saved_path(paths), format!("{}\n", slot.number()).as_bytes(), 0o600)
}

/// Both cards' radio settings. No file is both on, as every release before; a line that is not one of the four
/// counts as both on and says so: a broken file must not take the device off the network for good (FINDINGS 185).
pub fn radio_flags(paths: &Paths) -> [bool; 2] {
    let Some(line) = fsx::read_line(&radio_path(paths)) else { return [true, true] };
    match line.as_str() {
        "0 0" => [false, false],
        "0 1" => [false, true],
        "1 0" => [true, false],
        "1 1" => [true, true],
        _ => {
            tracing::warn!("the SIM radio setting is not \"<0|1> <0|1>\"; both radios count as on");
            [true, true]
        }
    }
}

fn save_radio_flags(paths: &Paths, flags: [bool; 2]) -> Result<()> {
    fsx::private_dir(&paths.state())?;
    let line = format!("{} {}\n", u8::from(flags[0]), u8::from(flags[1]));
    fsx::write_atomic(&radio_path(paths), line.as_bytes(), 0o600)
}

/// May `slot`'s radio be on? Every path that brings a radio up asks first.
pub(crate) fn radio_allowed(inner: &Inner, slot: Slot) -> bool {
    radio_flags(inner.paths())[usize::from(slot.index())]
}

/// Which cards a radio setting names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cards {
    One(Slot),
    All,
}

/// Saves a radio setting; true when it concerns the card in use (then the radio has to follow now).
pub(crate) fn set_radio(inner: &Inner, cards: Cards, on: bool) -> Result<bool> {
    let mut f = radio_flags(inner.paths());
    match cards {
        Cards::All => f = [on, on],
        Cards::One(s) => f[usize::from(s.index())] = on,
    }
    save_radio_flags(inner.paths(), f)?;
    Ok(matches!(cards, Cards::All) || cards == Cards::One(inner.slot))
}

/// `f50.sim get`: the selector's account, both cards' radio settings and the card's own state.
pub(crate) fn status(inner: &Inner) -> Value {
    let next = saved_slot(inner.paths());
    let f = radio_flags(inner.paths());
    json!({
        "active": inner.slot.number(),
        "default": next.number(),
        "slot2_supported": true,
        "live_switch": false,
        "reboot_required": next != inner.slot,
        "state": sim::state::current(inner),
        "radio": { "enabled": f, "all_off": f == [false, false] },
        "busy": inner.is_restarting(),
    })
}

/// `rf-apply`: the card in use's radio and its mobile data made what its setting says, now. Off: the data
/// interface restarted first (its dial then ends because the radio may not come on; netifd does not retry), then
/// `AT+SFUN=5`. On: the radio back, then the interface made available again (netifd starts it unless somebody took
/// it down). The caller holds the radio lock.
pub(crate) async fn rf_apply(inner: &Arc<Inner>, log: &(dyn Fn(&str) + Sync)) -> Result<Value> {
    let iface = inner.cfg.data_interface.as_str();
    if !radio_allowed(inner, inner.slot) {
        let st = inner.platform.netifd().status(iface).await.ok().flatten();
        let held = st.as_ref().is_some_and(|s| s["autostart"] == json!(true) || s["up"] == json!(true));
        if held && let Err(e) = inner.platform.netifd().up(iface).await {
            tracing::warn!(error = %e, "the data interface could not be restarted for the radio off");
        }
        radio::rf_off(inner).await?;
        log("radio off (saved)");
        return Ok(json!({ "radio": "off" }));
    }
    radio::rf_on(inner).await?;
    radio::radio_came_on(inner);
    wan_available(inner).await;
    log("radio on (saved)");
    Ok(json!({ "radio": "on" }))
}

/// The data interface made available again where the dial made it unavailable (a radio switched off, an empty
/// slot: FINDINGS 185, 186). netifd then starts it by itself, unless somebody took it down.
pub(crate) async fn wan_available(inner: &Inner) {
    let Some(ubus) = inner.platform.ubus() else { return };
    let iface = &inner.cfg.data_interface;
    let st = inner.platform.netifd().status(iface).await.ok().flatten();
    if st.is_some_and(|s| s["available"] == json!(false)) {
        let object = format!("network.interface.{iface}");
        let args = json!({ "action": 5, "available": true });
        if let Err(e) = ubus.call(&object, "notify_proto", &args, std::time::Duration::from_secs(5)).await {
            tracing::warn!(error = %e, "the data interface could not be made available again");
        }
    }
}

/// A radio switch the caller asked for, refused while it cannot be done.
pub(crate) fn check_radio_change(inner: &Inner) -> Result<()> {
    if inner.is_restarting() {
        return Err(Error::busy("a radio change is already running"));
    }
    Ok(())
}
