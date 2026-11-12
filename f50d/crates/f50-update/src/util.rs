//! Small shared helpers: hashing, the trust checks on a file, which directory the running system is, and running
//! blocking file work off the async threads.

use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use f50_core::{Error, Result};
use sha2::{Digest, Sha256};

use crate::sys;

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// SHA-256 of a whole file (blocking: call it from `blocking`).
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// A 64-character lowercase hex string.
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Runs blocking file work on tokio's blocking pool.
pub async fn blocking<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| Error::failed(format!("internal: {e}")))?
}

/// The file is ours to trust: a regular file (not a link), not empty, ours, and nobody else may write it.
pub fn trusted_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_file() && m.len() > 0 && m.uid() == sys::euid() && m.mode() & 0o022 == 0)
}

/// `dir` is the directory the running system booted from (it stays in use under any name until the reboot).
pub fn is_running_root(root: &Path, dir: &Path) -> bool {
    match (fs::metadata(root), fs::metadata(dir)) {
        (Ok(r), Ok(d)) => d.is_dir() && r.dev() == d.dev() && r.ino() == d.ino(),
        _ => false,
    }
}

/// The release an installed system's image says it is (`etc/mu300/image-version`), or `None`.
pub fn release_of(system_dir: &Path) -> Option<String> {
    let v = f50_core::fsx::read_line(&system_dir.join(crate::release::assets::IMAGE_VERSION))?;
    let ok = !v.is_empty() && v.len() <= 64 && v.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b));
    ok.then_some(v)
}

/// Removes a directory tree if it is there (never follows a link at `path`: a link is removed as itself).
pub fn remove_tree(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
    }
}

/// fsync of a directory, so that a rename in it is on the disk.
pub fn sync_dir(dir: &Path) {
    if let Ok(f) = File::open(dir) {
        let _ = f.sync_all();
    }
}

/// The size of a file, 0 when it is not there.
pub fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |m| m.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_sha() {
        assert_eq!(hex(&[0, 0xab, 0x10]), "00ab10");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(is_sha256(&sha256_hex(b"")));
        assert!(!is_sha256("ABC"));
    }
}
