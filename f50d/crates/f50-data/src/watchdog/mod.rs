//! The data watchdog: every 30 s, while the bearer is up, is it still the bearer the network gave us?
//!
//! The modem firmware resets itself now and then (modem_control reloads it); it comes back with no PDP context,
//! which Android's RIL repairs silently. Per round, with the bearer connected:
//!
//! - `AT+CGACT?` says *active*, *inactive* (a final OK and the context not up, or no line for it: what a firmware
//!   reset leaves) or *unknown* (no final result in time, ERROR). Unknown is not evidence of a lost bearer: a channel
//!   can be slow while the modem moves between cells (FINDINGS 90). Four unknown rounds in a row count as lost.
//! - Active, and the device on the context's address (`AT+CGPADDR`): fine; then data roaming (the card moved onto a
//!   roaming network with data roaming off: data goes down, FINDINGS 120) and the downlink ([`downlink`]).
//!   Active with another address than the device's (the network re-attached by itself and gave a new one; measured:
//!   rx 0 on the old address): reconnect at once.
//! - Two failed rounds: the interface is restarted (netifd runs the protocol again).
//!
//! The holds of the dial are watched too: an empty slot is asked again every tenth round and the interface made
//! available once a card answers (FINDINGS 186); a roaming hold ends when somebody takes the interface down
//! (netifd's own retry brings data back at home). A radio found off is switched on again through the modem service
//! when its card's setting allows it.

mod downlink;

use std::time::Duration;

use f50_at::SendOpts;
use tokio_util::sync::CancellationToken;

use crate::Data;
use crate::dial::address::v4_of_cgpaddr;
use crate::kernel::Kernel;
use crate::netifd::IfStatus;
use crate::radio;
use crate::roaming::{self, Network};
use crate::settings::WanSettings;
use crate::state::{Hold, Phase};
use f50_modem::SimState;

/// Rounds without an answer about the context before it counts as lost.
const UNKNOWN_ROUNDS: u32 = 4;

/// What AT+CGACT? says about our context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
    Active,
    Inactive,
    Unknown,
}

/// `+CGACT: <cid>,<state>` lines of an answer that ended OK; `+CGACT: 1,1` with or without a space, never
/// `+CGACT:11,1` or `1,10` for context 1.
pub(crate) fn context_state(ok: bool, values: &[&str], cid: u8) -> Context {
    if !ok {
        return Context::Unknown;
    }
    let active = values.iter().any(|v| {
        let mut f = v.split(',').map(str::trim);
        f.next().and_then(|c| c.parse::<u8>().ok()) == Some(cid) && f.next() == Some("1")
    });
    if active { Context::Active } else { Context::Inactive }
}

async fn context(data: &Data) -> Context {
    match data.at().send("AT+CGACT?", SendOpts::control(Duration::from_secs(4))).await {
        Ok(r) => {
            let v: Vec<&str> = r.values("+CGACT").collect();
            context_state(r.is_ok(), &v, data.config().cid)
        }
        Err(_) => Context::Unknown,
    }
}

/// netifd runs the protocol again (`ifup`: down, then up).
pub(crate) async fn restart(data: &Data, interface: &str) {
    let n = data.platform().netifd();
    if let Err(e) = n.down(interface).await {
        tracing::warn!(error = %e, "ifdown {interface}");
    }
    if let Err(e) = n.up(interface).await {
        tracing::warn!(error = %e, "ifup {interface}");
    }
}

#[derive(Debug, Default)]
struct Watch {
    fails: u32,
    unknown: u32,
    sim_rounds: u32,
    downlink: downlink::Downlink,
}

pub(crate) async fn run(data: Data, shutdown: CancellationToken) {
    let mut w = Watch::default();
    let every = data.config().watch_interval;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(every) => {}
        }
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = w.round(&data) => {}
        }
    }
}

impl Watch {
    async fn round(&mut self, data: &Data) {
        if data.modem().restarting() {
            return;
        }
        let (phase, hold, config, bearer) = {
            let st = data.state();
            (st.phase, st.hold, st.config.clone(), st.bearer.clone())
        };
        let interface = config.unwrap_or_else(|| data.config().interface.clone());
        match hold {
            Some(Hold::SimMissing) => return self.sim_missing(data, &interface).await,
            Some(Hold::Roaming) => {
                let st = IfStatus::read(data.platform(), &interface).await.ok().flatten();
                if st.is_some_and(|s| !s.autostart) {
                    data.state().hold = None;
                }
                return;
            }
            _ => {}
        }
        let (Phase::Connected, Some(bearer)) = (phase, bearer) else {
            self.fails = 0;
            self.unknown = 0;
            self.downlink.reset();
            return;
        };
        let dev = bearer.device.as_str();
        let state = context(data).await;
        if state == Context::Unknown {
            self.unknown += 1;
            if self.unknown < UNKNOWN_ROUNDS {
                return;
            }
            tracing::warn!("no answer about the data context for {} rounds", self.unknown);
        } else {
            self.unknown = 0;
        }
        if state == Context::Active {
            let have = Kernel::new(data.platform()).v4_addresses(dev).await;
            if let Some(have) = have.first().and_then(|c| c.split('/').next()).map(str::to_owned) {
                let cmd = format!("AT+CGPADDR={}", data.config().cid);
                let want = match data.at().send(&cmd, SendOpts::control(Duration::from_secs(4))).await {
                    Ok(r) => r.values("+CGPADDR").find_map(v4_of_cgpaddr),
                    Err(_) => None,
                };
                if want.as_ref().is_none_or(|w| *w == have) {
                    self.fails = 0;
                    return self.connected(data, &interface, dev).await;
                }
                tracing::warn!("the network gave the context a new address; {dev} still has the old one: reconnecting");
                self.fails = 1;
            }
        }
        if radio::cfun(data.at()).await != Some(1) {
            radio::ensure_on(data.modem()).await;
        }
        self.fails += 1;
        if self.fails < 2 {
            return;
        }
        tracing::warn!("mobile data lost (modem reset or detached), reconnecting");
        restart(data, &interface).await;
        self.fails = 0;
        self.downlink.reset();
    }

    /// A healthy context: roaming, then the downlink.
    async fn connected(&mut self, data: &Data, interface: &str, dev: &str) {
        let settings = match WanSettings::load(data.platform(), interface, data.modem().active_slot()).await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "network settings");
                WanSettings { dl_reboot: true, ..WanSettings::default() }
            }
        };
        if !settings.card.roaming_allowed() && roaming::network(data.at()).await == Network::Roaming {
            tracing::warn!("the card is on a roaming network and data roaming is off: mobile data goes down");
            restart(data, interface).await;
            return;
        }
        self.downlink.check(data, interface, dev, settings.dl_reboot).await;
    }

    async fn sim_missing(&mut self, data: &Data, interface: &str) {
        self.sim_rounds += 1;
        if !self.sim_rounds.is_multiple_of(10) || data.modem().check_sim().await != Some(SimState::Ready) {
            return;
        }
        tracing::info!("a SIM card answers again: mobile data comes back");
        data.state().hold = None;
        if let Err(e) = data.notifier().set_available(interface, true).await {
            tracing::warn!(error = %e, "could not make {interface} available");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_states() {
        assert_eq!(context_state(true, &["1,1", "11,1"], 1), Context::Active);
        assert_eq!(context_state(true, &["1, 1"], 1), Context::Active);
        assert_eq!(context_state(true, &["1,0", "11,1"], 1), Context::Inactive);
        assert_eq!(context_state(true, &["11,1"], 1), Context::Inactive);
        assert_eq!(context_state(true, &["1,10"], 1), Context::Inactive);
        assert_eq!(context_state(true, &[], 1), Context::Inactive);
        assert_eq!(context_state(false, &["1,1"], 1), Context::Unknown);
    }
}
