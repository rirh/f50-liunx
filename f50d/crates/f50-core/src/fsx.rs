//! Files written the safe way: atomically, privately, never through a link someone planted.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::error::{Error, Result};

/// A directory that exists, is a real directory (not a link), belongs to us and only we can enter.
pub fn private_dir(dir: &Path) -> Result<()> {
    match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| Error::io(dir.display(), &e))?;
        }
        Err(e) => return Err(Error::io(dir.display(), &e)),
        Ok(m) if !m.is_dir() => return Err(Error::failed(format!("{} is not a directory", dir.display()))),
        Ok(m) if m.uid() != current_uid() => {
            return Err(Error::failed(format!("{} belongs to someone else", dir.display())));
        }
        Ok(_) => {}
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| Error::io(dir.display(), &e))
}

fn current_uid() -> u32 {
    #[allow(unsafe_code)]
    // SAFETY: geteuid(2) has no preconditions and cannot fail.
    unsafe {
        libc::geteuid()
    }
}

/// `data` into `path` atomically (a temporary beside it, fsynced, renamed over it) with `mode`. The temporary is
/// created exclusively and never followed through a link.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::failed(format!("{}: no parent directory", path.display())))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    let write = || -> io::Result<()> {
        let mut f =
            OpenOptions::new().write(true).create_new(true).mode(mode).custom_flags(libc::O_NOFOLLOW).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io(format!("write {}", path.display()), &e)
    })
}

/// The file's contents, or `None` when it does not exist.
pub fn read_opt(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(format!("read {}", path.display()), &e)),
    }
}

/// The first line of a small file (a sysfs attribute), trimmed; `None` when missing or unreadable.
pub fn read_line(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?;
    Some(s.lines().next().unwrap_or("").trim().to_owned())
}

/// A number in a small file (a sysfs attribute).
pub fn read_num<T: std::str::FromStr>(path: &Path) -> Option<T> {
    read_line(path)?.parse().ok()
}
