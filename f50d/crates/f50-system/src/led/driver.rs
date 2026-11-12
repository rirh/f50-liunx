//! The LED task: renders on every change (inputs, settings, the heat alarm, a key, a timer) and writes only what
//! changed to `/sys/class/leds`.
//!
//! Every write is best effort: the PMIC rejects a brightness write now and then, and a cosmetic LED must never make
//! anything fail. A write that failed is tried again at the next rendering.

use std::sync::Arc;
use std::time::Duration;

use f50_platform::Platform;
use tokio::sync::{Notify, watch};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use super::render::{LAMPS, Lamp, Out, Overlay, Scene, render};
use super::{LedInputs, LedSettings};

/// The siren's step (one colour at a time).
const SIREN_STEP: Duration = Duration::from_millis(250);
/// The boot indicator's step on the F50 (kanoqwq's tempo).
const BOOT_STEP: Duration = Duration::from_millis(120);
/// A boot that never says it is done ends its indicator after this.
const BOOT_MAX: Duration = Duration::from_secs(180);
/// In idle, a key shows the LEDs this long when no timeout is set.
const IDLE_PEEK: Duration = Duration::from_secs(60);

pub(crate) struct Driver {
    platform: Platform,
    settings: watch::Receiver<LedSettings>,
    inputs: watch::Receiver<LedInputs>,
    alarm: watch::Receiver<bool>,
    wake: Arc<Notify>,
    present: [bool; 4],
    max: [u32; 4],
    written: [Option<Out>; 4],
    awake_until: Option<Instant>,
    peek_until: Option<Instant>,
    boot_until: Option<Instant>,
    phase: usize,
    next_step: Option<Instant>,
}

impl std::fmt::Debug for Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Driver").field("present", &self.present).field("written", &self.written).finish_non_exhaustive()
    }
}

impl Driver {
    pub(crate) fn new(
        platform: Platform,
        settings: watch::Receiver<LedSettings>,
        inputs: watch::Receiver<LedInputs>,
        alarm: watch::Receiver<bool>,
        wake: Arc<Notify>,
    ) -> Self {
        let sys = platform.sysfs();
        let mut present = [false; 4];
        let mut max = [0; 4];
        for l in LAMPS {
            let dir = format!("/sys/class/leds/{}", l.sysfs_name());
            present[l.index()] = sys.path(&dir).is_dir();
            max[l.index()] = sys.read_num(format!("{dir}/max_brightness")).unwrap_or(255);
        }
        let now = Instant::now();
        let s = settings.borrow().clone();
        // the start is a boot's "LEDs on": shown for the timeout, the boot indicator for at most BOOT_MAX
        let awake_until = (s.timeout_secs() > 0).then(|| now + Duration::from_secs(u64::from(s.timeout_secs())));
        let boot_until = s.boot.then(|| now + BOOT_MAX);
        Driver {
            platform,
            settings,
            inputs,
            alarm,
            wake,
            present,
            max,
            written: [None; 4],
            awake_until,
            peek_until: None,
            boot_until,
            phase: 0,
            next_step: None,
        }
    }

    pub(crate) async fn run(mut self, shutdown: CancellationToken) {
        loop {
            let now = Instant::now();
            let s = self.settings.borrow_and_update().clone();
            let i = *self.inputs.borrow_and_update();
            let alarm = *self.alarm.borrow_and_update();
            let booting = s.boot && !i.booted && self.boot_until.is_some_and(|u| now < u);
            if !booting {
                self.boot_until = None;
            }
            let overlay = Overlay::of(&s, alarm, booting, self.phase);
            let step = match overlay {
                Overlay::Siren(_) => Some(SIREN_STEP),
                Overlay::Boot(_) => Some(BOOT_STEP),
                _ => None,
            };
            match (step, self.next_step) {
                (Some(d), None) => self.next_step = Some(now + d),
                (None, _) => self.next_step = None,
                _ => {}
            }
            let lit = self.lit(&s, &i, now);
            let outs = render(&Scene { settings: &s, inputs: &i, lit, overlay, max: self.max });
            self.apply(outs);

            let deadline = [self.next_step, self.awake_until, self.peek_until, self.boot_until]
                .into_iter()
                .flatten()
                .filter(|t| *t > now)
                .min();
            let wake = Arc::clone(&self.wake);
            let ev = tokio::select! {
                () = shutdown.cancelled() => Ev::Stop,
                r = self.settings.changed() => if r.is_err() { Ev::Stop } else { Ev::Other },
                r = self.inputs.changed() => if r.is_err() { Ev::Stop } else { Ev::Other },
                r = self.alarm.changed() => if r.is_err() { Ev::Stop } else { Ev::Other },
                () = wake.notified() => Ev::Key,
                () = sleep_opt(deadline) => Ev::Other,
            };
            match ev {
                Ev::Stop => break,
                Ev::Key => self.on_key(&s, &i),
                Ev::Other => {}
            }
            if let (Some(d), Some(t)) = (step, self.next_step)
                && Instant::now() >= t
            {
                self.phase = self.phase.wrapping_add(1);
                self.next_step = Some(t + d);
            }
        }
        // the overlays would freeze on one colour: their LEDs go dark; the lamps stay as they are
        if self.next_step.is_some() {
            for l in [Lamp::Red, Lamp::Green, Lamp::Blue] {
                self.write(l, Out::Off);
            }
        }
    }

    /// Shown now: in idle only for a key's peek; else always without a timeout, or until it runs out.
    fn lit(&self, s: &LedSettings, i: &LedInputs, now: Instant) -> bool {
        if i.idle {
            self.peek_until.is_some_and(|u| now < u)
        } else {
            s.timeout_secs() == 0 || self.awake_until.is_some_and(|u| now < u)
        }
    }

    /// A key: the LEDs for the timeout (in idle as well: a key always shows life, the incident of 2026-10-07).
    fn on_key(&mut self, s: &LedSettings, i: &LedInputs) {
        let now = Instant::now();
        let t = Duration::from_secs(u64::from(s.timeout_secs()));
        if i.idle {
            self.peek_until = Some(now + if t.is_zero() { IDLE_PEEK } else { t });
        }
        self.awake_until = (!t.is_zero()).then(|| now + t);
    }

    fn apply(&mut self, outs: [Out; 4]) {
        for l in LAMPS {
            let o = outs[l.index()];
            if self.present[l.index()] && self.written[l.index()] != Some(o) {
                self.write(l, o);
            }
        }
    }

    fn write(&mut self, l: Lamp, o: Out) {
        let sys = self.platform.sysfs();
        let dir = format!("/sys/class/leds/{}", l.sysfs_name());
        let attr = |a: &str, v: &str| sys.write(format!("{dir}/{a}"), v);
        let ok = match o {
            Out::Off => attr("trigger", "none").and(attr("brightness", "0")),
            Out::Steady(level) => attr("trigger", "none").and(attr("brightness", &level.to_string())),
            Out::Blink { level, ms } => match attr("trigger", "timer") {
                Ok(()) => {
                    let _ = attr("delay_on", &ms.to_string());
                    let _ = attr("delay_off", &ms.to_string());
                    // a dimmed lamp: the brightness written under the timer is the blink's
                    if level < self.max[l.index()] { attr("brightness", &level.to_string()) } else { Ok(()) }
                }
                // no timer trigger: steady instead
                Err(_) => attr("trigger", "none").and(attr("brightness", &level.to_string())),
            },
        };
        self.written[l.index()] = ok.is_ok().then_some(o);
    }
}

/// What woke the loop.
enum Ev {
    Stop,
    Key,
    Other,
}

async fn sleep_opt(deadline: Option<Instant>) {
    match deadline {
        Some(t) => sleep_until(t).await,
        None => std::future::pending().await,
    }
}
