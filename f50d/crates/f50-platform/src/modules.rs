//! Kernel modules.

use f50_core::Result;

use crate::Platform;

#[derive(Debug, Clone)]
pub struct Modules {
    platform: Platform,
}

impl Modules {
    pub(crate) fn new(platform: Platform) -> Self {
        Modules { platform }
    }

    /// Loaded now (`/sys/module/<name>` or `/proc/modules`).
    pub fn loaded(&self, name: &str) -> bool {
        let _ = (&self.platform, name);
        todo!("f50-platform: Modules::loaded")
    }

    /// Loads a module with parameters (`insmod`/`modprobe` from the image); loaded already is success.
    pub async fn load(&self, name: &str, params: &[&str]) -> Result<()> {
        let _ = (name, params);
        todo!("f50-platform: Modules::load")
    }

    pub async fn unload(&self, name: &str) -> Result<()> {
        let _ = name;
        todo!("f50-platform: Modules::unload")
    }
}
