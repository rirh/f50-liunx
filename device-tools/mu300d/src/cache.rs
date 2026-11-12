//! Radio readings (signal, serving cell) served from a cache, never from the modem inside a request.
//!
//! The cache is the collector's own files: a read returns what is there with its age (`age_ms`) and, when the
//! reading is stale, starts one detached background refresh, rate-limited by a stamp file, so any number of clients
//! polling costs the modem one round per interval. Nothing is kicked while the modem restarts (a lock apply, a SIM
//! switch), as mu300dash's `status` does. This works the same for `call` (one process per request) and `serve`.
//!
//! Two collectors sit behind the `Collector` trait:
//! * `ScriptCollector`: the control panel's collector (`/usr/libexec/unisoc-modem/cell fast|full`, openwrt-luci),
//!   which writes sig.json and cell.json into the adapters' runtime directory.
//! * `AtCollector`: where that collector is not installed (Ubuntu, plain OpenWrt), `mu300d collect-at` reads
//!   AT+COPS?, AT+CEREG? and AT+CESQ through mu300-at (mu300-api's `signal_json`) into RUN/radio/sig.json. It has
//!   no cell table.

use std::ffi::OsString;
use std::fs;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::config::Config;
use crate::exec::{Tool, executable, spawn_detached};
use crate::jobs::private_dir;

/// Which reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Serving cell and signal (sig.json).
    Fast,
    /// Everything, neighbours included (cell.json).
    Full,
}

pub trait Collector: Send + Sync {
    fn name(&self) -> &'static str;
    /// Where the tier's reading is.
    fn file(&self, tier: Tier) -> Option<PathBuf>;
    /// The program (argv) that refreshes the tier in the background.
    fn refresh_argv(&self, tier: Tier) -> Vec<OsString>;
    /// A reading older than this is refreshed.
    fn ttl(&self, tier: Tier) -> Duration;
}

pub struct ScriptCollector {
    bin: PathBuf,
    dir: PathBuf,
}

impl Collector for ScriptCollector {
    fn name(&self) -> &'static str {
        "unisoc-modem/cell"
    }
    fn file(&self, tier: Tier) -> Option<PathBuf> {
        Some(self.dir.join(if tier == Tier::Fast { "sig.json" } else { "cell.json" }))
    }
    fn refresh_argv(&self, tier: Tier) -> Vec<OsString> {
        vec![self.bin.clone().into_os_string(), if tier == Tier::Fast { "fast" } else { "full" }.into()]
    }
    fn ttl(&self, tier: Tier) -> Duration {
        Duration::from_secs(if tier == Tier::Fast { 3 } else { 10 })
    }
}

pub struct AtCollector {
    exe: PathBuf,
    out: PathBuf,
}

impl Collector for AtCollector {
    fn name(&self) -> &'static str {
        "mu300-at"
    }
    fn file(&self, tier: Tier) -> Option<PathBuf> {
        (tier == Tier::Fast).then(|| self.out.clone())
    }
    fn refresh_argv(&self, _: Tier) -> Vec<OsString> {
        vec![self.exe.clone().into_os_string(), "collect-at".into()]
    }
    fn ttl(&self, _: Tier) -> Duration {
        Duration::from_secs(15)
    }
}

/// The collector this device has, or None (no collector and no mu300-at).
pub fn collector(cfg: &Config) -> Option<Box<dyn Collector>> {
    if executable(&cfg.cell_bin) {
        Some(Box::new(ScriptCollector { bin: cfg.cell_bin.clone(), dir: cfg.dash_dir.clone() }))
    } else if executable(&cfg.tool("mu300-at")) {
        Some(Box::new(AtCollector { exe: cfg.self_exe.clone(), out: at_sig_file(cfg) }))
    } else {
        None
    }
}

fn radio_dir(cfg: &Config) -> PathBuf {
    cfg.run_dir.join("radio")
}

fn at_sig_file(cfg: &Config) -> PathBuf {
    radio_dir(cfg).join("sig.json")
}

/// The modem is restarting (a lock apply, a SIM switch): no reading is started now.
pub fn modem_restarting(cfg: &Config) -> bool {
    ["/run/unisoc-modem-lock-apply", "/run/mu300/sim-switching", "/run/mu300/sim-active-unknown"]
        .iter()
        .any(|p| cfg.sys(p).exists())
}

fn age_of(p: &Path) -> Option<Duration> {
    let m = fs::metadata(p).ok()?.modified().ok()?;
    Some(SystemTime::now().duration_since(m).unwrap_or_default())
}

/// The minimum time between two refreshes of a tier, and how long a started one counts as "refreshing".
fn min_interval(tier: Tier) -> Duration {
    Duration::from_secs(if tier == Tier::Fast { 2 } else { 8 })
}
const REFRESH_WINDOW: Duration = Duration::from_secs(15);

/// Start a refresh unless one was started within `min_interval`; true when one runs (or was just started).
fn kick(cfg: &Config, c: &dyn Collector, tier: Tier) -> bool {
    if private_dir(&cfg.run_dir).is_err() || private_dir(&radio_dir(cfg)).is_err() {
        return false;
    }
    let stamp = radio_dir(cfg).join(if tier == Tier::Fast { "kick.fast" } else { "kick.full" });
    if age_of(&stamp).is_some_and(|a| a < min_interval(tier)) {
        return true;
    }
    let _ = fs::write(&stamp, b"");
    spawn_detached(cfg, &c.refresh_argv(tier), &[]).is_ok()
}

/// A tier's reading: `{data, age_ms, refreshing, modem_busy, collector}`; data is null when there is none yet.
pub fn reading(cfg: &Config, tier: Tier) -> Value {
    let Some(c) = collector(cfg) else {
        return json!({"data": null, "age_ms": null, "refreshing": false, "modem_busy": false, "collector": null});
    };
    let Some(file) = c.file(tier) else {
        return json!({"data": null, "age_ms": null, "refreshing": false, "modem_busy": false,
                      "collector": c.name()});
    };
    let data: Value = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let age = if data.is_null() { None } else { age_of(&file) };
    let busy = modem_restarting(cfg);
    let stale = age.is_none_or(|a| a > c.ttl(tier));
    let mut refreshing = false;
    if stale && !busy {
        refreshing = kick(cfg, c.as_ref(), tier);
    } else if !busy {
        let stamp = radio_dir(cfg).join(if tier == Tier::Fast { "kick.fast" } else { "kick.full" });
        refreshing = age_of(&stamp).is_some_and(|s| s < REFRESH_WINDOW && age.is_some_and(|a| a > s));
    }
    json!({
        "data": data,
        "age_ms": age.map(|a| a.as_millis() as u64),
        "refreshing": refreshing,
        "modem_busy": busy,
        "collector": c.name(),
    })
}

// ------------------------------------------------------------------ the AT collector

/// mu300-api's signal_json from the three replies: operator, access technology, registration, RSRP (dBm), RSRQ (dB).
pub fn parse_signal(cops: &str, cereg: &str, cesq: &str) -> Value {
    let line = |t: &str, p: &str| t.lines().map(str::trim).find(|l| l.starts_with(p)).map(str::to_owned);
    let cops = line(cops, "+COPS:").unwrap_or_default();
    let cereg = line(cereg, "+CEREG:").unwrap_or_default();
    let cesq = line(cesq, "+CESQ:").unwrap_or_default();
    let operator = cops.split('"').nth(1).unwrap_or("").to_owned();
    let field = |s: &str, i: usize| -> String {
        let body = s.split_once(':').map(|(_, b)| b).unwrap_or("");
        body.split(',').nth(i).map(|f| f.trim().to_owned()).unwrap_or_default()
    };
    let rat = match field(&cops, 3).as_str() {
        "7" => "LTE",
        "13" => "5G NSA",
        "11" | "12" => "5G",
        "2" | "4" | "5" | "6" => "3G",
        "0" | "1" | "3" => "2G",
        _ => "",
    };
    let reg = match field(&cereg, 1).as_str() {
        "1" => "home",
        "5" => "roaming",
        "2" => "searching",
        "3" => "denied",
        "0" => "none",
        "8" => "emergency",
        _ => "",
    };
    let num = |s: String| s.parse::<i64>().ok().filter(|n| *n != 255 && *n >= 0);
    let rsrq = num(field(&cesq, 4)).map(|i| -19.5 + i as f64 * 0.5);
    let rsrp = num(field(&cesq, 5)).map(|i| i - 140);
    json!({"operator": operator, "rat": rat, "registration": reg, "rsrp": rsrp, "rsrq": rsrq})
}

/// `mu300d collect-at`: one AT round into RUN/radio/sig.json (one at a time; a second one leaves at once).
pub async fn collect_at(cfg: &Config) -> i32 {
    if private_dir(&cfg.run_dir).is_err() || private_dir(&radio_dir(cfg)).is_err() {
        return 1;
    }
    let Ok(lock) = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(radio_dir(cfg).join("collect.lock"))
    else {
        return 1;
    };
    // SAFETY: flock(2) on a descriptor we own, released when it closes at exit.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return 0;
    }
    let at = |c: &'static str| {
        Tool::new(cfg, cfg.tool("mu300-at")).args(["-t", "5", "-"]).stdin(format!("{c}\n")).timeout_secs(10).run()
    };
    let text = |o: std::io::Result<crate::exec::Output>| o.map(|o| o.stdout.replace('\r', "")).unwrap_or_default();
    let cops = text(at("AT+COPS?").await);
    let cereg = text(at("AT+CEREG?").await);
    let cesq = text(at("AT+CESQ").await);
    let v = parse_signal(&cops, &cereg, &cesq);
    let out = at_sig_file(cfg);
    let tmp = out.with_extension("new");
    let ok = fs::write(&tmp, v.to_string()).and_then(|_| fs::rename(&tmp, &out)).is_ok();
    i32::from(!ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_signal() {
        let v = parse_signal(
            "+COPS: 0,0,\"Turkcell\",7\r\nOK",
            "+CEREG: 2,1,\"1A2B\",\"01234567\",7\nOK",
            "+CESQ: 99,99,255,255,20,45\nOK",
        );
        assert_eq!(v["operator"], "Turkcell");
        assert_eq!(v["rat"], "LTE");
        assert_eq!(v["registration"], "home");
        assert_eq!(v["rsrp"], -95);
        assert_eq!(v["rsrq"], -9.5);
        let e = parse_signal("", "ERROR", "+CESQ: 99,99,255,255,255,255");
        assert_eq!(e["rsrp"], Value::Null);
        assert_eq!(e["rat"], "");
    }
}
