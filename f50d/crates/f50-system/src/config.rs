//! What the system service is told at start: the F50's defaults, which background parts run, and their limits.

use std::path::PathBuf;
use std::time::Duration;

/// Linux key codes (`linux/input-event-codes.h`).
pub mod keycode {
    /// `KEY_POWER`: the F50's power key (gpio-keys, docs/research/FINDINGS.md 19).
    pub const POWER: u16 = 116;
    /// `KEY_RESTART`: the usual code of a reset key. The F50's gpio-keys exposes no such code (FINDINGS 19: only
    /// volume up/down and power), so with the default map the reset key does nothing until a device tree names one.
    pub const RESTART: u16 = 0x198;
    /// `KEY_WPS_BUTTON`: the usual code of a WPS key; the same caveat as [`RESTART`].
    pub const WPS: u16 = 0x211;
    /// `KEY_VOLUMEUP` / `KEY_VOLUMEDOWN`: what the F50's other two buttons report (FINDINGS 19), unmapped by default.
    pub const VOLUME_UP: u16 = 115;
    pub const VOLUME_DOWN: u16 = 114;
}

/// The thermal guard's thresholds, in millidegrees Celsius (docs/research/FINDINGS.md 48 and 166).
#[derive(Debug, Clone)]
pub struct ThermalConfig {
    /// Above this, cap cpufreq one step per round (when the kernel does not throttle) and raise the heat alarm.
    pub hot: i64,
    /// At or below this, lift our caps a step per round and end the heat alarm.
    pub cool: i64,
    /// At this, power off, whatever the kernel does.
    pub crit: i64,
    /// The kernel throttles but the SoC still climbs (FINDINGS 166): cap the A76 clusters on top from here...
    pub backstop: i64,
    /// ...and the A55 cluster (policy0) too from here.
    pub backstop_all: i64,
    /// The time between two rounds.
    pub interval: Duration,
}

impl Default for ThermalConfig {
    fn default() -> Self {
        ThermalConfig {
            hot: 85_000,
            cool: 75_000,
            crit: 105_000,
            backstop: 92_000,
            backstop_all: 95_000,
            interval: Duration::from_secs(5),
        }
    }
}

/// What a held power key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerKeyAction {
    /// Shut down cleanly, as mu300-buttons did (the battery-less F50 starts again while it has USB power).
    Poweroff,
    Reboot,
    /// Only wake the LEDs.
    Nothing,
}

/// Which key code is which button, and the press times.
#[derive(Debug, Clone)]
pub struct KeyConfig {
    pub power: u16,
    /// The key that switches the hotspot off and on when held (forwarded as [`crate::ButtonAction::WifiToggle`]).
    pub wifi: Option<u16>,
    /// The key that reboots when held past `long` and released, and asks for a factory reset when held `factory`.
    pub reset: Option<u16>,
    /// A press at least this long is a hold (mu300-keys `-l 3000`).
    pub long: Duration,
    /// A reset key held this long asks for a factory reset; the long hold itself is the confirmation.
    pub factory: Duration,
    pub power_long: PowerKeyAction,
}

impl Default for KeyConfig {
    fn default() -> Self {
        KeyConfig {
            power: keycode::POWER,
            wifi: Some(keycode::WPS),
            reset: Some(keycode::RESTART),
            long: Duration::from_secs(3),
            factory: Duration::from_secs(10),
            power_long: PowerKeyAction::Poweroff,
        }
    }
}

/// The system service's configuration. `Default` is the F50 on OpenWrt with every part running.
#[derive(Debug, Clone)]
pub struct SystemConfig {
    /// The hotspot's interface (the station table for the Wi-Fi clients summary).
    pub wifi_device: String,
    /// dnsmasq's leases.
    pub leases_file: PathBuf,
    /// The note of a newer release (a line holding its tag, `vYYYY.MM.DD[.N]`), as the update service leaves it.
    pub update_notice: PathBuf,
    /// Where the key reader looks for input devices.
    pub input_dir: PathBuf,
    pub thermal: ThermalConfig,
    pub keys: KeyConfig,
    /// Apply the saved CPU profile at start (init.d/mu300-cpu).
    pub cpu_profile_at_start: bool,
    /// Set up zram swap at start (zram-start).
    pub zram: bool,
    pub thermal_guard: bool,
    pub leds: bool,
    pub buttons: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        SystemConfig {
            wifi_device: "wlan0".into(),
            leases_file: "/tmp/dhcp.leases".into(),
            update_notice: "/var/run/f50d/update-notice.json".into(),
            input_dir: "/dev/input".into(),
            thermal: ThermalConfig::default(),
            keys: KeyConfig::default(),
            cpu_profile_at_start: true,
            zram: true,
            thermal_guard: true,
            leds: true,
            buttons: true,
        }
    }
}
