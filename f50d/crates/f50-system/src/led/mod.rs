//! The F50's LEDs by what they mean (`f50.led`), so the rest of f50d never names a sysfs LED.
//!
//! The F50 has two lamps, as ZTE's Android lights them (docs/research/FINDINGS.md 34):
//!
//! - **network**: the PMIC's RGB LED, one colour at a time: blue on 2G/3G/4G with data, white (its "green" channel)
//!   on 5G with data, red without service; dark while registered without data. Searching blinks the red.
//! - **Wi-Fi**: the PMIC's keypad backlight sink (`keyboard-backlight`, 0-127) at ZTE's 48 while the hotspot is up.
//!
//! It has no power and no battery LED. Over the lamps: the heat alarm (the thermal guard's, shown as a siren of the
//! RGB LED's colours, or its red blinking, or not at all) and, with `boot` on, the boot indicator (the colours in
//! turn until the binary says the boot is done, three minutes at most). The binary feeds the lamps' inputs through
//! [`crate::System::set_led_inputs`]; a key press wakes them; with a timeout they go dark after it.

pub(crate) mod driver;
pub(crate) mod render;
pub(crate) mod settings;

use std::sync::{Arc, Mutex, PoisonError};

use f50_core::{Error, Result};
use f50_platform::Platform;
use serde_json::Value;
use tokio::sync::{Notify, watch};

use crate::settings::ConfFile;
pub(crate) use settings::LedSettings;

/// What the network lamp has to show: the radio's state as the modem has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkLed {
    /// No SIM, no service, or the radio off.
    #[default]
    None,
    /// Not registered yet, looking for a network.
    Searching,
    G2,
    G3,
    G4,
    G5,
}

/// Everything the LEDs depend on that other parts of f50d know. Fed whole by the binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LedInputs {
    pub network: NetworkLed,
    /// Signal bars 0-5, when known. The F50 has no bar LEDs: kept for the aggregate, unused by the lamps.
    pub bars: Option<u8>,
    /// Mobile data is connected.
    pub data_up: bool,
    /// The hotspot's access point is up (hostapd ENABLED).
    pub wifi_on: bool,
    /// Associated clients. The F50's Wi-Fi lamp does not show them (ZTE's does not either).
    pub wifi_clients: u32,
    /// The power service's idle: everything dark, except for a minute (or the timeout) after a key.
    pub idle: bool,
    /// The boot is done: ends the boot indicator.
    pub booted: bool,
}

/// The LEDs' channels, shared by the driver and the methods.
#[derive(Debug)]
pub(crate) struct LedHandle {
    pub inputs: watch::Sender<LedInputs>,
    pub settings: watch::Sender<LedSettings>,
    pub wake: Arc<Notify>,
    conf: ConfFile,
    write: Mutex<()>,
}

impl LedHandle {
    pub(crate) fn load(platform: &Platform) -> Self {
        let conf = ConfFile::new(platform, "led.conf");
        let s = LedSettings::parse(&conf.read());
        LedHandle {
            inputs: watch::Sender::new(LedInputs::default()),
            settings: watch::Sender::new(s),
            wake: Arc::new(Notify::new()),
            conf,
            write: Mutex::new(()),
        }
    }

    pub(crate) fn get(&self) -> Value {
        self.settings.borrow().to_json()
    }

    /// `f50.led set`: one setting, written to led.conf and shown at once (a new timeout counts from now).
    pub(crate) fn set(&self, key: &str, value: &str) -> Result<Value> {
        let (conf_key, conf_value) = settings::validate(key, value)?;
        {
            let _one = self.write.lock().unwrap_or_else(PoisonError::into_inner);
            self.conf.put(conf_key, conf_value.as_deref()).map_err(|e| {
                Error::failed(format!("could not write {}: {}", self.conf.path().display(), e.message()))
            })?;
            let s = LedSettings::parse(&self.conf.read());
            self.settings.send_replace(s);
        }
        if key == "timeout" {
            self.wake.notify_one();
        }
        Ok(self.get())
    }
}
