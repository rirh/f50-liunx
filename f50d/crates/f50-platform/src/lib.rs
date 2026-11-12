//! OpenWrt, as f50d uses it.
//!
//! Every capability reaches the operating system through here, so the rest of f50d never builds a `uci` command
//! line or parses `ifstatus` itself:
//!
//! - [`uci`]: read a config, apply a batch of changes and commit (values never reach a command line);
//! - [`netifd`]: interface status, up, down, reload;
//! - [`services`]: procd services (`/etc/init.d/<name>`): start, stop, restart, reload, enabled;
//! - [`system`]: hostname, timezone, a user's password, reboot, sysctl;
//! - [`sysfs`]: small attribute files under the root (`/sys`, `/proc`);
//! - [`modules`]: kernel modules;
//! - [`fetch`]: HTTP(S) downloads through the image's `uclient-fetch`.
//!
//! CONTRACT: the public items below are used by other crates. Extend them freely; do not change a signature.

pub mod fetch;
pub mod modules;
pub mod netifd;
pub mod services;
pub mod sysfs;
pub mod system;
pub mod uci;

use std::sync::Arc;

use f50_core::paths::Paths;
use f50_ubus::Client;

/// The platform handle every capability gets. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Platform {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    paths: Paths,
    ubus: Option<Client>,
}

impl Platform {
    /// `ubus` is `None` in tests and before ubusd is up; the parts that need it then answer `unsupported`.
    pub fn new(paths: Paths, ubus: Option<Client>) -> Self {
        Platform { inner: Arc::new(Inner { paths, ubus }) }
    }

    pub fn paths(&self) -> &Paths {
        &self.inner.paths
    }

    pub fn ubus(&self) -> Option<&Client> {
        self.inner.ubus.as_ref()
    }

    pub fn uci(&self) -> uci::Uci {
        uci::Uci::new(self.clone())
    }

    pub fn netifd(&self) -> netifd::Netifd {
        netifd::Netifd::new(self.clone())
    }

    pub fn services(&self) -> services::Services {
        services::Services::new(self.clone())
    }

    pub fn system(&self) -> system::System {
        system::System::new(self.clone())
    }

    pub fn sysfs(&self) -> sysfs::Sysfs {
        sysfs::Sysfs::new(self.clone())
    }

    pub fn modules(&self) -> modules::Modules {
        modules::Modules::new(self.clone())
    }

    pub fn fetch(&self) -> fetch::Fetch {
        fetch::Fetch::new(self.clone())
    }
}
