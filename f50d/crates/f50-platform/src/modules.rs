//! Kernel modules, loaded the way the shell layer's `extra-modules` loads them on the F50.
//!
//! A module counts as loaded when `/sys/module/<name>` exists or `/proc/modules` lists it (names compare with `-`
//! and `_` as one, as the kernel does). Loading tries OpenWrt's `modprobe` (ubox kmodloader) first and falls back to
//! `insmod` of the file under `/lib/modules/<release>` (`<name>.ko`, the dashed spelling, `extra/<name>.ko`):
//! modprobe only finds a module that is indexed, and the bundle's own modules (`mu300_thermal`, `snd-aloop`) are
//! not always. With parameters the file is inserted directly when it is there, since ubox's `modprobe` reads
//! parameters from `/etc/modules.d` rather than its command line. A module that appears loaded after a failed
//! attempt (someone else loaded it meanwhile) is success.
//!
//! Requests the kernel makes on its own go through `mu300-modprobe-guard` (FINDINGS 163); f50d's explicit loads are
//! root's deliberate ones and run `modprobe` directly.

use std::path::PathBuf;
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};

use crate::Platform;
use crate::check::{is_dashed_word, require};

/// How long one load may take (a firmware request inside a probe can take seconds).
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);

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
        if !is_dashed_word(name) {
            return false;
        }
        let n = normal(name);
        let paths = self.platform.paths();
        paths.sys(format!("/sys/module/{n}")).exists()
            || std::fs::read_to_string(paths.sys("/proc/modules")).is_ok_and(|t| listed(&t, &n))
    }

    /// Loads a module with parameters (`insmod`/`modprobe` from the image); loaded already is success.
    pub async fn load(&self, name: &str, params: &[&str]) -> Result<()> {
        require(is_dashed_word(name), "module")?;
        for p in params {
            require(is_param(p), "module parameter")?;
        }
        if self.loaded(name) {
            return Ok(());
        }
        let file = self.file(name);
        let insmod_first = !params.is_empty() && file.is_some();
        let mut last = None;
        for insmod in [insmod_first, !insmod_first] {
            let r = if insmod {
                let Some(f) = file.as_ref() else { continue };
                Cmd::new("insmod").arg(f).args(params).timeout(LOAD_TIMEOUT).ok().await
            } else {
                Cmd::new("modprobe").arg(name).args(params).timeout(LOAD_TIMEOUT).ok().await
            };
            match r {
                // success with nothing in /sys/module is a built-in module: modprobe has nothing to do
                Ok(_) => return Ok(()),
                Err(_) if self.loaded(name) => return Ok(()),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or_else(|| Error::not_found("module")).context(format!("load {name}")))
    }

    pub async fn unload(&self, name: &str) -> Result<()> {
        require(is_dashed_word(name), "module")?;
        if !self.loaded(name) {
            return Ok(());
        }
        let r = Cmd::new("rmmod").arg(normal(name)).timeout(LOAD_TIMEOUT).ok().await;
        match r {
            Ok(_) => Ok(()),
            Err(_) if !self.loaded(name) => Ok(()),
            Err(e) => Err(e.context(format!("unload {name}"))),
        }
    }

    /// The running kernel's module directory (`/lib/modules/<release>`).
    pub fn module_dir(&self) -> Option<PathBuf> {
        let paths = self.platform.paths();
        let release = f50_core::fsx::read_line(&paths.sys("/proc/sys/kernel/osrelease")).filter(|r| is_release(r))?;
        Some(paths.sys(format!("/lib/modules/{release}")))
    }

    /// The module's file, where `extra-modules` looks for it.
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        if !is_dashed_word(name) {
            return None;
        }
        let dir = self.module_dir()?;
        let under = name.replace('-', "_");
        let dashed = name.replace('_', "-");
        [
            format!("{name}.ko"),
            format!("{under}.ko"),
            format!("{dashed}.ko"),
            format!("extra/{name}.ko"),
            format!("extra/{dashed}.ko"),
        ]
        .into_iter()
        .map(|f| dir.join(f))
        .find(|p| p.is_file())
    }
}

/// The name as the kernel keeps it (`-` is `_`).
fn normal(name: &str) -> String {
    name.replace('-', "_")
}

/// `/proc/modules` has a line for `name` (already normalised).
fn listed(proc_modules: &str, name: &str) -> bool {
    proc_modules.lines().any(|l| l.split(' ').next().is_some_and(|m| normal(m) == name))
}

/// `key=value` (or a bare `key`): a word key, a value without blanks, quotes or control characters.
fn is_param(p: &str) -> bool {
    let (k, v) = p.split_once('=').unwrap_or((p, ""));
    !k.is_empty()
        && k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        && v.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'\'' | b'\\'))
}

/// A kernel release (`6.6.80-mu300`): no `/`, no `..`.
fn is_release(r: &str) -> bool {
    !r.is_empty()
        && !r.contains("..")
        && r.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_modules_and_params() {
        let t = "snd_aloop 20480 0 - Live 0x0\nsipa_eth 16384 1 - Live 0x0\n";
        assert!(listed(t, &normal("snd-aloop")));
        assert!(listed(t, "sipa_eth"));
        assert!(!listed(t, "sipa"));
        assert!(is_param("index=0"));
        assert!(is_param("ids=1,2,3"));
        assert!(is_param("debug"));
        assert!(!is_param("x=a b"));
        assert!(!is_param("=1"));
        assert!(!is_param("-f"));
        assert!(is_release("6.6.80-mu300"));
        assert!(!is_release("../x"));
    }

    #[test]
    fn files_under_the_root() {
        let root = std::env::temp_dir().join(format!("f50-platform-mod-{}", std::process::id()));
        std::fs::create_dir_all(root.join("proc/sys/kernel")).expect("mkdir");
        std::fs::create_dir_all(root.join("lib/modules/6.6.1/extra")).expect("mkdir");
        std::fs::create_dir_all(root.join("sys/module/sipa_core")).expect("mkdir");
        std::fs::write(root.join("proc/sys/kernel/osrelease"), "6.6.1\n").expect("write");
        std::fs::write(root.join("lib/modules/6.6.1/extra/snd-aloop.ko"), "").expect("write");
        let p = Platform::new(f50_core::paths::Paths::new(&root), None);
        let m = p.modules();
        assert_eq!(m.file("snd_aloop"), Some(root.join("lib/modules/6.6.1/extra/snd-aloop.ko")));
        assert!(m.file("absent").is_none());
        assert!(m.loaded("sipa-core"));
        assert!(!m.loaded("snd_aloop"));
        assert!(!m.loaded("../x"));
        let _ = std::fs::remove_dir_all(root);
    }
}
