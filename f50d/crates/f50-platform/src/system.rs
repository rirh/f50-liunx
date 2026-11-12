//! System settings and power: hostname, timezone, passwords, reboot.

use std::time::Duration;

use f50_core::Result;

use crate::Platform;

#[derive(Debug, Clone)]
pub struct System {
    platform: Platform,
}

impl System {
    pub(crate) fn new(platform: Platform) -> Self {
        System { platform }
    }

    pub fn hostname(&self) -> String {
        let _ = &self.platform;
        todo!("f50-platform: System::hostname")
    }

    /// Saved in `system.@system[0].hostname` and applied now.
    pub async fn set_hostname(&self, name: &str) -> Result<()> {
        let _ = name;
        todo!("f50-platform: System::set_hostname")
    }

    /// The zone name (`Europe/Istanbul`) and its POSIX TZ string, saved and applied now.
    pub async fn set_timezone(&self, zonename: &str, tz: &str) -> Result<()> {
        let _ = (zonename, tz);
        todo!("f50-platform: System::set_timezone")
    }

    /// A login's password; the password reaches `passwd` on its stdin only.
    pub async fn set_password(&self, user: &str, password: &str) -> Result<()> {
        let _ = (user, password);
        todo!("f50-platform: System::set_password")
    }

    /// Reboots after `delay` (so a reply can leave first); returns at once.
    pub fn reboot_after(&self, delay: Duration) -> Result<()> {
        let _ = delay;
        todo!("f50-platform: System::reboot_after")
    }

    /// Powers off after `delay`; returns at once.
    pub fn poweroff_after(&self, delay: Duration) -> Result<()> {
        let _ = delay;
        todo!("f50-platform: System::poweroff_after")
    }

    /// Writes a sysctl (`net.ipv4.ip_default_ttl`) under /proc/sys.
    pub fn sysctl(&self, key: &str, value: &str) -> Result<()> {
        let _ = (key, value);
        todo!("f50-platform: System::sysctl")
    }
}
