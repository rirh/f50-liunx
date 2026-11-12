//! Mobile data on the F50: the cellular connection netifd runs through its `f50cell` protocol, the data roaming
//! policy, the dead-downlink watchdog, the data usage count and the IPv6 relay.
//!
//! The pieces, and who calls them:
//!
//! - [`proto`]: netifd's `f50cell` protocol. `/lib/netifd/proto/f50cell.sh` runs `f50d proto setup|teardown|renew`,
//!   and the binary hands those to [`Data::proto_setup`], [`Data::proto_teardown`] and [`Data::proto_renew`]. The
//!   daemon dials ([`dial`]) and reports the result to netifd itself (`network.interface notify_proto`, [`netifd`]).
//! - [`watchdog`]: every 30 s, while the bearer is up: the context, its address, data roaming and the downlink.
//! - [`traffic`]: the bytes of the bearer, per local day and billing cycle, sampled once a minute into RAM and
//!   rarely written to flash, with a monthly cap that can turn data off.
//! - [`relay`]: in `option ipv6 'relay'`, the carrier's RA addresses reported to netifd and its /64 routed to the LAN.
//! - [`objects`]: `f50.data` and `f50.traffic`.
//!
//! Settings live in UCI (`network.wan`), see [`settings`].

pub mod dial;
mod kernel;
pub mod netifd;
pub mod objects;
pub mod proto;
mod radio;
pub mod relay;
pub mod roaming;
pub mod settings;
mod state;
pub mod traffic;
pub mod watchdog;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use f50_at::At;
use f50_core::Result;
use f50_core::jobs::Jobs;
use f50_core::rpc::Object;
use f50_modem::{Modem, Slot};
use f50_platform::Platform;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use state::{Hold, Phase};

/// Settings of the data service; the defaults are the F50's.
#[derive(Debug, Clone)]
pub struct DataConfig {
    /// netifd's interface of the bearer (`network.wan`, proto `f50cell`).
    pub interface: String,
    /// The DHCPv6 alias added for the bearer's IPv6 when the network assigned it (not in relay mode).
    pub interface6: String,
    /// The bearer of SIM1, the tray card.
    pub device: String,
    /// The bearer of SIM2, the internal card: the second SIPA group (`sipa_eth8`).
    pub device_internal: String,
    /// The LAN bridge that IPv6 is shared with in relay mode.
    pub lan: String,
    /// The PDP context of the internet bearer.
    pub cid: u8,
    /// The watchdog's wait between two looks.
    pub watch_interval: Duration,
    /// How long a dial waits for the registration.
    pub register_wait: Duration,
    /// How often the traffic counters are read.
    pub traffic_interval: Duration,
}

impl Default for DataConfig {
    fn default() -> Self {
        DataConfig {
            interface: "wan".into(),
            interface6: "wan6".into(),
            device: "sipa_eth0".into(),
            device_internal: "sipa_eth8".into(),
            lan: "br-lan".into(),
            cid: 1,
            watch_interval: Duration::from_secs(30),
            register_wait: Duration::from_secs(60),
            traffic_interval: Duration::from_secs(60),
        }
    }
}

impl DataConfig {
    /// The bearer of a SIM slot: each card has its own SIPA group.
    pub fn device_for(&self, slot: Slot) -> &str {
        match slot {
            Slot::One => &self.device,
            Slot::Two => &self.device_internal,
        }
    }
}

/// The data service. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Data {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    cfg: DataConfig,
    at: At,
    modem: Modem,
    platform: Platform,
    tracker: TaskTracker,
    shutdown: CancellationToken,
    state: Mutex<state::State>,
    session: proto::Sessions,
    traffic: traffic::Traffic,
}

impl Data {
    /// Starts the watchdog, the traffic sampler and (in relay mode) the IPv6 relay on `tracker`; they stop at
    /// `shutdown`. `jobs` is not used: no method of data runs longer than a request (a dial is netifd's).
    pub fn start(
        cfg: DataConfig,
        at: At,
        modem: Modem,
        platform: Platform,
        jobs: Jobs,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<Data> {
        let _ = jobs;
        let traffic = traffic::Traffic::new(&platform, &cfg)?;
        let data = Data {
            inner: Arc::new(Inner {
                cfg,
                at,
                modem,
                platform,
                tracker: tracker.clone(),
                shutdown: shutdown.clone(),
                state: Mutex::new(state::State::default()),
                session: proto::Sessions::default(),
                traffic,
            }),
        };
        tracker.spawn(watchdog::run(data.clone(), shutdown.clone()));
        tracker.spawn(traffic::sampler::run(data.clone(), shutdown.clone()));
        tracker.spawn(relay::ndp::run(data.clone(), shutdown));
        Ok(data)
    }

    /// `f50.data` and `f50.traffic`. netifd's protocol is [`Data::proto_object`], which is not among them.
    pub fn objects(&self) -> Vec<Object> {
        vec![objects::data_object(self), objects::traffic_object(self)]
    }

    /// The clock is right (NTP has set it): the data count dates the bytes it held back. The binary calls this from
    /// its NTP hook (`f50d clock-synced`, run by `/etc/hotplug.d/ntp` on a stratum change).
    pub fn clock_synced(&self) -> Result<()> {
        let run = self.inner.platform.paths().run();
        f50_core::fsx::private_dir(&run)?;
        f50_core::fsx::write_atomic(&run.join("clock-synced"), b"", 0o600)?;
        self.inner.traffic.wake.notify_one();
        Ok(())
    }

    pub fn config(&self) -> &DataConfig {
        &self.inner.cfg
    }

    pub(crate) fn at(&self) -> &At {
        &self.inner.at
    }

    pub(crate) fn modem(&self) -> &Modem {
        &self.inner.modem
    }

    pub(crate) fn platform(&self) -> &Platform {
        &self.inner.platform
    }

    pub(crate) fn traffic(&self) -> &traffic::Traffic {
        &self.inner.traffic
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, state::State> {
        self.inner.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The bearer of the card in use.
    pub(crate) fn device(&self) -> String {
        self.inner.cfg.device_for(self.inner.modem.active_slot()).to_owned()
    }

    pub(crate) fn notifier(&self) -> netifd::Notifier<'_> {
        netifd::Notifier::new(&self.inner.platform)
    }
}
