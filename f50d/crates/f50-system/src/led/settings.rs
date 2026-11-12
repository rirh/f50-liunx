//! The LED settings (`/etc/f50d/led.conf`, the keys of the shell's led.conf) and their validation.
//!
//! - `LED_TIMEOUT=SECONDS`: dark after that long without a key (0: always on; the F50's default is 0);
//! - `LED_NETWORK`, `LED_WIFI=0|1`: that lamp as usual (1) or always dark (the heat alarm still shows);
//! - `LED_NETWORK_BRIGHTNESS`, `LED_WIFI_BRIGHTNESS=1..100`: % of the lamp's usual level; both F50 lamps dim
//!   (docs/research/FINDINGS.md 227);
//! - `LED_BOOT=0|1`: the boot indicator (off by default);
//! - `LED_THERMAL=siren|blink|off`: how the heat alarm shows. Only the light: the guard decides when it is hot.
//!
//! A bad value in the file is ignored (the default holds); a bad value from the API is refused.

use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::settings;

/// The F50's own timeout: it has no key that is pressed to see the LEDs, so they stay on.
pub(crate) const DEFAULT_TIMEOUT: u32 = 0;
const TIMEOUT_MAX: u32 = 86_400;

/// How the heat alarm shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThermalLight {
    Siren,
    Blink,
    Off,
}

impl ThermalLight {
    fn as_str(self) -> &'static str {
        match self {
            ThermalLight::Siren => "siren",
            ThermalLight::Blink => "blink",
            ThermalLight::Off => "off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LedSettings {
    /// `None`: the device's default.
    pub timeout: Option<u32>,
    pub network: bool,
    pub wifi: bool,
    pub network_brightness: u8,
    pub wifi_brightness: u8,
    pub boot: bool,
    pub thermal: ThermalLight,
}

impl Default for LedSettings {
    fn default() -> Self {
        LedSettings {
            timeout: None,
            network: true,
            wifi: true,
            network_brightness: 100,
            wifi_brightness: 100,
            boot: false,
            thermal: ThermalLight::Siren,
        }
    }
}

fn pct(v: &str) -> Option<u8> {
    let n: u8 = v.parse().ok()?;
    (v == n.to_string() && (1..=100).contains(&n)).then_some(n)
}

fn switch(v: &str) -> Option<bool> {
    match v {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

/// A timeout as the shell took it from the API: digits, no leading zero (0 itself is fine), at most 86400.
fn timeout(v: &str) -> Option<u32> {
    if v.is_empty() || v.len() > 5 || (v.len() > 1 && v.starts_with('0')) || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse().ok().filter(|n| *n <= TIMEOUT_MAX)
}

/// A timeout in the file: digits ("060" is 60, as the shell read it).
fn file_timeout(v: &str) -> Option<u32> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse().ok().filter(|n| *n <= TIMEOUT_MAX)
}

fn thermal(v: &str) -> Option<ThermalLight> {
    match v {
        "siren" => Some(ThermalLight::Siren),
        "blink" => Some(ThermalLight::Blink),
        "off" => Some(ThermalLight::Off),
        _ => None,
    }
}

impl LedSettings {
    pub(crate) fn parse(text: &str) -> Self {
        let d = LedSettings::default();
        let g = |k: &str| settings::get(text, k);
        LedSettings {
            // the file may hold "060" (the shell read it as 60)
            timeout: g("LED_TIMEOUT").and_then(file_timeout),
            network: g("LED_NETWORK").and_then(switch).unwrap_or(d.network),
            wifi: g("LED_WIFI").and_then(switch).unwrap_or(d.wifi),
            network_brightness: g("LED_NETWORK_BRIGHTNESS").and_then(pct).unwrap_or(d.network_brightness),
            wifi_brightness: g("LED_WIFI_BRIGHTNESS").and_then(pct).unwrap_or(d.wifi_brightness),
            boot: g("LED_BOOT").and_then(switch).unwrap_or(d.boot),
            thermal: g("LED_THERMAL").and_then(thermal).unwrap_or(d.thermal),
        }
    }

    pub(crate) fn timeout_secs(&self) -> u32 {
        self.timeout.unwrap_or(DEFAULT_TIMEOUT)
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "device": crate::device::DEVICE,
            "lamps": ["network", "wifi"],
            "dimmable": ["network", "wifi"],
            "timeout": self.timeout_secs(),
            "timeout_default": self.timeout.is_none(),
            "default_timeout": DEFAULT_TIMEOUT,
            "network": self.network,
            "network_brightness": self.network_brightness,
            "wifi": self.wifi,
            "wifi_brightness": self.wifi_brightness,
            "boot": self.boot,
            "thermal": self.thermal.as_str(),
        })
    }
}

/// The settings `f50.led set` takes.
pub(crate) const KEYS: &[&str] =
    &["timeout", "network", "wifi", "network_brightness", "wifi_brightness", "boot", "thermal"];

/// `key` and `value` from the API as the led.conf line to write (`None`: the key taken out, the default again).
pub(crate) fn validate(key: &str, value: &str) -> Result<(&'static str, Option<String>)> {
    let bad = || Error::bad_value("value");
    let ok = |k: &'static str, good: bool| if good { Ok((k, Some(value.to_owned()))) } else { Err(bad()) };
    match key {
        "timeout" if value == "default" => Ok(("LED_TIMEOUT", None)),
        "timeout" => ok("LED_TIMEOUT", timeout(value).is_some()),
        "network" => ok("LED_NETWORK", switch(value).is_some()),
        "wifi" => ok("LED_WIFI", switch(value).is_some()),
        "network_brightness" => ok("LED_NETWORK_BRIGHTNESS", pct(value).is_some()),
        "wifi_brightness" => ok("LED_WIFI_BRIGHTNESS", pct(value).is_some()),
        "boot" => ok("LED_BOOT", switch(value).is_some()),
        "thermal" => ok("LED_THERMAL", thermal(value).is_some()),
        _ => Err(Error::bad_value("key")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ignores_bad_lines() {
        let s =
            LedSettings::parse("# mine\nLED_TIMEOUT=060\nLED_NETWORK=2\nLED_WIFI_BRIGHTNESS=40\nLED_THERMAL=loud\n");
        assert_eq!(s.timeout, Some(60));
        assert!(s.network);
        assert_eq!(s.wifi_brightness, 40);
        assert_eq!(s.thermal, ThermalLight::Siren);
        assert_eq!(LedSettings::parse("LED_TIMEOUT=0\n").timeout, Some(0));
        assert_eq!(LedSettings::parse("").timeout_secs(), 0);
    }

    #[test]
    fn validate_as_the_shell_did() {
        for (k, v) in
            [("network", "0"), ("wifi_brightness", "40"), ("boot", "1"), ("thermal", "blink"), ("timeout", "120")]
        {
            assert!(validate(k, v).is_ok(), "{k} {v}");
        }
        assert_eq!(validate("timeout", "default").expect("default"), ("LED_TIMEOUT", None));
        for (k, v) in [
            ("network", "2"),
            ("network_brightness", "0"),
            ("wifi_brightness", "101"),
            ("timeout", "86401"),
            ("timeout", "060"),
            ("thermal", "loud"),
            ("boot", "yes"),
            ("battery", "1"),
        ] {
            assert!(validate(k, v).is_err(), "{k} {v}");
        }
    }
}
