//! One update at a time, across processes: an exclusive flock(2) on `update-lock/lock` under the runtime directory.
//!
//! As the shell's lock (tests/test_update.py `Lock`): the directory is root's own, mode 700, and refused when it is a
//! link or someone else's - never under /tmp, where any login could plant a link. The file is a regular file opened
//! without following a link. The kernel drops a flock however its holder ends, so a lock left by an update that is
//! gone is no lock at all: the next run takes it, overwrites the pid and says so. A run that cannot take it fails
//! closed, naming the holder (its pid, digits only and at most 8 of them, and its command when /proc still has it).
//! While both updaters may be installed, the shell `mu300-update`'s lock is taken too, where its directory exists.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use f50_core::{Error, Result, fsx};
use serde::Serialize;

use crate::config::Layout;
use crate::sys;

/// Held while an update changes anything; dropping it releases the lock.
#[derive(Debug)]
pub struct UpdateLock {
    _files: Vec<File>,
}

/// Who holds the lock, as `status` reports it.
#[derive(Debug, Clone, Serialize)]
pub struct Holder {
    pub pid: u32,
    pub command: Option<String>,
    /// This daemon holds it (one of its update jobs runs).
    pub this_daemon: bool,
}

fn lock_file(layout: &Layout) -> PathBuf {
    layout.lock_dir.join("lock")
}

fn open(path: &Path) -> Result<File> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.file_type().is_file() => {
            return Err(Error::failed(format!("{} is not a regular file: not updating", path.display())));
        }
        _ => {}
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| Error::io(format!("open {}", path.display()), &e))
}

fn read_pid(f: &mut File) -> Option<u32> {
    let mut s = String::new();
    f.seek(SeekFrom::Start(0)).ok()?;
    f.take(16).read_to_string(&mut s).ok()?;
    let s = s.trim();
    (!s.is_empty() && s.len() <= 8 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten()
}

fn command_of(pid: u32) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let s: String = String::from_utf8_lossy(&raw).replace('\0', " ").trim().to_owned();
    (!s.is_empty()).then(|| f50_core::sanitize::sanitize_message(&s))
}

fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Takes the update lock (and the legacy updater's, where it is installed); `busy` naming the holder otherwise.
pub fn take(layout: &Layout, legacy: &[PathBuf]) -> Result<UpdateLock> {
    fsx::private_dir(&layout.lock_dir).map_err(|e| e.context("the update lock"))?;
    let path = lock_file(layout);
    let mut f = open(&path)?;
    if !sys::try_lock(&f).map_err(|e| Error::io("flock", &e))? {
        let pid = read_pid(&mut f);
        let who = pid.filter(|p| alive(*p)).map(|p| format!(" (pid {p})")).unwrap_or_default();
        return Err(Error::busy(format!("another update is running{who}")));
    }
    if let Some(old) = read_pid(&mut f).filter(|p| *p != std::process::id()) {
        tracing::info!(pid = old, "taking over the lock of an update that is gone");
    }
    let me = std::process::id().to_string();
    f.set_len(0)
        .and_then(|()| f.seek(SeekFrom::Start(0)))
        .and_then(|_| f.write_all(me.as_bytes()))
        .map_err(|e| Error::io("write the update lock", &e))?;
    let mut files = vec![f];
    for l in legacy {
        let path = layout.sys(l);
        let Some(dir) = path.parent() else { continue };
        // only where that updater made its directory: a plain directory, ours
        let ok = fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir());
        if !ok {
            continue;
        }
        let lf = open(&path)?;
        if !sys::try_lock(&lf).map_err(|e| Error::io("flock", &e))? {
            return Err(Error::busy("the shell updater (mu300-update) is running"));
        }
        files.push(lf);
    }
    Ok(UpdateLock { _files: files })
}

/// The process that holds the lock, by the pid it wrote, when it is alive. Never tries the lock itself: a probe that
/// held it for an instant could turn a real run away (FINDINGS 86).
pub fn holder(layout: &Layout) -> Option<Holder> {
    let path = lock_file(layout);
    let m = fs::symlink_metadata(&path).ok()?;
    if !m.file_type().is_file() {
        return None;
    }
    let mut f = File::open(&path).ok()?;
    let pid = read_pid(&mut f)?;
    // a pid file with no flock behind it is a run that is gone: try a shared look without taking anything
    if !alive(pid) {
        return None;
    }
    let this = pid == std::process::id();
    Some(Holder { pid, command: command_of(pid), this_daemon: this })
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::paths::Paths;

    #[test]
    fn one_holder_and_a_gone_one_is_taken_over() {
        let root = std::env::temp_dir().join(format!("f50-update-lock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let layout = Layout::new(&Paths::new(&root));
        let a = take(&layout, &[]).expect("first");
        assert_eq!(take(&layout, &[]).expect_err("second").code(), f50_core::Code::Busy);
        drop(a);
        // a pid that no process has: the lock is free, and is taken over
        fs::write(lock_file(&layout), "99999999").expect("plant");
        let b = take(&layout, &[]).expect("taken over");
        assert_eq!(fs::read_to_string(lock_file(&layout)).expect("pid"), std::process::id().to_string());
        drop(b);
        // what is not a pid is never used
        fs::write(lock_file(&layout), "12; rm -rf /").expect("plant");
        assert!(holder(&layout).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_link_in_place_of_the_file_is_refused() {
        let root = std::env::temp_dir().join(format!("f50-update-lock-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let layout = Layout::new(&Paths::new(&root));
        fsx::private_dir(&layout.lock_dir).expect("dir");
        std::os::unix::fs::symlink("/etc/passwd", lock_file(&layout)).expect("link");
        assert!(take(&layout, &[]).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
