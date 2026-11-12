//! System settings and power: hostname, timezone, passwords, reboot, sysctl.
//!
//! Settings are saved in UCI (`system.@system[0]`) and applied at once the way OpenWrt's `/etc/init.d/system`
//! applies them at boot: the hostname to `/proc/sys/kernel/hostname`, the POSIX TZ string to `/tmp/TZ`.
//!
//! Passwords go through busybox `passwd <user>`, the password twice on its stdin, as LuCI's rpcd does
//! (`luci.setPassword`). OpenWrt's busybox has `passwd` but no `chpasswd` (the shell layer carried a static busybox
//! for that); `passwd` hashes with the image's configured algorithm, takes `/etc/shadow`'s lock and writes it
//! atomically. f50d writing `/etc/shadow` itself would need its own crypt implementation and would race `passwd`
//! run from a shell.

mod detach;

use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};

use crate::Platform;
use crate::check::{is_one_line, refused, require};
use crate::uci::Change;

/// The longest password accepted (as the shell layer did).
pub const PASSWORD_MAX: usize = 128;

#[derive(Debug, Clone)]
pub struct System {
    platform: Platform,
}

impl System {
    pub(crate) fn new(platform: Platform) -> Self {
        System { platform }
    }

    /// The running hostname; the saved one when /proc has none, else `OpenWrt`.
    pub fn hostname(&self) -> String {
        let paths = self.platform.paths();
        if let Some(h) = f50_core::fsx::read_line(&paths.sys("/proc/sys/kernel/hostname")).filter(|h| !h.is_empty()) {
            return h;
        }
        std::fs::read_to_string(paths.sys("/etc/config/system"))
            .ok()
            .and_then(|t| {
                crate::uci::parse(&t)
                    .into_iter()
                    .find(|s| s.stype == "system")
                    .and_then(|s| s.get("hostname").map(str::to_owned))
            })
            .unwrap_or_else(|| "OpenWrt".to_owned())
    }

    /// Saved in `system.@system[0].hostname` and applied now.
    pub async fn set_hostname(&self, name: &str) -> Result<()> {
        require(is_hostname(name), "hostname")?;
        self.platform
            .uci()
            .apply(
                "system",
                &[Change::Set { section: "@system[0]".into(), option: "hostname".into(), value: name.into() }],
            )
            .await?;
        self.platform.sysfs().write("/proc/sys/kernel/hostname", name)
    }

    /// The zone name (`Europe/Istanbul`) and its POSIX TZ string, saved and applied now.
    pub async fn set_timezone(&self, zonename: &str, tz: &str) -> Result<()> {
        require(is_zonename(zonename), "zonename")?;
        require(is_tz(tz), "timezone")?;
        let s = "@system[0]".to_owned();
        self.platform
            .uci()
            .apply(
                "system",
                &[
                    Change::Set { section: s.clone(), option: "zonename".into(), value: zonename.into() },
                    Change::Set { section: s, option: "timezone".into(), value: tz.into() },
                ],
            )
            .await?;
        f50_core::fsx::write_atomic(&self.platform.paths().sys("/tmp/TZ"), format!("{tz}\n").as_bytes(), 0o644)
    }

    /// A login's password; the password reaches `passwd` on its stdin only.
    pub async fn set_password(&self, user: &str, password: &str) -> Result<()> {
        require(is_user(user), "user")?;
        if password.is_empty() || password.len() > PASSWORD_MAX || !is_one_line(password) {
            return Err(refused("password"));
        }
        if !self.user_exists(user) {
            return Err(Error::not_found("user"));
        }
        Cmd::new("passwd")
            .arg(user)
            .stdin(format!("{password}\n{password}\n"))
            .timeout(Duration::from_secs(20))
            .ok()
            .await
            .map(drop)
            .map_err(|e| e.context("passwd"))
    }

    /// The user is in `/etc/passwd`.
    pub fn user_exists(&self, user: &str) -> bool {
        std::fs::read_to_string(self.platform.paths().sys("/etc/passwd"))
            .is_ok_and(|t| t.lines().any(|l| l.split(':').next() == Some(user)))
    }

    /// Reboots after `delay` (so a reply can leave first); returns at once.
    pub fn reboot_after(&self, delay: Duration) -> Result<()> {
        detach::after(delay, "/sbin/reboot")
    }

    /// Powers off after `delay`; returns at once.
    pub fn poweroff_after(&self, delay: Duration) -> Result<()> {
        detach::after(delay, "/sbin/poweroff")
    }

    /// Writes a sysctl (`net.ipv4.ip_default_ttl`) under /proc/sys.
    pub fn sysctl(&self, key: &str, value: &str) -> Result<()> {
        let path = sysctl_path(key).ok_or_else(|| refused("sysctl"))?;
        if !is_one_line(value) {
            return Err(refused("sysctl value"));
        }
        let p = self.platform.paths().sys(&path);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&p)
            .map_err(|e| Error::io(format!("sysctl {key}"), &e))?;
        std::io::Write::write_all(&mut f, value.as_bytes()).map_err(|e| Error::io(format!("sysctl {key}"), &e))
    }

    /// Reads a sysctl; `None` when there is no such key.
    pub fn sysctl_get(&self, key: &str) -> Option<String> {
        f50_core::fsx::read_line(&self.platform.paths().sys(sysctl_path(key)?))
    }
}

/// `net.ipv4.conf.br-lan.forwarding` → `/proc/sys/net/ipv4/conf/br-lan/forwarding`; `None` for a key that is not
/// dot-separated `[A-Za-z0-9_-]` words. (Like `sysctl`, a `/` in a key is a separator too: interface names with dots
/// are written `net/ipv4/conf/eth0.2/...`.)
pub fn sysctl_path(key: &str) -> Option<String> {
    let parts: Vec<&str> = if key.contains('/') { key.split('/').collect() } else { key.split('.').collect() };
    let ok = !parts.is_empty()
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && !p.starts_with('-')
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        });
    ok.then(|| format!("/proc/sys/{}", parts.join("/")))
}

/// A hostname: dot-separated labels of `[A-Za-z0-9-]`, 1-63 bytes each, not starting or ending with `-`; at most
/// 64 bytes, what the kernel keeps.
pub fn is_hostname(s: &str) -> bool {
    s.len() <= 64
        && s.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// A zone name of the zoneinfo database (`Europe/Istanbul`, `America/Argentina/Buenos_Aires`, `UTC`, `Etc/GMT+3`).
pub fn is_zonename(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && !s.starts_with(['/', '-'])
        && !s.contains("..")
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'+' | b'-'))
}

/// A POSIX TZ string (`<+03>-3`, `CET-1CEST,M3.5.0,M10.5.0/3`), as the shell layer accepted it.
pub fn is_tz(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'<' | b'>' | b'+' | b',' | b'.' | b'/' | b':' | b'-'))
}

/// A login name: `[a-z_][a-z0-9_-]*`, at most 32 bytes.
pub fn is_user(s: &str) -> bool {
    s.len() <= 32
        && s.bytes().next().is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysctl_keys() {
        assert_eq!(sysctl_path("net.ipv4.ip_default_ttl").as_deref(), Some("/proc/sys/net/ipv4/ip_default_ttl"));
        assert_eq!(
            sysctl_path("net/ipv4/conf/eth0.2/forwarding").as_deref(),
            Some("/proc/sys/net/ipv4/conf/eth0.2/forwarding")
        );
        assert_eq!(
            sysctl_path("net.ipv6.conf.br-lan.forwarding").as_deref(),
            Some("/proc/sys/net/ipv6/conf/br-lan/forwarding")
        );
        assert!(sysctl_path("net..x").is_none());
        assert!(sysctl_path("net/../../etc/shadow").is_none());
        assert!(sysctl_path("kernel.core pattern").is_none());
        assert!(sysctl_path("").is_none());
    }

    #[test]
    fn settings_are_checked() {
        assert!(is_hostname("F50-router"));
        assert!(is_hostname("f50.lan"));
        assert!(!is_hostname("-x"));
        assert!(!is_hostname("a b"));
        assert!(!is_hostname(""));
        assert!(is_zonename("America/Argentina/Buenos_Aires"));
        assert!(is_zonename("Etc/GMT+3"));
        assert!(!is_zonename("../etc"));
        assert!(is_tz("<+03>-3"));
        assert!(is_tz("CET-1CEST,M3.5.0,M10.5.0/3"));
        assert!(!is_tz("x;reboot"));
        assert!(is_user("root"));
        assert!(is_user("f50-admin"));
        assert!(!is_user("Root"));
        assert!(!is_user("-r"));
        assert!(!is_user("a:b"));
    }

    #[tokio::test]
    async fn bad_passwords_never_reach_passwd() {
        let p = Platform::new(f50_core::paths::Paths::new("/nonexistent"), None);
        let code = |r: Result<()>| r.expect_err("refused").code();
        assert_eq!(code(p.system().set_password("root", "").await), f50_core::Code::BadValue);
        assert_eq!(code(p.system().set_password("root", "a\nb").await), f50_core::Code::BadValue);
        assert_eq!(code(p.system().set_password("root", "fine").await), f50_core::Code::NotFound);
    }

    #[test]
    fn hostname_and_sysctl_under_a_root() {
        let root = std::env::temp_dir().join(format!("f50-platform-sys-{}", std::process::id()));
        std::fs::create_dir_all(root.join("proc/sys/kernel")).expect("mkdir");
        std::fs::create_dir_all(root.join("etc/config")).expect("mkdir");
        std::fs::write(root.join("etc/config/system"), "config system\n option hostname 'saved'\n").expect("write");
        let p = Platform::new(f50_core::paths::Paths::new(&root), None);
        assert_eq!(p.system().hostname(), "saved");
        std::fs::write(root.join("proc/sys/kernel/hostname"), "F50\n").expect("write");
        assert_eq!(p.system().hostname(), "F50");
        p.system().sysctl("kernel.hostname", "other").expect("write");
        assert_eq!(p.system().sysctl_get("kernel.hostname").as_deref(), Some("other"));
        assert!(p.system().sysctl("kernel.nonexistent", "1").is_err(), "never creates a file");
        let _ = std::fs::remove_dir_all(root);
    }
}
