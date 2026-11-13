//! Where f50d keeps things. Every path goes through [`Paths`], whose root tests point at a temporary directory.

use std::path::{Path, PathBuf};

/// The directories f50d uses, under one root (`/` on the device).
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Paths { root: root.into() }
    }

    /// The device's own filesystem.
    pub fn device() -> Self {
        Self::new("/")
    }

    /// An absolute device path under the root: `sys("/etc/config/wireless")`.
    pub fn sys(&self, absolute: impl AsRef<Path>) -> PathBuf {
        let p = absolute.as_ref();
        self.root.join(p.strip_prefix("/").unwrap_or(p))
    }

    /// Runtime state, in RAM, root's only (`/var/run/f50d`).
    pub fn run(&self) -> PathBuf {
        self.sys("/var/run/f50d")
    }

    /// Settings f50d keeps itself, on flash, root's only (`/etc/f50d`). User-visible settings live in UCI.
    pub fn state(&self) -> PathBuf {
        self.sys("/etc/f50d")
    }

    /// The Linux data partition the systems live on, mounted by the initramfs (`/mnt/f50-disk`).
    pub fn disk(&self) -> PathBuf {
        self.sys("/mnt/f50-disk")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}
