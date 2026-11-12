//! Small attribute files under /sys and /proc, through the root tests can move.

use std::path::{Path, PathBuf};

use f50_core::Result;

use crate::Platform;

#[derive(Debug, Clone)]
pub struct Sysfs {
    platform: Platform,
}

impl Sysfs {
    pub(crate) fn new(platform: Platform) -> Self {
        Sysfs { platform }
    }

    /// The device path under the root.
    pub fn path(&self, absolute: impl AsRef<Path>) -> PathBuf {
        self.platform.paths().sys(absolute)
    }

    /// The first line, trimmed; `None` when missing or unreadable.
    pub fn read(&self, absolute: impl AsRef<Path>) -> Option<String> {
        f50_core::fsx::read_line(&self.path(absolute))
    }

    pub fn read_num<T: std::str::FromStr>(&self, absolute: impl AsRef<Path>) -> Option<T> {
        f50_core::fsx::read_num(&self.path(absolute))
    }

    /// Writes an attribute (no newline appended unless given).
    pub fn write(&self, absolute: impl AsRef<Path>, value: &str) -> Result<()> {
        let p = self.path(absolute);
        std::fs::write(&p, value).map_err(|e| f50_core::Error::io(format!("write {}", p.display()), &e))
    }
}
