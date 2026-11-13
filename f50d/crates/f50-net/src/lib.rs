//! The F50's networks on OpenWrt: the Wi-Fi hotspot, the Wi-Fi client uplink, the LAN, the USB-C port's role and
//! network function, and adbd in the USB gadget.
//!
//! [`Net::start`] builds the service; [`Net::objects`] publishes `f50.wifi`, `f50.wificlient`, `f50.lan` and
//! `f50.usb`. The binary calls the boot and hotplug entry points below at the moments procd and netifd give it:
//!
//! | entry point | when | replaces |
//! |---|---|---|
//! | [`Net::boot_wifi_config`] | at boot, before the Wi-Fi comes up (`wifi up`) | `openwrt-wifi-wpa2-only migrate`, `94-f50-ap-uapsd`, `openwrt-wifi-config` |
//! | [`Net::wifi_ready`] | after it, to decide the one Wi-Fi restart | `wifi-ready` |
//! | [`Net::boot_wifi_client`] | at boot, once the radio is there | `wifi-client up` |
//! | [`Net::boot_usb`] | at boot | `device-usb boot`, `f50-usb boot` (the one-boot USB mode) |
//! | [`Net::hotplug_iface`] | netifd `iface` hotplug (`ACTION`, `INTERFACE`) | `10-f50-usb`, `90-unisoc-usb-host` (iface) |
//! | [`Net::hotplug_net`] | kernel `net` hotplug (`ACTION`, `INTERFACE`) | `90-unisoc-usb-host` (net) |
//! | [`Net::usb_rebind_if_no_lease`] | at the end of the boot | `f50-usb-reset --if-no-lease` |
//! | [`Net::adbd_start`] / [`Net::adbd_stop`] | the adbd service (procd instance or the daemon's own task) | `f50-adbd run`/`stop` |
//! | [`Net::hotspot_toggle`] | the Wi-Fi key | `hotspot-ap.sh` |
//! | [`Net::hotspot_up`] | the LED inputs, every few seconds | `mu300-led events` (`hostapd.wlan0 get_status`) |
//!
//! Settings that are UCI's (`wireless`, `network`, `dhcp`, `firewall`) are changed only through
//! `f50_platform::uci::Uci::apply`, then netifd or the service reloads. What the shell layer kept in `/etc/f50`
//! (the Wi-Fi client's saved network, adb's state, the USB network mode that boot/init reads) stays there, so an
//! updated device keeps its choices and the initramfs keeps reading them.

mod adbd;
mod client;
mod lan;
mod objects;
mod rules;
mod tools;
mod usb;
mod wifi;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use f50_core::Result;
use f50_core::jobs::Jobs;
use f50_core::rpc::Object;
use f50_platform::Platform;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use adbd::AdbState;
pub use wifi::ready::WifiReady;

/// Where things are on the F50. `Default` is the F50 on OpenWrt; every path is a device path, resolved under the
/// platform's root (`Paths::sys`).
#[derive(Debug, Clone)]
pub struct NetConfig {
    /// The SC2355's interface: the hotspot, or the client of another network.
    pub wlan: String,
    /// The LAN bridge and its netifd interface.
    pub lan_bridge: String,
    pub lan_interface: String,
    /// The F50's LAN address when UCI has none (`f50-lan-ip`).
    pub default_lan_ip: std::net::Ipv4Addr,
    /// The regulatory country when the radio has none.
    pub default_country: String,
    /// The route metric of the Wi-Fi uplink: below mobile data's 100, so Wi-Fi carries the traffic while it is up.
    pub client_metric: u32,
    /// How long a join waits for the supplicant to say COMPLETED.
    pub join_wait: Duration,
    /// The shell layer's settings directory (adb, the saved Wi-Fi network, the USB network mode boot/init reads).
    pub legacy_dir: PathBuf,
    /// What boot/init made of the USB network mode this boot (`ncm`, `ecm`, `rndis`, `multi`).
    pub usb_net_applied: PathBuf,
    /// The DWC3 role switch of the USB-C port.
    pub usb_role: PathBuf,
    /// The configfs gadget boot/init made, and the directory of gadgets.
    pub gadget: PathBuf,
    pub gadgets: PathBuf,
    /// The device controllers (`/sys/class/udc`).
    pub udc_class: PathBuf,
    /// FunctionFS for adbd, and adbd itself.
    pub ffs: PathBuf,
    pub adbd: PathBuf,
    /// wpa_supplicant's control sockets.
    pub wpa_ctrl_dir: PathBuf,
    /// dnsmasq's leases.
    pub leases: PathBuf,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            wlan: "wlan0".into(),
            lan_bridge: "br-lan".into(),
            lan_interface: "lan".into(),
            default_lan_ip: std::net::Ipv4Addr::new(192, 168, 77, 1),
            default_country: "TR".into(),
            client_metric: 50,
            join_wait: Duration::from_secs(25),
            legacy_dir: "/etc/f50".into(),
            usb_net_applied: "/run/f50/usb-net-applied".into(),
            usb_role: "/sys/class/usb_role/25100000.dwc3-role-switch/role".into(),
            gadget: "/sys/kernel/config/usb_gadget/linux".into(),
            gadgets: "/sys/kernel/config/usb_gadget".into(),
            udc_class: "/sys/class/udc".into(),
            ffs: "/dev/usb-ffs/adb".into(),
            adbd: "/opt/f50/bin/adbd".into(),
            wpa_ctrl_dir: "/var/run/wpa_supplicant".into(),
            leases: "/tmp/dhcp.leases".into(),
        }
    }
}

/// The networks service. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Net {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    cfg: NetConfig,
    platform: Platform,
    jobs: Jobs,
    tracker: TaskTracker,
    shutdown: CancellationToken,
    /// One of these at a time: a join, a leave, the handler of the supplicant's events (each would otherwise put up
    /// its own firewall over the others').
    client: tokio::sync::Mutex<client::State>,
    /// The client holds the radio (a join under way or done): the hotspot must not take it. Read without the lock.
    client_active: std::sync::atomic::AtomicBool,
    /// Whoever unbinds and binds the USB gadget holds this until it is bound again (FINDINGS 237).
    rebind: tokio::sync::Mutex<()>,
    /// The LAN hook's last re-enumeration (at most one every 20 s).
    last_rebind: Mutex<Option<Instant>>,
    /// The running adbd supervisor's stop.
    adbd: Mutex<Option<CancellationToken>>,
}

impl Net {
    /// The service. Nothing runs until an entry point or a method asks: the boot stages decide what starts.
    pub fn start(
        cfg: NetConfig,
        platform: Platform,
        jobs: Jobs,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<Net> {
        if !rules::ifname_ok(&cfg.wlan) || !rules::ifname_ok(&cfg.lan_bridge) {
            return Err(f50_core::Error::failed("f50-net: an interface name in the configuration is not one"));
        }
        Ok(Net {
            inner: Arc::new(Inner {
                cfg,
                platform,
                jobs,
                tracker: tracker.clone(),
                shutdown,
                client: tokio::sync::Mutex::new(client::State::default()),
                client_active: std::sync::atomic::AtomicBool::new(false),
                rebind: tokio::sync::Mutex::new(()),
                last_rebind: Mutex::new(None),
                adbd: Mutex::new(None),
            }),
        })
    }

    /// `f50.wifi`, `f50.wificlient`, `f50.lan`, `f50.usb`.
    pub fn objects(&self) -> Vec<Object> {
        vec![objects::wifi(self), objects::wificlient(self), objects::lan(self), objects::usb(self)]
    }

    /// At boot, before the Wi-Fi comes up: an access point on WPA3/SAE becomes WPA2 (FINDINGS 80), U-APSD goes off on
    /// every access point (FINDINGS 203), and the wireless configuration is made once (or a DFS channel set since is
    /// replaced, FINDINGS 42). A saved, enabled Wi-Fi client network keeps the access point off for this boot, so the
    /// radio stays a station (FINDINGS 14b). Returns whether `/etc/config/wireless` changed (the caller's Wi-Fi start
    /// then takes it).
    pub async fn boot_wifi_config(&self) -> Result<bool> {
        wifi::boot::config(self).await
    }

    /// Is the Wi-Fi up as `/etc/config/wireless` has it? Read-only (netifd and hostapd over ubus).
    pub async fn wifi_ready(&self) -> WifiReady {
        wifi::ready::check(self).await
    }

    /// At boot, once wlan0 exists: joins the saved Wi-Fi network if it is enabled (`wifi-client up`); when that
    /// fails, the hotspot comes back. Nothing saved is not an error.
    pub async fn boot_wifi_client(&self) -> Result<()> {
        client::boot(self).await
    }

    /// The hotspot is off in `/etc/config/wireless` (scan mode, a saved network kept the radio, or by hand): netifd
    /// then makes no wlan0, and [`Net::boot_wifi_client`] makes it as a station.
    pub async fn hotspot_disabled(&self) -> bool {
        client::hotspot_disabled(self).await
    }

    /// The hotspot is up (enabled, and hostapd runs it): what the Wi-Fi LED shows. Read-only, over ubus.
    pub async fn hotspot_up(&self) -> bool {
        wifi::ap::is_up(self).await
    }

    /// The hotspot on or off in one press (the Wi-Fi key, FINDINGS 222). Returns the word `hotspot-ap.sh` printed:
    /// `pending`, `unchanged` or `saved`; a refusal is an error (`busy` for staged changes or the client).
    pub async fn hotspot_toggle(&self) -> Result<&'static str> {
        wifi::ap::toggle(self).await
    }

    /// At boot: a one-boot USB network mode is used up once boot/init made it; the host role is applied when it is
    /// the saved policy; USB NICs saved as LAN ports join the bridge.
    pub async fn boot_usb(&self) -> Result<()> {
        usb::boot(self).await
    }

    /// netifd's `iface` hotplug. On `ifup` of the LAN: the gadget's netdevs into the bridge, the early DHCP server
    /// gone, the bridge's address off the ports, one throttled gadget re-enumeration (macOS), the USB NICs synced.
    pub async fn hotplug_iface(&self, action: &str, interface: &str) {
        usb::hotplug::iface(self, action, interface).await;
    }

    /// The kernel's `net` hotplug: a USB NIC saved as a LAN port that appeared joins the bridge.
    pub async fn hotplug_net(&self, action: &str, iface: &str) {
        usb::hotplug::net(self, action, iface).await;
    }

    /// The end of the boot: when the USB host has no lease after `delay`, the gadget is re-enumerated once
    /// (FINDINGS 237). Runs as a tracked task; returns at once.
    pub fn usb_rebind_if_no_lease(&self, delay: Duration) {
        usb::gadget::spawn_lease_check(self, delay);
    }

    /// Starts the adbd supervisor (adbd in the USB gadget) unless adb is off or cannot run here. Returns at once;
    /// the supervisor is a tracked task that stops at shutdown or [`Net::adbd_stop`].
    pub fn adbd_start(&self) {
        adbd::start(self);
    }

    /// Stops the supervisor: adbd goes, the gadget is bound again without it.
    pub fn adbd_stop(&self) {
        adbd::stop(self);
    }

    /// adb's state in one word: `off`, `unavailable`, `open` or `keys`.
    pub fn adb_state(&self) -> AdbState {
        adbd::state(self)
    }

    // ---- shared pieces for the modules

    fn cfg(&self) -> &NetConfig {
        &self.inner.cfg
    }

    fn platform(&self) -> &Platform {
        &self.inner.platform
    }

    /// A device path under the platform's root.
    fn path(&self, absolute: impl AsRef<std::path::Path>) -> PathBuf {
        self.inner.platform.paths().sys(absolute)
    }

    /// A file of the shell layer's settings directory.
    fn legacy(&self, name: &str) -> PathBuf {
        self.path(self.cfg().legacy_dir.join(name))
    }

    /// f50d's runtime directory (private).
    fn run_dir(&self) -> PathBuf {
        self.inner.platform.paths().run()
    }

    /// Runs `fut` as a task the daemon owns, stopped at shutdown.
    fn spawn<F>(&self, name: &'static str, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let stop = self.inner.shutdown.clone();
        self.inner.tracker.spawn(async move {
            tokio::select! {
                () = fut => {}
                () = stop.cancelled() => tracing::debug!(task = name, "stopped at shutdown"),
            }
        });
    }

    /// `f` after `delay`, so the reply of the request that asked can leave first (a Wi-Fi or LAN reload cuts the
    /// caller's link).
    fn later<F, Fut>(&self, name: &'static str, delay: Duration, f: F)
    where
        F: FnOnce(Net) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let net = self.clone();
        self.spawn(name, async move {
            tokio::time::sleep(delay).await;
            if let Err(e) = f(net).await {
                tracing::warn!(task = name, error = %e, "deferred step failed");
            }
        });
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
