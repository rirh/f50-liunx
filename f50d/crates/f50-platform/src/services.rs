//! procd services (`/etc/init.d/<name>`).

use f50_core::Result;

use crate::Platform;

#[derive(Debug, Clone)]
pub struct Services {
    platform: Platform,
}

/// What can be done to a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Reload,
    Enable,
    Disable,
}

impl Services {
    pub(crate) fn new(platform: Platform) -> Self {
        Services { platform }
    }

    /// `/etc/init.d/<name> <action>`; `unsupported` when the service is not installed.
    pub async fn run(&self, name: &str, action: Action) -> Result<()> {
        let _ = (&self.platform, name, action);
        todo!("f50-platform: Services::run")
    }

    /// Enabled at boot (an `/etc/rc.d/S??<name>` link).
    pub fn enabled(&self, name: &str) -> bool {
        let _ = name;
        todo!("f50-platform: Services::enabled")
    }

    /// procd has a running instance of it (`ubus call service list`).
    pub async fn running(&self, name: &str) -> Result<bool> {
        let _ = name;
        todo!("f50-platform: Services::running")
    }
}
