//! CPU performance profiles (`f50.cpu`): a governor and a per-cluster scaling_min/max, inside the hardware table.
//!
//! - `saving`: every cluster capped at the highest table step at or below 60 % of its maximum, schedutil;
//! - `balanced`: schedutil over the full range (the stock behaviour);
//! - `performance`: the performance governor.
//!
//! A profile never touches voltage or the kernel's trips. The cap it sets is the *ceiling* the thermal guard never
//! lifts a policy above on cool-down, so `saving` stays capped; the kernel's own cap is a frequency QoS request under
//! scaling_max_freq, so the lower of the two wins (docs/research/FINDINGS.md 48). The saved profile lives in
//! `/etc/f50d/cpu.conf` (`PROFILE=`); without a saved one nothing is applied at start (init.d/f50-cpu).
//! Core hotplug, CPU0's low latency, the services' cpuset and the voltage offset were removed with the shell layer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use f50_core::fsx::{read_line, read_num};
use f50_core::{Error, Result};
use f50_platform::Platform;
use serde_json::{Value, json};

use crate::settings::{self, ConfFile};
use crate::thermal;

pub(crate) const PROFILES: &[&str] = &["saving", "balanced", "performance"];
const DEFAULT_PROFILE: &str = "balanced";
const SAVING_PERCENT: u64 = 60;

/// The cpufreq policies (one per cluster: policy0, policy4, policy7 on the UMS9620), in order.
pub(crate) fn policies(platform: &Platform) -> Vec<PathBuf> {
    thermal::numbered(&platform.sysfs().path("/sys/devices/system/cpu/cpufreq"), "policy")
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

pub(crate) fn policy_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The table of a policy, ascending.
pub(crate) fn available(p: &Path) -> Vec<u64> {
    let mut v: Vec<u64> = std::fs::read_to_string(p.join("scaling_available_frequencies"))
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|f| f.parse().ok())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The highest step at or below `percent` % of `hwmax` and not below `hwmin`; `hwmax` when there is none.
pub(crate) fn saving_cap(table: &[u64], hwmin: u64, hwmax: u64, percent: u64) -> u64 {
    let lim = hwmax * percent / 100;
    table.iter().copied().filter(|f| *f <= lim && *f >= hwmin && *f > 0).max().unwrap_or(hwmax)
}

/// A governor the policy has, else its first available one.
fn pick_governor(p: &Path, want: &str) -> Option<String> {
    let govs = read_line(&p.join("scaling_available_governors"))?;
    if govs.split_whitespace().any(|g| g == want) {
        Some(want.to_owned())
    } else {
        govs.split_whitespace().next().map(str::to_owned)
    }
}

/// The CPU profiles and the ceilings they set. Cheap to clone.
#[derive(Debug, Clone)]
pub(crate) struct Cpu {
    platform: Platform,
    conf: ConfFile,
    /// policyN -> Hz: the maximum the applied profile set.
    ceilings: Arc<Mutex<BTreeMap<String, u64>>>,
    /// One apply at a time (an API call and the start must not interleave their writes).
    apply_lock: Arc<Mutex<()>>,
}

impl Cpu {
    pub(crate) fn new(platform: Platform) -> Self {
        let conf = ConfFile::new(&platform, "cpu.conf");
        Cpu { platform, conf, ceilings: Arc::default(), apply_lock: Arc::default() }
    }

    /// The saved profile, when one is saved and valid.
    pub(crate) fn saved(&self) -> Option<String> {
        let text = self.conf.read();
        settings::get(&text, "PROFILE").filter(|p| PROFILES.contains(p)).map(str::to_owned)
    }

    pub(crate) fn profile(&self) -> String {
        self.saved().unwrap_or_else(|| DEFAULT_PROFILE.to_owned())
    }

    /// The ceiling for one policy: what the profile set, else the hardware maximum.
    pub(crate) fn ceiling(&self, policy: &Path) -> Option<u64> {
        let set = self.ceilings.lock().unwrap_or_else(PoisonError::into_inner).get(&policy_name(policy)).copied();
        set.or_else(|| read_num(&policy.join("cpuinfo_max_freq")))
    }

    /// The saved profile, at start; nothing without one.
    pub(crate) fn apply_saved(&self) {
        if let Some(p) = self.saved()
            && let Err(e) = self.apply(&p)
        {
            tracing::warn!("cpu profile {p}: {e}");
        }
    }

    /// Applies `name` to every policy (best effort per attribute, as the shell did).
    pub(crate) fn apply(&self, name: &str) -> Result<()> {
        if !PROFILES.contains(&name) {
            return Err(Error::bad_value("name"));
        }
        let _one = self.apply_lock.lock().unwrap_or_else(PoisonError::into_inner);
        let pols = policies(&self.platform);
        if pols.is_empty() {
            tracing::info!("no cpufreq policies (this kernel has no CPU scaling): nothing to apply");
        }
        for pol in pols {
            let pname = policy_name(&pol);
            let (Some(hwmin), Some(hwmax)) =
                (read_num::<u64>(&pol.join("cpuinfo_min_freq")), read_num::<u64>(&pol.join("cpuinfo_max_freq")))
            else {
                tracing::info!("{pname}: no hardware frequency range, left alone");
                continue;
            };
            let (gov, max) = match name {
                "saving" => {
                    (pick_governor(&pol, "schedutil"), saving_cap(&available(&pol), hwmin, hwmax, SAVING_PERCENT))
                }
                "performance" => (pick_governor(&pol, "performance"), hwmax),
                _ => (pick_governor(&pol, "schedutil"), hwmax),
            };
            // order matters: min first (never above the current max), then max, then the governor
            let _ = std::fs::write(pol.join("scaling_min_freq"), hwmin.to_string());
            let _ = std::fs::write(pol.join("scaling_max_freq"), max.to_string());
            if let Some(g) = gov {
                let _ = std::fs::write(pol.join("scaling_governor"), g);
            }
            self.ceilings.lock().unwrap_or_else(PoisonError::into_inner).insert(pname, max);
        }
        Ok(())
    }

    /// `f50.cpu profile`: apply, then save.
    pub(crate) fn set_profile(&self, name: &str) -> Result<()> {
        self.apply(name)?;
        self.conf.put("PROFILE", Some(name))
    }

    /// `f50.cpu get`: the profile, throttling, the kernel's thermal zones (read only) and every policy, in MHz.
    pub(crate) fn status(&self, heat_alarm: bool) -> Value {
        let zones = thermal::zones(&self.platform);
        let mhz = |p: &Path, a: &str| read_num::<u64>(&p.join(a)).map(|k| k / 1000);
        let pols: Vec<Value> = policies(&self.platform)
            .iter()
            .map(|p| {
                let rel = read_line(&p.join("related_cpus")).unwrap_or_default();
                let mut cpus = rel.split_whitespace();
                let lo = cpus.next().unwrap_or("");
                let hi = cpus.next_back().unwrap_or(lo);
                json!({
                    "name": policy_name(p),
                    "cpus": format!("{lo}-{hi}"),
                    "governor": read_line(&p.join("scaling_governor")),
                    "cur": mhz(p, "scaling_cur_freq"),
                    "min": mhz(p, "scaling_min_freq"),
                    "max": mhz(p, "scaling_max_freq"),
                    "hwmin": mhz(p, "cpuinfo_min_freq"),
                    "hwmax": mhz(p, "cpuinfo_max_freq"),
                })
            })
            .collect();
        json!({
            "profile": self.profile(),
            "saved": self.saved().is_some(),
            "profiles": PROFILES,
            "throttling": thermal::cpu_capped(&self.platform) || heat_alarm,
            "thermal": thermal::details(&zones),
            "policies": pols,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::paths::Paths;

    #[test]
    fn saving_cap_is_a_table_step() {
        let t = [614_400, 1_228_800, 1_536_000, 2_000_000];
        // 60 % of 2.0 GHz is 1.2 GHz: 1_228_800 is above it, so the step below is taken
        assert_eq!(saving_cap(&t, 614_400, 2_000_000, 60), 614_400);
        assert_eq!(saving_cap(&t, 614_400, 2_000_000, 65), 1_228_800);
        // a step exactly at the limit is allowed; one below hwmin is not
        assert_eq!(saving_cap(&t, 614_400, 2_048_000, 60), 1_228_800);
        assert_eq!(saving_cap(&t, 1_000_000, 2_000_000, 60), 2_000_000);
        assert_eq!(saving_cap(&[], 614_400, 2_000_000, 60), 2_000_000);
        assert_eq!(saving_cap(&[2_000_000], 614_400, 2_000_000, 60), 2_000_000);
    }

    #[test]
    fn apply_writes_the_limits_and_the_ceiling() {
        let r = std::env::temp_dir().join(format!("f50-system-cpu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        let pol = r.join("sys/devices/system/cpu/cpufreq/policy4");
        std::fs::create_dir_all(&pol).expect("dir");
        for (k, v) in [
            ("cpuinfo_min_freq", "500000"),
            ("cpuinfo_max_freq", "2000000"),
            ("scaling_available_frequencies", "500000 1000000 1200000 2000000"),
            ("scaling_available_governors", "performance schedutil"),
        ] {
            std::fs::write(pol.join(k), v).expect("write");
        }
        let cpu = Cpu::new(Platform::new(Paths::new(&r), None));
        cpu.set_profile("saving").expect("saving");
        assert_eq!(std::fs::read_to_string(pol.join("scaling_max_freq")).expect("max"), "1200000");
        assert_eq!(std::fs::read_to_string(pol.join("scaling_governor")).expect("gov"), "schedutil");
        assert_eq!(cpu.ceiling(&pol), Some(1_200_000));
        assert_eq!(cpu.saved().as_deref(), Some("saving"));
        assert_eq!(cpu.apply("turbo").map_err(|e| e.code()), Err(f50_core::Code::BadValue));
        let _ = std::fs::remove_dir_all(&r);
    }
}
