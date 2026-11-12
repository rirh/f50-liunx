//! The F50's modem: the SIM cards, the radio, the network/band/cell/EN-DC locks, and the readings.
//!
//! [`Modem`] owns the background collector (signal, registration, serving and neighbour cells: read on the poll
//! lane, published as a [`RadioSnapshot`]; readers never wait for the modem) and the **radio lock**: every
//! sequence that restarts or reconfigures the radio (a dial, the data watchdog, a lock apply, a SIM switch, a modem
//! restart) holds a [`RadioGuard`], so two of them never interleave.
//!
//! CONTRACT: the items below are used by other crates. Extend them freely; do not change a signature.

use std::sync::Arc;
use std::time::{Duration, Instant};

use f50_at::At;
use f50_core::Result;
use f50_core::jobs::Jobs;
use f50_core::rpc::Object;
use f50_platform::Platform;
use serde::Serialize;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// A SIM slot: 1 is the internal (eSIM/soldered) card, 2 the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Slot {
    One = 1,
    Two = 2,
}

/// The radio access technology in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rat {
    Gsm,
    Umts,
    Lte,
    /// 5G non-standalone (LTE anchor with an NR leg).
    Nsa,
    /// 5G standalone.
    Sa,
}

/// The serving cell's signal.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Signal {
    pub rsrp: Option<f64>,
    pub rsrq: Option<f64>,
    pub sinr: Option<f64>,
    pub rssi: Option<f64>,
    /// 0-5 bars, as the LEDs and the dashboard show them.
    pub bars: u8,
}

/// The latest reading. `detail` carries everything else the collector read (cells, neighbours, carrier
/// aggregation, QoS) as the API reports it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RadioSnapshot {
    #[serde(skip)]
    pub taken: Option<Instant>,
    pub registered: bool,
    pub roaming: bool,
    pub rat: Option<Rat>,
    pub operator: Option<String>,
    pub signal: Option<Signal>,
    pub detail: serde_json::Value,
}

/// Settings of the modem service; defaults are the F50's.
#[derive(Debug, Clone)]
pub struct ModemConfig {
    /// How often the poll lane reads the serving cell.
    pub fast_interval: Duration,
    /// How often it reads everything else (neighbours, operator, QoS).
    pub full_interval: Duration,
}

impl Default for ModemConfig {
    fn default() -> Self {
        ModemConfig { fast_interval: Duration::from_secs(2), full_interval: Duration::from_secs(10) }
    }
}

/// The modem service. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Modem {
    inner: Arc<()>,
}

impl Modem {
    /// Starts the collector and the boot-time replay of saved locks on `tracker`.
    pub fn start(
        cfg: ModemConfig,
        at: At,
        platform: Platform,
        jobs: Jobs,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<Modem> {
        let _ = (cfg, at, platform, jobs, tracker, shutdown);
        todo!("f50-modem: Modem::start")
    }

    /// The modem's ubus objects (`f50.modem`, `f50.sim`, `f50.lock`).
    pub fn objects(&self) -> Vec<Object> {
        todo!("f50-modem: Modem::objects")
    }

    /// The latest reading; `changed()` wakes on every new one.
    pub fn snapshot(&self) -> watch::Receiver<Arc<RadioSnapshot>> {
        let _ = &self.inner;
        todo!("f50-modem: Modem::snapshot")
    }

    /// The SIM slot the modem uses now.
    pub fn active_slot(&self) -> Slot {
        todo!("f50-modem: Modem::active_slot")
    }

    /// The radio is being restarted or reconfigured (a lock apply, a SIM switch): no new radio work should start.
    pub fn restarting(&self) -> bool {
        todo!("f50-modem: Modem::restarting")
    }

    /// The radio lock, held for a whole radio sequence. `busy` after `wait`.
    pub async fn radio_guard(&self, owner: &'static str, wait: Duration) -> Result<RadioGuard> {
        let _ = (owner, wait);
        todo!("f50-modem: Modem::radio_guard")
    }
}

/// Held while a radio sequence runs; released when dropped.
#[derive(Debug)]
pub struct RadioGuard {
    _priv: (),
}
