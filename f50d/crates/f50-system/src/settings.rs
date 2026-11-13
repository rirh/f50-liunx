//! The small `KEY=VALUE` files this crate keeps under `/etc/f50d` (`cpu.conf`, `led.conf`, `zram.conf`).
//!
//! The format is the shell layer's (`/etc/f50/*.conf`): the last line of a key counts, other lines (comments, keys
//! of another version) are kept as they are, and a file is replaced whole, atomically.

use std::path::{Path, PathBuf};

use f50_core::{Result, fsx};
use f50_platform::Platform;

/// A settings file under the state directory.
#[derive(Debug, Clone)]
pub(crate) struct ConfFile {
    dir: PathBuf,
    path: PathBuf,
}

impl ConfFile {
    pub(crate) fn new(platform: &Platform, name: &str) -> Self {
        let dir = platform.paths().state();
        ConfFile { path: dir.join(name), dir }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The file's text; empty when it is missing or unreadable (every key at its default then).
    pub(crate) fn read(&self) -> String {
        match fsx::read_opt(&self.path) {
            Ok(t) => t.unwrap_or_default(),
            Err(e) => {
                tracing::warn!("{e}");
                String::new()
            }
        }
    }

    /// `key` set to `value` (`None`: taken out), the other lines as they were.
    pub(crate) fn put(&self, key: &str, value: Option<&str>) -> Result<()> {
        let text = put(&self.read(), key, value);
        fsx::private_dir(&self.dir)?;
        fsx::write_atomic(&self.path, text.as_bytes(), 0o600)
    }
}

/// The value of the last `key=` line.
pub(crate) fn get<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().rev().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(str::trim_end)
}

/// `text` without `key`'s lines, then `key=value` last.
pub(crate) fn put(text: &str, key: &str, value: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    for l in text.lines() {
        if l.strip_prefix(key).is_some_and(|r| r.starts_with('=')) {
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    if let Some(v) = value {
        out.push_str(key);
        out.push('=');
        out.push_str(v);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_line_counts_and_others_stay() {
        let t = "# mine\nLED_TIMEOUT=30\nLED_TIMEOUTX=1\nLED_TIMEOUT=40 \n";
        assert_eq!(get(t, "LED_TIMEOUT"), Some("40"));
        assert_eq!(get(t, "LED_BOOT"), None);
        assert_eq!(put(t, "LED_TIMEOUT", Some("120")), "# mine\nLED_TIMEOUTX=1\nLED_TIMEOUT=120\n");
        assert_eq!(put(t, "LED_TIMEOUT", None), "# mine\nLED_TIMEOUTX=1\n");
        assert_eq!(put("", "A", Some("1")), "A=1\n");
    }
}
