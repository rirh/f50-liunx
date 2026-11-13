//! The F50's modem: the SIM cards, the radio, the network/band/cell/EN-DC locks, and the readings.
//!
//! [`Modem`] owns the background collector (signal, registration, serving and neighbour cells: read on the poll
//! lane, published as a [`RadioSnapshot`]; readers never wait for the modem) and the **radio lock**: every
//! sequence that restarts or reconfigures the radio (a dial, the data watchdog, a lock apply, a SIM switch, a modem
//! restart) holds a [`RadioGuard`], so two of them never interleave.
//!
//! Modules: [`collector`] (the AT rounds and their parsers), [`lock`] (the locks, their codecs, apply, reset and the
//! boot replay), [`sim`] (the slot chosen per boot, each card's radio setting, the card's own state), [`radio`]
//! (the radio lock and the SFUN sequences), `objects` (`f50.modem`, `f50.sim`, `f50.lock`).
//!
//! CONTRACT: the items below are used by other crates. Extend them freely; do not change a signature.

pub mod collector;
pub mod lock;
mod objects;
pub mod radio;
mod shared;
pub mod sim;

use std::sync::Arc;
use std::sync::atomic::Ordering;
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

pub use radio::RadioOn;
pub use sim::{SimState, boot_slot};

use shared::Inner;

/// A SIM slot, numbered as the panel and `f50-sim default` number them: 1 is SIM1, the external card (the
/// nano-SIM tray, the shell's slot 0), 2 is SIM2, the internal card (the shell's slot 1; FINDINGS 154, 204).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Slot {
    One = 1,
    Two = 2,
}

impl Slot {
    /// 1 or 2.
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// The shell's slot index (0 external, 1 internal): the SIPA data devices and the channel sets count from it.
    pub const fn index(self) -> u8 {
        self as u8 - 1
    }

    pub const fn from_number(n: i64) -> Option<Slot> {
        match n {
            1 => Some(Slot::One),
            2 => Some(Slot::Two),
            _ => None,
        }
    }
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
    /// The whole of a fast round at most (the shell's 12 s): past it every further command is skipped.
    pub fast_deadline: Duration,
    /// The whole of a full round at most (the shell's 25 s).
    pub full_deadline: Duration,
    /// The netifd interface of the cellular data (`wan`): restarted after a stack restart, its bearer's QoS read.
    pub data_interface: String,
    /// How long the boot replay waits for the AT channel and the radio (the shell's `replay_timeout`, 90 s).
    pub replay_timeout: Duration,
    /// How long a lock apply waits for the radio lock held by a dial or the watchdog (the shell's 120 s).
    pub radio_lock_wait: Duration,
    /// A complete SIM/module identity is read again after this long (6 h: a SIM change goes through a reboot).
    pub ident_max_age: Duration,
    /// A partial identity (modem or SIM not ready) is asked again after this long.
    pub ident_retry: Duration,
    /// The lock reading is refreshed in the background when a `get` finds it older than this.
    pub lock_cache_max_age: Duration,
}

impl Default for ModemConfig {
    fn default() -> Self {
        ModemConfig {
            fast_interval: Duration::from_secs(2),
            full_interval: Duration::from_secs(10),
            fast_deadline: Duration::from_secs(12),
            full_deadline: Duration::from_secs(25),
            data_interface: "wan".into(),
            replay_timeout: Duration::from_secs(90),
            radio_lock_wait: Duration::from_secs(120),
            ident_max_age: Duration::from_secs(6 * 3600),
            ident_retry: Duration::from_secs(60),
            lock_cache_max_age: Duration::from_secs(600),
        }
    }
}

/// The modem service. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Modem {
    inner: Arc<Inner>,
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
        let slot = sim::boot_slot(platform.paths());
        let inner = Arc::new(Inner::new(cfg, at, platform, jobs, tracker.clone(), shutdown, slot));
        collector::spawn(&inner);
        lock::replay::spawn_boot_worker(&inner);
        sim::state::spawn_event_listener(&inner);
        Ok(Modem { inner })
    }

    /// The modem's ubus objects (`f50.modem`, `f50.sim`, `f50.lock`).
    pub fn objects(&self) -> Vec<Object> {
        objects::all(&self.inner)
    }

    /// The latest reading; `changed()` wakes on every new one.
    pub fn snapshot(&self) -> watch::Receiver<Arc<RadioSnapshot>> {
        self.inner.snap_tx.subscribe()
    }

    /// The SIM slot the modem uses now.
    pub fn active_slot(&self) -> Slot {
        self.inner.slot
    }

    /// The radio is being restarted or reconfigured (a lock apply, a SIM switch): no new radio work should start.
    pub fn restarting(&self) -> bool {
        self.inner.restarting.load(Ordering::SeqCst) > 0
    }

    /// The radio lock, held for a whole radio sequence. `busy` after `wait`.
    pub async fn radio_guard(&self, owner: &'static str, wait: Duration) -> Result<RadioGuard> {
        radio::acquire(&self.inner, owner, wait).await
    }

    /// May the radio of the card in use be on (its setting in `f50.sim card`; FINDINGS 185)? The dial and the
    /// watchdog ask this before they bring the radio up.
    pub fn radio_allowed(&self) -> bool {
        sim::radio_allowed(&self.inner, self.inner.slot)
    }

    /// What the card in use said of itself at its last check (`AT+CPIN?`; FINDINGS 186), `None` before the first.
    pub fn sim_state(&self) -> Option<SimState> {
        sim::state::current(&self.inner)
    }

    /// Asks the card in use now (`AT+CPIN?`, one command on the control lane) and keeps the answer.
    pub async fn check_sim(&self) -> Option<SimState> {
        sim::state::check(&self.inner).await
    }

    /// The radio on, the way the vendor RIL starts it (the RIL handshake once per boot, the early lock replay, then
    /// `AT+SFUN=4` and a wait for `+CFUN: 1`). The caller holds the radio lock. [`RadioOn::Disabled`] when the card's
    /// radio is switched off in its settings (FINDINGS 185, 202).
    pub async fn radio_on(&self, guard: &RadioGuard) -> Result<RadioOn> {
        radio::radio_on(&self.inner, guard).await
    }
}

/// Held while a radio sequence runs; released when dropped.
pub struct RadioGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    owner: Arc<std::sync::Mutex<Option<&'static str>>>,
}

impl std::fmt::Debug for RadioGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioGuard").finish_non_exhaustive()
    }
}

impl Drop for RadioGuard {
    fn drop(&mut self) {
        *self.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}
