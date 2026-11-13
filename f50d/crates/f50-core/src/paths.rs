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
    ///
    /// The project's own directories are named for the F50 (`/etc/f50`, `/run/f50`, `/run/f50-*`, `/opt/f50`,
    /// `/lib/f50`, `/mnt/f50-disk`, `.f50`). A device installed before the rename (and an initramfs or image built
    /// then) has them under their older `mu300` names: where the F50 name is not there and the older one is, the
    /// older one is the path (see [`legacy_name`]). So reads and writes go where the rest of the system looks.
    pub fn sys(&self, absolute: impl AsRef<Path>) -> PathBuf {
        let p = absolute.as_ref();
        let mut real = self.root.clone();
        let mut virt = String::new();
        for c in p.strip_prefix("/").unwrap_or(p).components() {
            let Some(name) = c.as_os_str().to_str() else {
                real.push(c);
                continue;
            };
            real = match legacy_name(if virt.is_empty() { "/" } else { &virt }, name) {
                Some(old) => compat_join(&real, name, &old),
                None => real.join(name),
            };
            virt.push('/');
            virt.push_str(name);
        }
        real
    }

    /// Runtime state, in RAM, root's only (`/var/run/f50d`).
    pub fn run(&self) -> PathBuf {
        self.sys("/var/run/f50d")
    }

    /// Settings f50d keeps itself, on flash, root's only (`/etc/f50d`). User-visible settings live in UCI.
    pub fn state(&self) -> PathBuf {
        self.sys("/etc/f50d")
    }

    /// The Linux data partition the systems live on, mounted by the initramfs (`/mnt/f50-disk`, or
    /// `/mnt/mu300-disk` where the initramfs and the installer still name it so).
    pub fn disk(&self) -> PathBuf {
        self.sys("/mnt/f50-disk")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// The older (`mu300`) name of the component `name` of the directory `parent` (a device path), if it had one.
pub fn legacy_name(parent: &str, name: &str) -> Option<String> {
    match (parent, name) {
        (_, ".f50") => Some(".mu300".into()),
        ("/etc" | "/run" | "/opt" | "/lib", "f50") => Some("mu300".into()),
        ("/mnt", "f50-disk") => Some("mu300-disk".into()),
        ("/run", n) => n.strip_prefix("f50-").map(|rest| format!("mu300-{rest}")),
        _ => None,
    }
}

/// `dir/name`, unless only `dir/old` (the name it had before) is there: then that.
pub fn compat_join(dir: &Path, name: &str, old: &str) -> PathBuf {
    let new = dir.join(name);
    if new.symlink_metadata().is_err() {
        let legacy = dir.join(old);
        if legacy.symlink_metadata().is_ok() {
            return legacy;
        }
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f50_names_fall_back_to_mu300() {
        let dir = std::env::temp_dir().join(format!("f50-paths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = Paths::new(&dir);
        // nothing there: the F50 names
        assert_eq!(p.sys("/etc/f50/adb"), dir.join("etc/f50/adb"));
        assert_eq!(p.disk(), dir.join("mnt/f50-disk"));
        // a device of before the rename
        std::fs::create_dir_all(dir.join("etc/mu300")).expect("etc");
        std::fs::create_dir_all(dir.join("mnt/mu300-disk/.mu300")).expect("disk");
        std::fs::create_dir_all(dir.join("run")).expect("run");
        std::fs::write(dir.join("run/mu300-root-dev"), "x").expect("dev");
        assert_eq!(p.sys("/etc/f50/adb"), dir.join("etc/mu300/adb"));
        assert_eq!(p.sys("/mnt/f50-disk/.f50/boot-tries"), dir.join("mnt/mu300-disk/.mu300/boot-tries"));
        assert_eq!(p.sys("/run/f50-root-dev"), dir.join("run/mu300-root-dev"));
        assert_eq!(p.sys("/etc/f50d/x"), dir.join("etc/f50d/x"));
        assert_eq!(p.sys("/etc/config/f50"), dir.join("etc/config/f50"));
        // both: the F50 name wins
        std::fs::create_dir_all(dir.join("etc/f50")).expect("etc");
        assert_eq!(p.sys("/etc/f50/adb"), dir.join("etc/f50/adb"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
