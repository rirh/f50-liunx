//! The ZTE F50 as a system: facts and live figures, CPU profiles, the thermal guard, the LEDs, the buttons, zram,
//! hostname/time zone/password, reboot and power-off. Publishes `f50.system`, `f50.cpu` and `f50.led`.
//!
//! [`System::start`] refuses any board but the F50 ([`device`]), applies the saved CPU profile, and starts the
//! background parts on the caller's [`TaskTracker`], each stopped by its [`CancellationToken`]:
//!
//! - zram setup, once ([`zram`]);
//! - the thermal guard ([`guard`], [`thermal`]), which also raises the heat alarm;
//! - the LED driver ([`led`]), fed by the binary through [`System::set_led_inputs`];
//! - the key reader ([`buttons`]), whose requests for other crates arrive on [`System::button_actions`].
//!
//! Ported from dashboard-info (non-modem parts), f50-device, f50-cpu, thermal-guard, f50-led, f50-buttons
//! with f50-keys, f50-zram/zram-start and f50-api's system and CPU methods. Settings f50d keeps for itself are
//! `cpu.conf`, `led.conf` and `zram.conf` under `/etc/f50d`, in the shell's `KEY=VALUE` format ([`settings`]).

mod admin;
mod buttons;
mod config;
mod cpu;
mod dash;
mod device;
mod facts;
mod guard;
mod info;
pub mod led;
mod objects;
mod settings;
mod thermal;
mod zram;

use std::sync::Arc;

use f50_core::Result;
use f50_core::rpc::Object;
use f50_platform::Platform;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use buttons::ButtonAction;
pub use config::{KeyConfig, PowerKeyAction, SystemConfig, ThermalConfig, keycode};
pub use led::{LedInputs, NetworkLed};

/// The system service. Cheap to clone.
#[derive(Debug, Clone)]
pub struct System {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    cfg: SystemConfig,
    platform: Platform,
    cpu: cpu::Cpu,
    leds: led::LedHandle,
    alarm: watch::Sender<bool>,
    buttons: broadcast::Sender<ButtonAction>,
}

impl System {
    /// Checks the device, applies the saved CPU profile and starts the background parts. Must run inside the
    /// Tokio runtime.
    pub fn start(
        cfg: SystemConfig,
        platform: Platform,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<System> {
        device::require_f50(&platform)?;
        let cpu = cpu::Cpu::new(platform.clone());
        let sys = System {
            inner: Arc::new(Inner {
                leds: led::LedHandle::load(&platform),
                alarm: watch::Sender::new(false),
                buttons: broadcast::Sender::new(8),
                cpu,
                platform,
                cfg,
            }),
        };
        let i = &sys.inner;
        if i.cfg.cpu_profile_at_start {
            i.cpu.apply_saved();
        }
        if i.cfg.zram {
            let p = i.platform.clone();
            let stop = shutdown.clone();
            tracker.spawn(async move {
                tokio::select! {
                    () = stop.cancelled() => {}
                    r = zram::start(&p) => if let Err(e) = r { tracing::warn!("zram: {e}") },
                }
            });
        }
        if i.cfg.thermal_guard {
            let g = guard::Guard::new(i.platform.clone(), i.cfg.thermal.clone(), i.cpu.clone(), i.alarm.clone());
            tracker.spawn(g.run(shutdown.clone()));
        }
        if i.cfg.leds {
            let d = led::driver::Driver::new(
                i.platform.clone(),
                i.leds.settings.subscribe(),
                i.leds.inputs.subscribe(),
                i.alarm.subscribe(),
                Arc::clone(&i.leds.wake),
            );
            tracker.spawn(d.run(shutdown.clone()));
        }
        if i.cfg.buttons {
            let b = buttons::Buttons {
                keys: i.cfg.keys.clone(),
                platform: i.platform.clone(),
                wake: Arc::clone(&i.leds.wake),
                actions: i.buttons.clone(),
                input_dir: i.cfg.input_dir.clone(),
            };
            tracker.spawn(b.run(tracker.clone(), shutdown));
        }
        Ok(sys)
    }

    /// `f50.system`, `f50.cpu`, `f50.led`.
    pub fn objects(&self) -> Vec<Object> {
        objects::objects(self)
    }

    /// What the LEDs show from now on (the binary feeds this from the modem, data and Wi-Fi crates).
    pub fn set_led_inputs(&self, inputs: LedInputs) {
        self.inner.leds.inputs.send_if_modified(|cur| {
            let changed = *cur != inputs;
            *cur = inputs;
            changed
        });
    }

    /// Shows the LEDs for the timeout, as a key press does.
    pub fn wake_leds(&self) {
        self.inner.leds.wake.notify_one();
    }

    /// The buttons' requests for other crates (the hotspot toggle, the factory reset).
    pub fn button_actions(&self) -> broadcast::Receiver<ButtonAction> {
        self.inner.buttons.subscribe()
    }

    /// The thermal guard's heat alarm.
    pub fn heat_alarm(&self) -> watch::Receiver<bool> {
        self.inner.alarm.subscribe()
    }
}
