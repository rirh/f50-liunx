//! netifd interfaces through ubus (`network.interface.<name>`).

use f50_core::Result;
use serde_json::Value;

use crate::Platform;

#[derive(Debug, Clone)]
pub struct Netifd {
    platform: Platform,
}

impl Netifd {
    pub(crate) fn new(platform: Platform) -> Self {
        Netifd { platform }
    }

    /// `ubus call network.interface.<name> status`, as netifd returns it; `None` when there is no such interface.
    pub async fn status(&self, interface: &str) -> Result<Option<Value>> {
        let _ = (&self.platform, interface);
        todo!("f50-platform: Netifd::status")
    }

    pub async fn up(&self, interface: &str) -> Result<()> {
        let _ = interface;
        todo!("f50-platform: Netifd::up")
    }

    pub async fn down(&self, interface: &str) -> Result<()> {
        let _ = interface;
        todo!("f50-platform: Netifd::down")
    }

    /// `ubus call network reload` (after a change to /etc/config/network).
    pub async fn reload(&self) -> Result<()> {
        todo!("f50-platform: Netifd::reload")
    }
}
