//! The write side of `f50.system`: hostname, timezone, password, reboot, power-off, logs. The validation is
//! f50-api's (`m_system_hostname`, `m_system_timezone`, `m_system_password`, `m_logs`).

use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::rpc::matches;
use f50_core::sanitize::{sanitize_text, tail_bytes};
use f50_core::{Error, Result};
use serde_json::{Value, json};

use crate::System;

/// `name`: a single label, as f50-api took it.
pub(crate) const HOSTNAME_RE: &str = "[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?";
pub(crate) const ZONE_RE: &str = "UTC|[A-Z][A-Za-z_+-]{1,30}(/[A-Za-z0-9_+-]{1,30}){1,2}";
const TZ_RE: &str = "[A-Za-z0-9<>+,./:-]{1,64}";
/// The logs' reply: at most this many bytes, the newest kept.
const LOGS_MAX: usize = 256 * 1024;
/// The reply leaves first: the action waits this long.
const ACTION_DELAY: Duration = Duration::from_secs(2);

/// OpenWrt keeps a POSIX TZ string: the last line of the zone's file (`UTC0` for UTC).
pub(crate) fn tz_of(zone: &str, zonefile: Option<&str>) -> Result<String> {
    if zone.contains("..") || !matches(ZONE_RE, zone) {
        return Err(Error::bad_value("zone"));
    }
    if zone == "UTC" {
        return Ok("UTC0".into());
    }
    let tz = zonefile.and_then(|t| t.lines().rev().find(|l| !l.trim().is_empty())).unwrap_or("").trim();
    if matches(TZ_RE, tz) { Ok(tz.to_owned()) } else { Err(Error::unsupported(format!("zoneinfo for {zone}"))) }
}

impl System {
    pub(crate) async fn set_hostname(&self, name: &str) -> Result<Value> {
        if !matches(HOSTNAME_RE, name) {
            return Err(Error::bad_value("name"));
        }
        self.inner.platform.system().set_hostname(name).await?;
        Ok(json!({ "hostname": name }))
    }

    pub(crate) async fn set_timezone(&self, zone: &str) -> Result<Value> {
        let file = self.inner.platform.paths().sys(format!("/usr/share/zoneinfo/{zone}"));
        let text = if zone != "UTC" && !zone.contains("..") {
            // a zone file holds binary data before the TZ footer
            std::fs::read(&file).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
        } else {
            None
        };
        let tz = tz_of(zone, text.as_deref())?;
        self.inner.platform.system().set_timezone(zone, &tz).await?;
        Ok(json!({ "timezone": zone }))
    }

    pub(crate) async fn set_password(&self, password: &str) -> Result<Value> {
        if password.is_empty() {
            return Err(Error::missing("password"));
        }
        self.inner.platform.system().set_password("root", password).await?;
        Ok(json!({ "user": "root" }))
    }

    pub(crate) fn reboot(&self) -> Result<Value> {
        self.inner.platform.system().reboot_after(ACTION_DELAY)?;
        Ok(json!({}))
    }

    pub(crate) fn poweroff(&self) -> Result<Value> {
        self.inner.platform.system().poweroff_after(ACTION_DELAY)?;
        Ok(json!({}))
    }

    /// The kernel's and the system log's last `lines` lines (`dmesg`, `logread`), sanitised, the newest 256 KiB.
    pub(crate) async fn logs(&self, lines: usize) -> Result<Value> {
        let tail = |s: String| {
            let v: Vec<&str> = s.lines().collect();
            v[v.len().saturating_sub(lines)..].join("\n")
        };
        let run = |p: &'static str| Cmd::new(p).timeout(Duration::from_secs(20)).max_output(2 * 1024 * 1024).output();
        let dmesg = run("dmesg").await.map(|o| tail(o.stdout)).unwrap_or_default();
        let logread = run("logread").await.map(|o| tail(o.stdout)).unwrap_or_default();
        let text = format!("--- dmesg (last {lines} lines)\n{dmesg}\n--- system log (last {lines} lines)\n{logread}\n");
        Ok(json!({ "text": sanitize_text(tail_bytes(&text, LOGS_MAX)) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_as_the_shell_api() {
        assert_eq!(tz_of("UTC", None).expect("utc"), "UTC0");
        assert_eq!(tz_of("Europe/Istanbul", Some("TZif...\n<+03>-3\n")).expect("ist"), "<+03>-3");
        assert_eq!(tz_of("Europe/Nowhere", None).map_err(|e| e.code()), Err(f50_core::Code::Unsupported));
        assert_eq!(tz_of("Europe/../etc", None).map_err(|e| e.code()), Err(f50_core::Code::BadValue));
        assert_eq!(tz_of("europe/x", None).map_err(|e| e.code()), Err(f50_core::Code::BadValue));
        assert!(matches(HOSTNAME_RE, "F50-home") && !matches(HOSTNAME_RE, "-x") && !matches(HOSTNAME_RE, "a.b"));
    }
}
