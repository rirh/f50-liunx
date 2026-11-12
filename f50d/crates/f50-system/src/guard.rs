//! The thermal guard: every few seconds, the hottest zone decides caps, the heat alarm and the power-off.
//!
//! - Where an enabled zone with a passive trip has every cpufreq cooling device bound (the stock `soc-thmzone` on
//!   5.4, or mainline's `mu300_thermal`), the kernel throttles and the guard only watches: it lifts the caps it set
//!   itself and leaves scaling_max_freq alone (docs/research/FINDINGS.md 48; asked of every kernel, FINDINGS 139).
//! - Without one, the guard is the policy: from `hot` every policy goes one table step lower per round, at or below
//!   `cool` one step back up, never above the CPU profile's ceiling.
//! - Watching is not trusting: when the kernel throttles but the SoC passes `backstop` (mainline's step_wise held
//!   it to no more than three steps and it reached 102 °C, FINDINGS 166), the guard caps on top of the kernel, the
//!   A76 clusters from `backstop` and policy0 too from `backstop_all`, and lifts those caps again at or below `hot`.
//! - At `crit` it powers off, on every kernel.
//! - At `hot` the heat alarm goes on (the LEDs show it), until the SoC is down to `cool`.
//!
//! A round without any reading (every sensor failing) lifts no cap and ends no alarm: 0 is not a cool SoC. The
//! marker of our caps (`/var/run/f50d/thermal-capped`, "backstop" for the backstop's) outlives a restart, so a cap
//! from before the kernel took over is lifted on the handoff and does not stick for ever.

use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::fsx::read_num;
use f50_platform::Platform;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::config::ThermalConfig;
use crate::cpu::{self, Cpu};
use crate::thermal;

/// What a round does to the policies (decided by [`decide`], a pure function).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Poweroff,
    /// Every policy one step down (we are the policy).
    CapAll,
    /// Every policy one step up towards its ceiling.
    Raise,
    /// One step down on top of the kernel: the A76 clusters, and policy0 too when `all`.
    Backstop {
        all: bool,
    },
    Nothing,
}

/// The decision of one round: the hottest reading (`None`: blind), whether the kernel throttles, whether our caps
/// are held now.
pub(crate) fn decide(cfg: &ThermalConfig, t: Option<i64>, native: bool, capped: bool) -> Step {
    let blind = t.is_none();
    let t = t.unwrap_or(0);
    if t >= cfg.crit {
        Step::Poweroff
    } else if t >= cfg.hot && !native {
        Step::CapAll
    } else if t <= cfg.cool && !native && !blind {
        Step::Raise
    } else if t >= cfg.backstop && native {
        Step::Backstop { all: t >= cfg.backstop_all }
    } else if t <= cfg.hot && native && capped && !blind {
        Step::Raise
    } else {
        Step::Nothing
    }
}

/// The heat alarm after a round: on from `hot`, off at or below `cool` with a reading.
pub(crate) fn alarm_next(cfg: &ThermalConfig, t: Option<i64>, was: bool) -> bool {
    match t {
        Some(t) if !was && t >= cfg.hot => true,
        Some(t) if was && t <= cfg.cool => false,
        _ => was,
    }
}

pub(crate) struct Guard {
    platform: Platform,
    cfg: ThermalConfig,
    cpu: Cpu,
    alarm: watch::Sender<bool>,
    capped: PathBuf,
    native_was: Option<String>,
    first: bool,
    blind: bool,
    powering_off: bool,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard").field("native_was", &self.native_was).finish_non_exhaustive()
    }
}

impl Guard {
    pub(crate) fn new(platform: Platform, cfg: ThermalConfig, cpu: Cpu, alarm: watch::Sender<bool>) -> Self {
        let capped = platform.paths().run().join("thermal-capped");
        Guard { platform, cfg, cpu, alarm, capped, native_was: None, first: true, blind: false, powering_off: false }
    }

    pub(crate) async fn run(mut self, shutdown: CancellationToken) {
        loop {
            self.round();
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = tokio::time::sleep(self.cfg.interval) => {}
            }
        }
    }

    fn round(&mut self) {
        let zones = thermal::zones(&self.platform);
        let t = thermal::hottest(&zones);
        match (t, self.blind) {
            (Some(t), true) => tracing::info!("thermal zones read again ({t} mC)"),
            (None, false) => tracing::warn!("no thermal zone gave a temperature: keeping caps and alarm as they are"),
            _ => {}
        }
        self.blind = t.is_none();
        let native = thermal::native_cpu_cooling(&self.platform, &zones, t.unwrap_or(0), self.cfg.hot);
        self.handoff(native.as_deref());
        let marker = std::fs::read_to_string(&self.capped).ok();
        let step = decide(&self.cfg, t, native.is_some(), marker.is_some());
        let tc = t.unwrap_or(0);
        match step {
            Step::Poweroff => {
                if !self.powering_off {
                    tracing::error!("critical temperature {tc} mC, powering off");
                    self.powering_off = true;
                    if let Err(e) = self.platform.system().poweroff_after(Duration::ZERO) {
                        tracing::error!("poweroff: {e}");
                        self.powering_off = false;
                    }
                }
            }
            Step::CapAll => {
                for p in cpu::policies(&self.platform) {
                    self.step(&p, Dir::Down);
                }
                self.mark("");
                tracing::warn!("hot ({tc} mC): lowered cpufreq limits");
            }
            Step::Raise => self.raise_caps(),
            Step::Backstop { all } => {
                if marker.is_none() {
                    tracing::warn!(
                        "kernel thermal zone {} does not hold the SoC ({tc} mC): capping cpufreq on top of it from {} mC \
                         (the A55 cluster too from {} mC) until {} mC",
                        native.as_deref().unwrap_or("?"),
                        self.cfg.backstop,
                        self.cfg.backstop_all,
                        self.cfg.hot
                    );
                }
                for p in cpu::policies(&self.platform) {
                    if all || cpu::policy_name(&p) != "policy0" {
                        self.step(&p, Dir::Down);
                    }
                }
                self.mark("backstop");
            }
            Step::Nothing => {}
        }
        let was = *self.alarm.borrow();
        let now = alarm_next(&self.cfg, t, was);
        if now != was {
            if now {
                tracing::warn!("too hot (SoC {tc} mC): heat alarm on");
            } else {
                tracing::info!("cooled down (SoC {tc} mC): heat alarm off");
            }
            self.alarm.send_replace(now);
        }
    }

    /// The kernel took over or let go since the last round.
    fn handoff(&mut self, native: Option<&str>) {
        match (native, self.native_was.as_deref()) {
            (Some(n), was) if was != Some(n) => {
                tracing::info!(
                    "kernel thermal zone {n} throttles the CPUs: watching only, power-off at {} mC stays",
                    self.cfg.crit
                );
                self.lift_caps();
            }
            (None, Some(was)) => tracing::warn!("kernel thermal zone {was} gone: capping cpufreq again"),
            (None, None) if self.first => tracing::info!(
                "no kernel thermal zone throttles the CPUs: capping cpufreq here above {} mC, power-off at {} mC",
                self.cfg.hot,
                self.cfg.crit
            ),
            _ => {}
        }
        self.native_was = native.map(str::to_owned);
        self.first = false;
    }

    fn mark(&self, what: &str) {
        if let Some(d) = self.capped.parent() {
            let _ = f50_core::fsx::private_dir(d);
        }
        if let Err(e) = f50_core::fsx::write_atomic(&self.capped, what.as_bytes(), 0o600) {
            tracing::warn!("{e}");
        }
    }

    /// Every policy back to its ceiling when we hold a cap from before the kernel throttled; the backstop's stay.
    fn lift_caps(&self) {
        match std::fs::read_to_string(&self.capped) {
            Ok(m) if m.trim() != "backstop" => {
                for p in cpu::policies(&self.platform) {
                    if let Some(c) = self.cpu.ceiling(&p) {
                        let _ = std::fs::write(p.join("scaling_max_freq"), c.to_string());
                    }
                }
                let _ = std::fs::remove_file(&self.capped);
            }
            _ => {}
        }
    }

    /// Every policy one step up towards its ceiling; the marker goes when none is below it.
    fn raise_caps(&self) {
        let mut below = false;
        for p in cpu::policies(&self.platform) {
            let cur: Option<u64> = read_num(&p.join("scaling_max_freq"));
            let max: Option<u64> = read_num(&p.join("cpuinfo_max_freq"));
            if let (Some(c), Some(m)) = (cur, max)
                && c < m
            {
                self.step(&p, Dir::Up);
            }
            let cur: Option<u64> = read_num(&p.join("scaling_max_freq"));
            if let (Some(c), Some(ceil)) = (cur, self.cpu.ceiling(&p)) {
                below |= c < ceil;
            }
        }
        if !below {
            let _ = std::fs::remove_file(&self.capped);
        }
    }

    fn step(&self, p: &Path, dir: Dir) {
        let Some(cur) = read_num::<u64>(&p.join("scaling_max_freq")) else { return };
        if let Some(next) = next_step(&cpu::available(p), cur, dir, self.cpu.ceiling(p)) {
            let _ = std::fs::write(p.join("scaling_max_freq"), next.to_string());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    Down,
    Up,
}

/// The neighbouring table step of `cur`; going up, never above `ceiling`.
pub(crate) fn next_step(table: &[u64], cur: u64, dir: Dir, ceiling: Option<u64>) -> Option<u64> {
    match dir {
        Dir::Down => table.iter().copied().filter(|f| *f < cur).max(),
        Dir::Up => table.iter().copied().filter(|f| *f > cur && ceiling.is_none_or(|c| *f <= c)).min(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds() {
        let c = ThermalConfig::default();
        assert_eq!(decide(&c, Some(106_000), true, false), Step::Poweroff);
        assert_eq!(decide(&c, Some(86_000), false, false), Step::CapAll);
        assert_eq!(decide(&c, Some(70_000), false, true), Step::Raise);
        assert_eq!(decide(&c, None, false, true), Step::Nothing, "blind: no cap lifted");
        assert_eq!(decide(&c, Some(80_000), false, true), Step::Nothing);
        assert_eq!(decide(&c, Some(93_000), true, false), Step::Backstop { all: false });
        assert_eq!(decide(&c, Some(96_000), true, true), Step::Backstop { all: true });
        assert_eq!(decide(&c, Some(85_000), true, true), Step::Raise);
        assert_eq!(decide(&c, Some(85_000), true, false), Step::Nothing, "the kernel's caps are not ours");
    }

    #[test]
    fn alarm_has_hysteresis_and_ignores_blind_rounds() {
        let c = ThermalConfig::default();
        assert!(alarm_next(&c, Some(85_000), false));
        assert!(alarm_next(&c, Some(80_000), true));
        assert!(alarm_next(&c, None, true));
        assert!(!alarm_next(&c, Some(75_000), true));
    }

    #[test]
    fn steps_respect_the_ceiling() {
        let t = [500, 1000, 1500, 2000];
        assert_eq!(next_step(&t, 2000, Dir::Down, None), Some(1500));
        assert_eq!(next_step(&t, 500, Dir::Down, None), None);
        assert_eq!(next_step(&t, 1000, Dir::Up, Some(1000)), None);
        assert_eq!(next_step(&t, 1000, Dir::Up, Some(2000)), Some(1500));
    }
}
