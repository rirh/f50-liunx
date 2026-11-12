//! One dial: from "netifd wants the bearer" to an address on the device, or the reason there is none.
//!
//! The order, each step a lesson of the shell's `mobile-data up`:
//!
//! 1. The modem stack exists (else `NO_MODEM`, no retry) and no radio sequence of the modem service runs.
//! 2. The radio on (the modem service's `radio_on`, under the radio lock), unless the card's radio is switched off
//!    in its settings (FINDINGS 185): then held, no dial.
//! 3. A card answers (`AT+CPIN?`; asked twice, 3 s apart, since a card still starting must not count as none):
//!    an empty slot is held, not redialled every 20 s (FINDINGS 186).
//! 4. Registered within 60 s; then the roaming gate (FINDINGS 120).
//! 5. The IPA delegate (FINDINGS 13f), the context and its credentials (FINDINGS 65), AT+CGACT=1, the address,
//!    AT+CGDATA - under the radio lock, so no lock apply or SIM switch interleaves.
//! 6. The interface identifier of the IPv6 address, if any.

pub mod address;
mod connect;
mod context;

use std::time::Duration;

use f50_core::error::ResultExt;
use f50_core::{Error, Result};
use f50_modem::{RadioOn, SimState};

pub use address::Bearer;

use crate::Data;
use crate::roaming::{self, Gate};
use crate::settings::WanSettings;
use crate::state::Hold;

/// How a dial ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    Up(Bearer),
    Held(Hold),
    /// No modem in this kernel: netifd should not retry.
    NoModem,
    Failed(Error),
}

/// The dial for the card in use with `settings`.
pub(crate) async fn dial(data: &Data, settings: &WanSettings) -> Outcome {
    match run(data, settings).await {
        Ok(o) => o,
        Err(e) => Outcome::Failed(e),
    }
}

async fn run(data: &Data, settings: &WanSettings) -> Result<Outcome> {
    let (at, modem, platform, cfg) = (data.at(), data.modem(), data.platform(), data.config());
    if !crate::kernel::Kernel::new(platform).modem_stack() {
        return Ok(Outcome::NoModem);
    }
    if modem.restarting() {
        return Err(Error::busy("the radio is being reconfigured"));
    }
    let slot = modem.active_slot();
    if !modem.radio_allowed() {
        tracing::info!("the radio of the SIM in use is switched off in its settings: no radio, no dial");
        return Ok(Outcome::Held(Hold::RadioOff));
    }
    {
        let guard = modem.radio_guard("data.dial", Duration::from_secs(120)).await?;
        if modem.radio_on(&guard).await.context("the radio did not come on")? == RadioOn::Disabled {
            return Ok(Outcome::Held(Hold::RadioOff));
        }
    }
    if modem.check_sim().await == Some(SimState::Absent) {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if modem.check_sim().await == Some(SimState::Absent) {
            tracing::info!(slot = slot.number(), "no card in the SIM slot in use: no dial until one answers");
            return Ok(Outcome::Held(Hold::SimMissing));
        }
    }
    if !connect::wait_registered(at, modem, cfg.register_wait).await {
        return Err(Error::failed("not registered on the network"));
    }
    match roaming::gate(at, settings.card.roaming_allowed()).await {
        Gate::Yes => {}
        Gate::Held => return Ok(Outcome::Held(Hold::Roaming)),
        Gate::Unknown => {
            return Err(Error::failed(
                "could not tell whether the network is a roaming one; data roaming is off, so no dial",
            ));
        }
    }
    connect::delegate(at, platform).await?;
    let (ipv4, prefix, dns) = {
        // the radio sequence of the dial: nothing else reconfigures the radio meanwhile
        let _guard = modem.radio_guard("data.dial", Duration::from_secs(120)).await?;
        let ctx = context::Context { at, platform, slot, cid: cfg.cid };
        ctx.define(&settings.card).await?;
        let addr = connect::activate(at, cfg.cid).await?;
        connect::data_mode(at, cfg.cid, slot).await?;
        addr
    };
    let iid = connect::iid(at, cfg.cid).await;
    let dns = if settings.peerdns { dns } else { Vec::new() };
    Ok(Outcome::Up(Bearer { device: cfg.device_for(slot).to_owned(), ipv4, prefix, dns, iid }))
}
