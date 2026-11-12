//! The system the daemon runs on and the small facts and settings files mu300-api reads inline (os-release, KEY=VALUE
//! files, UCI, systemd units, the LAN address).

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;

use crate::api::error::{ApiError, ApiResult};
use crate::config::Config;
use crate::exec::{Tool, executable};

/// Which Linux this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Ubuntu,
    OpenWrt,
    /// OpenWrt with the LuCI control panel's package (its adapters: the SMS pool, device-usb).
    OpenWrtLuci,
}

impl System {
    pub fn detect(cfg: &Config) -> Self {
        if cfg.sys("/etc/openwrt_release").exists() {
            if executable(&cfg.sys("/usr/libexec/rpcd/mu300dash")) { System::OpenWrtLuci } else { System::OpenWrt }
        } else {
            System::Ubuntu
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            System::Ubuntu => "ubuntu",
            System::OpenWrt => "openwrt",
            System::OpenWrtLuci => "openwrt-luci",
        }
    }
    pub fn is_openwrt(self) -> bool {
        self != System::Ubuntu
    }
    pub fn is_luci(self) -> bool {
        self == System::OpenWrtLuci
    }
    /// The account the device password belongs to (system.password, auth.login).
    pub fn login_user(self) -> &'static str {
        if self.is_openwrt() { "root" } else { "ubuntu" }
    }
}

/// What every method gets: the configuration and the system. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub cfg: Arc<Config>,
    pub sys: System,
}

impl Ctx {
    pub fn new(cfg: Config) -> Self {
        let sys = System::detect(&cfg);
        Ctx { cfg: Arc::new(cfg), sys }
    }

    pub fn tool(&self, name: &str) -> Tool {
        Tool::new(&self.cfg, self.cfg.tool(name))
    }

    /// A device tool, or `unsupported` when it is not installed.
    pub fn need_tool(&self, name: &str) -> ApiResult<Tool> {
        let p = self.cfg.tool(name);
        if executable(&p) { Ok(Tool::new(&self.cfg, p)) } else { Err(ApiError::unsupported(name)) }
    }

    pub fn has_tool(&self, name: &str) -> bool {
        executable(&self.cfg.tool(name))
    }

    /// A system command (uci, systemctl, iw, ip, chpasswd ...), found on the configured PATH.
    pub fn cmd(&self, name: &str) -> Tool {
        Tool::new(&self.cfg, name)
    }

    /// `uci -q [-c ROOT/etc/config]`.
    pub fn uci(&self) -> Tool {
        let t = self.cmd("uci").arg("-q");
        if self.cfg.sysroot.as_os_str().is_empty() { t } else { t.arg("-c").arg(self.cfg.sys("/etc/config")) }
    }

    pub async fn uci_get(&self, key: &str) -> String {
        match self.uci().arg("get").arg(key).timeout_secs(5).run().await {
            Ok(o) if o.success() => o.stdout.trim_end_matches('\n').to_owned(),
            _ => String::new(),
        }
    }

    /// `uci batch` with SCRIPT on its stdin (values in it quoted with `uq`).
    pub async fn uci_batch(&self, script: String) -> ApiResult<()> {
        self.uci()
            .arg("batch")
            .stdin(script)
            .timeout_secs(15)
            .ok()
            .await
            .map(|_| ())
            .map_err(|_| ApiError::failed("uci"))
    }

    /// A one-line answer of a quick command, or "".
    pub async fn line_of(&self, t: Tool) -> String {
        match t.timeout_secs(10).run().await {
            Ok(o) if o.success() => o.stdout.lines().next().unwrap_or("").trim().to_owned(),
            _ => String::new(),
        }
    }

    /// The service is active (systemd), or running (OpenWrt's init.d).
    pub async fn svc_active(&self, unit: &str) -> bool {
        if self.sys.is_openwrt() {
            let init = self.cfg.sys(&format!("/etc/init.d/{unit}"));
            executable(&init)
                && Tool::new(&self.cfg, init).arg("running").timeout_secs(5).run().await.is_ok_and(|o| o.success())
        } else {
            self.systemctl_state(unit).await == "active"
        }
    }

    pub async fn systemctl_state(&self, unit: &str) -> String {
        match self.cmd("systemctl").arg("is-active").arg(unit).timeout_secs(5).run().await {
            Ok(o) => o.stdout.trim().to_owned(),
            Err(_) => String::new(),
        }
    }

    /// The device's name (f50, u30air, ...), from mu300-device.
    pub async fn device(&self) -> String {
        let d = self.line_of(self.tool("mu300-device")).await;
        if d.is_empty() { "f50".into() } else { d }
    }

    /// The LAN address of the device (UCI or lan.conf, then mu300-lan-ip, then 192.168.77.1).
    pub async fn lan_ip(&self) -> String {
        let ip = if self.sys.is_openwrt() {
            self.uci_get("network.lan.ipaddr").await
        } else {
            conf_unq(&self.cfg.sys("/etc/mu300/lan.conf"), "LAN_IP").unwrap_or_default()
        };
        let ip = ip.split('/').next().unwrap_or("").to_owned();
        if !ip.is_empty() {
            return ip;
        }
        let d = self.line_of(self.tool("mu300-lan-ip")).await;
        if d.is_empty() { "192.168.77.1".into() } else { d }
    }

    pub fn release(&self) -> String {
        first_line(&self.cfg.sys("/etc/mu300/image-version"))
    }

    /// "ID VERSION_ID" of os-release.
    pub fn os_name(&self) -> String {
        let text = fs::read_to_string(self.cfg.sys("/etc/os-release")).unwrap_or_default();
        let get = |k: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
                .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_owned())
                .unwrap_or_default()
        };
        format!("{} {}", get("ID"), get("VERSION_ID")).trim().to_owned()
    }
}

/// The first line of a file, without CR, or "".
pub fn first_line(p: &Path) -> String {
    fs::read_to_string(p)
        .ok()
        .and_then(|t| t.lines().next().map(|l| l.trim_end_matches('\r').to_owned()))
        .unwrap_or_default()
}

/// KEY's last value in a KEY=VALUE file, exactly as written.
pub fn conf_get(p: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(p).ok()?;
    text.lines().rev().find_map(|l| l.strip_prefix(key).and_then(|v| v.strip_prefix('='))).map(str::to_owned)
}

/// As `conf_get`, one pair of quotes taken off (as systemd reads it).
pub fn conf_unq(p: &Path, key: &str) -> Option<String> {
    conf_get(p, key).map(|v| {
        let b = v.as_bytes();
        if v.len() >= 2 && (b[0] == b'"' && b[v.len() - 1] == b'"' || b[0] == b'\'' && b[v.len() - 1] == b'\'') {
            v[1..v.len() - 1].to_owned()
        } else {
            v
        }
    })
}

/// FILE with those keys replaced (the other lines as they were), written whole with MODE and renamed over the old
/// one; a pair with an empty value removes the key. mu300-api's `conf_put`.
pub fn conf_put(p: &Path, mode: u32, pairs: &[(&str, &str)]) -> std::io::Result<()> {
    let old = fs::read_to_string(p).unwrap_or_default();
    let mut lines: Vec<String> = old
        .lines()
        .filter(|l| !pairs.iter().any(|(k, _)| l.strip_prefix(k).is_some_and(|r| r.starts_with('='))))
        .map(str::to_owned)
        .collect();
    for (k, v) in pairs {
        if !v.is_empty() {
            lines.push(format!("{k}={v}"));
        }
    }
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("new");
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(&tmp)?;
    f.write_all((lines.join("\n") + "\n").as_bytes())?;
    f.sync_all()?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, p)
}

/// VALUE single-quoted for `uci batch`.
pub fn uq(v: &str) -> String {
    format!("'{}'", v.replace('\'', r"'\''"))
}

/// Seconds since the epoch.
pub fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Milliseconds since the epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_roundtrip() {
        let d = std::env::temp_dir().join(format!("mu300d-conf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let f = d.join("x.conf");
        fs::create_dir_all(&d).unwrap();
        fs::write(&f, "# c\nSSID=old\nPSK=\"a b\"\nKEEP=1\n").unwrap();
        assert_eq!(conf_unq(&f, "PSK").as_deref(), Some("a b"));
        conf_put(&f, 0o600, &[("SSID", "new"), ("KEEP", "")]).unwrap();
        let t = fs::read_to_string(&f).unwrap();
        assert_eq!(t, "# c\nPSK=\"a b\"\nSSID=new\n");
        assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn uci_quote() {
        assert_eq!(uq("a'b"), r"'a'\''b'");
    }
}
