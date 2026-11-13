//! An update without the network: a release's files brought to the device as one plain tar (ustar) archive, uploaded
//! through uhttpd's cgi-io to the one path the panel's ACL may write (`.f50-upload/update.tar` on the Linux partition,
//! in a directory only root can enter: a zip of a release is 40 to 400 MB, more than RAM should hold).
//!
//! The archive holds the files flat, as a release publishes them (`SHA256SUMS`, its signature, `f50-release` or `mu300-release`, the
//! root filesystems, the kernel bundles). Nothing of it is unpacked to a path it names: a member is found by a name
//! this code made (one of the release's file names) and copied out by its offset. The Magisk zip the shell took is not
//! read here: the workspace has no zip or inflate, and the release's own tarballs are already compressed.
//!
//! The upload is renamed to a name the ACL does not let the panel write before it is looked at, and only that file is
//! inspected and used; it goes when the update ran, however it went (the LuCI adapter's rules, FINDINGS 86).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use f50_core::{Error, Result, fsx};

use crate::config::Layout;
use crate::sys;

const MEMBERS_MAX: usize = 64;

/// An offline bundle, its members by name.
#[derive(Debug, Clone)]
pub struct Bundle {
    path: PathBuf,
    members: HashMap<String, (u64, u64)>,
}

fn octal(field: &[u8]) -> Option<u64> {
    let s: String = field.iter().take_while(|b| **b != 0).map(|b| *b as char).collect();
    let s = s.trim();
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(s, 8).ok()
}

fn cstr(field: &[u8]) -> String {
    String::from_utf8_lossy(&field[..field.iter().position(|b| *b == 0).unwrap_or(field.len())]).into_owned()
}

impl Bundle {
    /// Reads the archive's index (blocking). Members whose name is not in `allowed` are passed over; a name twice,
    /// or anything that is not a tar archive, is refused.
    pub fn open(path: &Path, allowed: &[String]) -> Result<Bundle> {
        let mut f = File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), &e))?;
        let len = f.metadata().map_err(|e| Error::io("stat the upload", &e))?.len();
        let mut members = HashMap::new();
        let mut pos = 0u64;
        let mut hdr = [0u8; 512];
        let bad = || Error::failed("the file is not a release bundle (a tar archive of a release's files)");
        for _ in 0..=MEMBERS_MAX * 2 {
            f.seek(SeekFrom::Start(pos)).map_err(|e| Error::io("read the upload", &e))?;
            if f.read_exact(&mut hdr).is_err() {
                break;
            }
            if hdr.iter().all(|b| *b == 0) {
                return if members.is_empty() { Err(bad()) } else { Ok(Bundle { path: path.to_owned(), members }) };
            }
            // the checksum of the header: a stray file is not taken for an archive
            let want = octal(&hdr[148..156]).ok_or_else(bad)?;
            let sum: u64 =
                hdr.iter().enumerate().map(|(i, b)| if (148..156).contains(&i) { 32 } else { u64::from(*b) }).sum();
            if sum != want {
                return Err(bad());
            }
            let size = octal(&hdr[124..136]).ok_or_else(bad)?;
            let data = pos + 512;
            let next = data + size.div_ceil(512) * 512;
            if next > len {
                return Err(bad());
            }
            let mut name = cstr(&hdr[0..100]);
            if &hdr[257..262] == b"ustar" {
                let prefix = cstr(&hdr[345..500]);
                if !prefix.is_empty() {
                    name = format!("{prefix}/{name}");
                }
            }
            let name = name.trim_start_matches("./").to_owned();
            if matches!(hdr[156], b'0' | 0) && allowed.contains(&name) {
                if members.insert(name, (data, size)).is_some() {
                    return Err(Error::failed("the bundle has a file twice"));
                }
                if members.len() > MEMBERS_MAX {
                    return Err(bad());
                }
            }
            pos = next;
        }
        if members.is_empty() { Err(bad()) } else { Ok(Bundle { path: path.to_owned(), members }) }
    }

    pub fn has(&self, name: &str) -> bool {
        self.members.contains_key(name)
    }

    /// A small member's content (at most `max` bytes), `None` when the bundle has no such member.
    pub fn read(&self, name: &str, max: u64) -> Result<Option<Vec<u8>>> {
        let Some(&(off, size)) = self.members.get(name) else { return Ok(None) };
        if size > max {
            return Err(Error::failed(format!("{name} in the bundle is too large")));
        }
        let mut f = File::open(&self.path).map_err(|e| Error::io("open the upload", &e))?;
        f.seek(SeekFrom::Start(off)).map_err(|e| Error::io("read the upload", &e))?;
        let mut v = Vec::new();
        f.take(size).read_to_end(&mut v).map_err(|e| Error::io("read the upload", &e))?;
        Ok(Some(v))
    }

    /// The names of the members found.
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.members.keys().cloned().collect();
        v.sort();
        v
    }

    /// SHA-256 of a member (blocking).
    pub fn sha256(&self, name: &str) -> Result<Option<String>> {
        use sha2::{Digest, Sha256};
        let Some(&(off, size)) = self.members.get(name) else { return Ok(None) };
        let mut f = File::open(&self.path).map_err(|e| Error::io("open the upload", &e))?;
        f.seek(SeekFrom::Start(off)).map_err(|e| Error::io("read the upload", &e))?;
        let mut h = Sha256::new();
        let mut r = f.take(size);
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = r.read(&mut buf).map_err(|e| Error::io("read the upload", &e))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(Some(crate::util::hex(&h.finalize())))
    }

    /// A member into `dest` (a new private file; blocking). False when the bundle has no such member.
    pub fn extract(&self, name: &str, dest: &Path) -> Result<bool> {
        let Some(&(off, size)) = self.members.get(name) else { return Ok(false) };
        let copy = || -> io::Result<()> {
            let _ = fs::remove_file(dest);
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(dest)?;
            let mut f = File::open(&self.path)?;
            f.seek(SeekFrom::Start(off))?;
            let n = io::copy(&mut f.take(size), &mut out)?;
            if n != size {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            out.flush()?;
            out.sync_all()
        };
        copy().map_err(|e| Error::io(format!("copy {name} out of the upload"), &e))?;
        Ok(true)
    }
}

/// The upload directory as it must be: on the Linux partition (not a link), root's own, nobody else may enter.
pub fn upload_dir(layout: &Layout) -> Result<()> {
    let disk_ok = fs::symlink_metadata(&layout.disk).is_ok_and(|m| m.is_dir());
    if !disk_ok {
        return Err(Error::failed("the Linux partition is not mounted"));
    }
    fsx::private_dir(&layout.upload_dir).map_err(|e| e.context("the upload directory is not safe; nothing was changed"))
}

/// A regular file of ours at `path`, not a link, not empty.
pub fn upload_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file() && m.uid() == sys::euid() && m.len() > 0)
}

/// Removes the upload and what was said about it.
pub fn forget(layout: &Layout) {
    for p in [&layout.upload, &layout.staged, &layout.inspect] {
        let _ = fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, data: &[u8]) -> Vec<u8> {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(b"0000644\0");
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        let mut v = h.to_vec();
        v.extend_from_slice(data);
        v.resize(v.len().div_ceil(512) * 512, 0);
        v
    }

    #[test]
    fn members_by_name_only() {
        let dir = std::env::temp_dir().join(format!("f50-update-offline-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let mut tar = member("./SHA256SUMS", b"sums\n");
        tar.extend(member("../../etc/passwd", b"evil"));
        tar.extend(member("mu300-release", b"v2026.11.01.1\n"));
        tar.extend([0u8; 1024]);
        let p = dir.join("b.tar");
        fs::write(&p, &tar).expect("write");
        let allowed = vec!["SHA256SUMS".to_owned(), "mu300-release".to_owned()];
        let b = Bundle::open(&p, &allowed).expect("bundle");
        assert_eq!(b.read("SHA256SUMS", 64).expect("read").as_deref(), Some(&b"sums\n"[..]));
        assert!(!b.has("../../etc/passwd"));
        assert!(b.extract("mu300-release", &dir.join("rel")).expect("extract"));
        assert_eq!(fs::read(dir.join("rel")).expect("rel"), b"v2026.11.01.1\n");
        fs::write(&p, b"PK\x03\x04 not a tar").expect("zip");
        assert!(Bundle::open(&p, &allowed).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
