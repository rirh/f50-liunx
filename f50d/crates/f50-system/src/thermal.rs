//! The kernel's thermal zones and cooling devices, read in one pass.
//!
//! Zones are matched by type, never by number (dashboard-info). A reading that is not a number is no reading: an
//! unreadable sensor is never reported as cold (docs/research/FINDINGS.md 48: `mu300_thermal`'s `soc-thmzone` fails
//! its read when none of its zones reads).

use std::path::{Path, PathBuf};

use f50_core::fsx::{read_line, read_num};
use f50_platform::Platform;
use serde::Serialize;

/// One thermal zone, as read this pass.
#[derive(Debug, Clone)]
pub(crate) struct Zone {
    pub dir: PathBuf,
    pub kind: String,
    /// Millidegrees Celsius.
    pub temp: Option<i64>,
}

/// The entries of `dir` whose names start with `prefix`, sorted by their number.
pub(crate) fn numbered(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(u32, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let n = name.to_str()?.strip_prefix(prefix)?.parse().ok()?;
            Some((n, e.path()))
        })
        .collect();
    v.sort_by_key(|(n, _)| *n);
    v.into_iter().map(|(_, p)| p).collect()
}

/// A temperature attribute: a whole number (the shell's `-?[0-9]+`).
fn read_temp(p: &Path) -> Option<i64> {
    read_line(p)?.parse().ok()
}

/// Every zone, one read each.
pub(crate) fn zones(platform: &Platform) -> Vec<Zone> {
    numbered(&platform.sysfs().path("/sys/class/thermal"), "thermal_zone")
        .into_iter()
        .filter_map(|dir| {
            let kind = read_line(&dir.join("type"))?;
            let temp = read_temp(&dir.join("temp"));
            Some(Zone { dir, kind, temp })
        })
        .collect()
}

/// The hottest reading; `None` when no zone gave a number.
pub(crate) fn hottest(zones: &[Zone]) -> Option<i64> {
    zones.iter().filter_map(|z| z.temp).max()
}

/// The four zones this SoC labels, and the hottest of all, in °C with one decimal.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct Temps {
    pub soc: Option<f64>,
    pub cpu: Option<f64>,
    pub modem: Option<f64>,
    pub board: Option<f64>,
    pub hottest: Option<f64>,
    pub zones: Vec<ZoneTemp>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ZoneTemp {
    pub name: String,
    pub millicelsius: i64,
}

fn celsius(mc: i64) -> f64 {
    // one decimal, as the page shows it
    (mc as f64 / 100.0).round() / 10.0
}

pub(crate) fn temps(zones: &[Zone]) -> Temps {
    let of = |kind: &str| zones.iter().find(|z| z.kind == kind).and_then(|z| z.temp).map(celsius);
    Temps {
        soc: of("soc-thmzone"),
        cpu: of("apcpu0-thmzone"),
        modem: of("nr0-thmzone"),
        board: of("board-thmzone"),
        hottest: hottest(zones).map(celsius),
        zones: zones.iter().filter_map(|z| Some(ZoneTemp { name: z.kind.clone(), millicelsius: z.temp? })).collect(),
    }
}

/// A cooling device.
#[derive(Debug, Clone)]
pub(crate) struct Cooling {
    pub dir: PathBuf,
    pub kind: String,
    pub cur: u64,
    pub max: u64,
}

pub(crate) fn cooling_devices(platform: &Platform) -> Vec<Cooling> {
    numbered(&platform.sysfs().path("/sys/class/thermal"), "cooling_device")
        .into_iter()
        .filter_map(cooling_at)
        .collect()
}

fn cooling_at(dir: PathBuf) -> Option<Cooling> {
    let kind = read_line(&dir.join("type"))?;
    let cur = read_num(&dir.join("cur_state")).unwrap_or(0);
    let max = read_num(&dir.join("max_state")).unwrap_or(0);
    Some(Cooling { dir, kind, cur, max })
}

fn is_cpufreq(kind: &str) -> bool {
    kind.starts_with("cpufreq-") || kind.starts_with("thermal-cpufreq-")
}

/// A zone's trip points, in order.
fn trips(zone: &Path) -> Vec<(String, Option<i64>)> {
    (0..)
        .map_while(|i| {
            let kind = read_line(&zone.join(format!("trip_point_{i}_type")))?;
            Some((kind, read_temp(&zone.join(format!("trip_point_{i}_temp")))))
        })
        .collect()
}

/// The cooling devices bound to a zone (its `cdevK` links; `cdevK_trip_point` and `cdevK_weight` are files).
fn bound(zone: &Path) -> Vec<PathBuf> {
    numbered(zone, "cdev").into_iter().filter(|p| p.is_dir()).collect()
}

/// Does the kernel throttle the CPUs itself? The type of the zone that does, when an enabled zone with a passive
/// trip at or below `hot` has every cpufreq cooling device bound and reads within 15 °C of `hottest` (a zone whose
/// sensor fails, or a stale or zero reading, would not make the kernel throttle). Decided again every round; on
/// every kernel, the vendor 5.4 one too (docs/research/FINDINGS.md 48 and 139).
pub(crate) fn native_cpu_cooling(platform: &Platform, zones: &[Zone], hottest: i64, hot: i64) -> Option<String> {
    let cpu_cdevs: Vec<PathBuf> = cooling_devices(platform)
        .into_iter()
        .filter(|c| is_cpufreq(&c.kind))
        .filter_map(|c| std::fs::canonicalize(&c.dir).ok())
        .collect();
    if cpu_cdevs.is_empty() {
        return None;
    }
    zones.iter().find_map(|z| {
        if read_line(&z.dir.join("mode")).as_deref() != Some("enabled") {
            return None;
        }
        let passive_ok = trips(&z.dir).iter().any(|(k, t)| k == "passive" && t.is_some_and(|t| t >= 0 && t <= hot));
        if !passive_ok {
            return None;
        }
        let t = z.temp.filter(|t| *t >= 0)?;
        if t < hottest - 15_000 {
            return None;
        }
        let links: Vec<PathBuf> = bound(&z.dir).iter().filter_map(|l| std::fs::canonicalize(l).ok()).collect();
        cpu_cdevs.iter().all(|c| links.contains(c)).then(|| z.kind.clone())
    })
}

/// A cpufreq cooling device caps a policy now.
pub(crate) fn cpu_capped(platform: &Platform) -> bool {
    cooling_devices(platform).iter().any(|c| c.kind.contains("cpufreq") && c.cur > 0)
}

/// A zone for the CPU page's read-only thermal section.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ZoneDetail {
    pub zone: String,
    pub temp: Option<i64>,
    pub mode: Option<String>,
    pub trips: Vec<Trip>,
    pub cooling: Vec<CoolingState>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Trip {
    #[serde(rename = "type")]
    pub kind: String,
    pub temp: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CoolingState {
    #[serde(rename = "type")]
    pub kind: String,
    pub state: u64,
    pub max: u64,
}

pub(crate) fn details(zones: &[Zone]) -> Vec<ZoneDetail> {
    zones
        .iter()
        .map(|z| ZoneDetail {
            zone: z.kind.clone(),
            temp: z.temp,
            mode: read_line(&z.dir.join("mode")),
            trips: trips(&z.dir).into_iter().map(|(kind, temp)| Trip { kind, temp }).collect(),
            cooling: bound(&z.dir)
                .into_iter()
                .filter_map(cooling_at)
                .map(|c| CoolingState { kind: c.kind, state: c.cur, max: c.max })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::paths::Paths;

    fn root(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f50-system-thermal-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("tmp");
        d
    }

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().expect("parent")).expect("dir");
        std::fs::write(p, s).expect("write");
    }

    #[test]
    fn one_pass_labels_and_the_kernel_handoff() {
        let r = root("pass");
        let th = r.join("sys/class/thermal");
        write(&th.join("thermal_zone0/type"), "soc-thmzone\n");
        write(&th.join("thermal_zone0/temp"), "86500\n");
        write(&th.join("thermal_zone0/mode"), "enabled\n");
        write(&th.join("thermal_zone0/trip_point_0_type"), "passive\n");
        write(&th.join("thermal_zone0/trip_point_0_temp"), "85000\n");
        write(&th.join("thermal_zone1/type"), "board-thmzone\n");
        write(&th.join("thermal_zone1/temp"), "garbage\n");
        write(&th.join("cooling_device0/type"), "cpufreq-cpu0\n");
        write(&th.join("cooling_device0/cur_state"), "2\n");
        let p = Platform::new(Paths::new(&r), None);
        let z = zones(&p);
        assert_eq!(z.len(), 2);
        let t = temps(&z);
        assert_eq!((t.soc, t.board, t.hottest), (Some(86.5), None, Some(86.5)));
        assert_eq!(t.zones.len(), 1);
        assert!(cpu_capped(&p));
        // not bound yet: the guard caps
        assert_eq!(native_cpu_cooling(&p, &z, 86_500, 85_000), None);
        std::os::unix::fs::symlink(th.join("cooling_device0"), th.join("thermal_zone0/cdev0")).expect("link");
        assert_eq!(native_cpu_cooling(&p, &z, 86_500, 85_000).as_deref(), Some("soc-thmzone"));
        // a stale reading far below the die is not trusted
        assert_eq!(native_cpu_cooling(&p, &z, 102_000, 85_000), None);
        let _ = std::fs::remove_dir_all(&r);
    }
}
