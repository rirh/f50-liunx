//! The buttons: every press shows the LEDs first, then what the key and the hold mean.
//!
//! - power, held `long` (3 s): shut down cleanly, as f50-buttons did (the F50 has no battery and starts again
//!   while it has USB power); configurable to reboot or nothing;
//! - the Wi-Fi key, held `long`: the hotspot off/on, forwarded to the binary as [`ButtonAction::WifiToggle`]
//!   (the hotspot is f50-net's; this crate does not depend on it);
//! - the reset key, released after `long` but before `factory`: reboot; held `factory` (10 s): forwarded as
//!   [`ButtonAction::FactoryReset`]. The ten-second hold is the confirmation; a shorter one never wipes anything.
//!
//! The F50's gpio-keys reports only power and the volume keys (docs/research/FINDINGS.md 19), so the Wi-Fi and reset
//! keys use the usual codes (`KEY_WPS_BUTTON`, `KEY_RESTART`) unless the configuration maps others. f50-buttons had
//! no reset key, no band toggle on the F50 and no factory reset; those are new here.

mod input;
mod press;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use f50_platform::Platform;
use tokio::sync::{Notify, broadcast, mpsc};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::{KeyConfig, PowerKeyAction};
use press::{KeyEvent, Presses};

/// What a button asks of another part of f50d. The binary subscribes ([`crate::System::button_actions`]) and acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    /// The hotspot off when it is on, on when it is off.
    WifiToggle,
    /// The reset key was held long enough: wipe the settings and reboot.
    FactoryReset,
}

/// What one key event does here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Wake,
    Forward(ButtonAction),
    Reboot,
    Poweroff,
}

pub(crate) fn decide(keys: &KeyConfig, ev: KeyEvent) -> Option<Effect> {
    match ev {
        KeyEvent::Pressed(_) => Some(Effect::Wake),
        KeyEvent::Held { code, threshold } if code == keys.power && threshold == keys.long => match keys.power_long {
            PowerKeyAction::Poweroff => Some(Effect::Poweroff),
            PowerKeyAction::Reboot => Some(Effect::Reboot),
            PowerKeyAction::Nothing => None,
        },
        KeyEvent::Held { code, threshold } if Some(code) == keys.wifi && threshold == keys.long => {
            Some(Effect::Forward(ButtonAction::WifiToggle))
        }
        KeyEvent::Held { code, threshold } if Some(code) == keys.reset && threshold == keys.factory => {
            Some(Effect::Forward(ButtonAction::FactoryReset))
        }
        KeyEvent::Released { code, held } if Some(code) == keys.reset && held >= keys.long && held < keys.factory => {
            Some(Effect::Reboot)
        }
        _ => None,
    }
}

#[derive(Debug)]
pub(crate) struct Buttons {
    pub keys: KeyConfig,
    pub platform: Platform,
    pub wake: Arc<Notify>,
    pub actions: broadcast::Sender<ButtonAction>,
    pub input_dir: PathBuf,
}

impl Buttons {
    pub(crate) async fn run(self, tracker: TaskTracker, shutdown: CancellationToken) {
        let mut warned = false;
        loop {
            let devices = input::open_key_devices(&self.platform.paths().sys(&self.input_dir));
            if devices.is_empty() {
                if !warned {
                    tracing::warn!(
                        "no input device with keys under {}: looking again every 30 s",
                        self.input_dir.display()
                    );
                    warned = true;
                }
                tokio::select! {
                    () = shutdown.cancelled() => return,
                    () = tokio::time::sleep(Duration::from_secs(30)) => continue,
                }
            }
            warned = false;
            let (tx, rx) = mpsc::channel(64);
            let readers = shutdown.child_token();
            for d in devices {
                tracing::debug!("reading keys from {}", d.path().display());
                tracker.spawn(read_device(d, tx.clone(), readers.clone()));
            }
            drop(tx);
            self.handle(rx, &shutdown).await;
            readers.cancel();
            // every device went away (a driver reloaded): look again shortly
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    }

    async fn handle(&self, mut rx: mpsc::Receiver<(u16, i32)>, shutdown: &CancellationToken) {
        let mut presses = Presses::new(vec![self.keys.long, self.keys.factory]);
        loop {
            let deadline = presses.next_deadline();
            let events = tokio::select! {
                () = shutdown.cancelled() => return,
                k = rx.recv() => match k {
                    Some((code, value)) => presses.on_key(code, value, Instant::now()).into_iter().collect(),
                    None => return,
                },
                () = sleep_opt(deadline) => presses.due(Instant::now()),
            };
            for ev in events {
                if let Some(e) = decide(&self.keys, ev) {
                    self.act(e);
                }
            }
        }
    }

    fn act(&self, e: Effect) {
        match e {
            Effect::Wake => self.wake.notify_one(),
            Effect::Forward(a) => {
                tracing::info!("button: {a:?}");
                if self.actions.send(a).is_err() {
                    tracing::warn!("button: {a:?} asked for, but nothing in f50d handles it");
                }
            }
            Effect::Reboot => {
                tracing::warn!("button held: rebooting");
                if let Err(e) = self.platform.system().reboot_after(Duration::from_secs(1)) {
                    tracing::error!("reboot: {e}");
                }
            }
            Effect::Poweroff => {
                tracing::warn!("power button held: shutting down");
                if let Err(e) = self.platform.system().poweroff_after(Duration::from_secs(1)) {
                    tracing::error!("poweroff: {e}");
                }
            }
        }
    }
}

async fn read_device(mut d: input::KeyDevice, tx: mpsc::Sender<(u16, i32)>, stop: CancellationToken) {
    loop {
        let keys = tokio::select! {
            () = stop.cancelled() => return,
            r = d.next_keys() => r,
        };
        match keys {
            Ok(keys) => {
                for k in keys {
                    if tx.send(k).await.is_err() {
                        return;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("{}: {e}", d.path().display());
                return;
            }
        }
    }
}

async fn sleep_opt(deadline: Option<Instant>) {
    match deadline {
        Some(t) => tokio::time::sleep_until(t).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::keycode;

    #[test]
    fn what_each_hold_does() {
        let k = KeyConfig::default();
        let s = Duration::from_secs;
        let held = |code, t| KeyEvent::Held { code, threshold: s(t) };
        let rel = |code, t| KeyEvent::Released { code, held: s(t) };
        assert_eq!(decide(&k, KeyEvent::Pressed(keycode::VOLUME_UP)), Some(Effect::Wake));
        assert_eq!(decide(&k, held(keycode::POWER, 3)), Some(Effect::Poweroff));
        assert_eq!(decide(&k, rel(keycode::POWER, 1)), None);
        assert_eq!(decide(&k, held(keycode::WPS, 3)), Some(Effect::Forward(ButtonAction::WifiToggle)));
        assert_eq!(decide(&k, held(keycode::RESTART, 3)), None, "the reset key waits for the release");
        assert_eq!(decide(&k, rel(keycode::RESTART, 2)), None);
        assert_eq!(decide(&k, rel(keycode::RESTART, 5)), Some(Effect::Reboot));
        assert_eq!(decide(&k, held(keycode::RESTART, 10)), Some(Effect::Forward(ButtonAction::FactoryReset)));
        assert_eq!(decide(&k, rel(keycode::RESTART, 11)), None, "after the factory reset nothing more");
    }
}
