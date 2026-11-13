//! What a system holds, file by file (the `scan` and `sum` of `device-tools/carry/f50-carry.c`): one entry for
//! everything below a root, parents before what is in them, never leaving the root's filesystem (a directory another
//! filesystem is mounted on is listed as `m` and not entered), never following a link.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use f50_core::{Error, Result};

use crate::util::{sha256_file, sha256_hex};

/// One thing in a system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `f` `d` `l` `c` `b` `p` `s`, or `m` for a mount point.
    pub kind: char,
    /// Permission bits (07777).
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// A file's bytes, a link target's length, else 0.
    pub size: u64,
    pub nlink: u64,
    pub dev: u64,
    pub ino: u64,
    /// 512-byte blocks it takes.
    pub blocks: u64,
    /// Relative to the root, no leading `./`.
    pub path: String,
}

/// The kind letter of a file type.
pub fn kind_of(ft: &fs::FileType) -> char {
    if ft.is_symlink() {
        'l'
    } else if ft.is_dir() {
        'd'
    } else if ft.is_file() {
        'f'
    } else if ft.is_char_device() {
        'c'
    } else if ft.is_block_device() {
        'b'
    } else if ft.is_fifo() {
        'p'
    } else {
        's'
    }
}

fn entry(md: &fs::Metadata, path: String, root_dev: u64) -> Entry {
    let mut kind = kind_of(&md.file_type());
    if kind == 'd' && md.dev() != root_dev {
        kind = 'm';
    }
    Entry {
        kind,
        mode: md.mode() & 0o7777,
        uid: md.uid(),
        gid: md.gid(),
        size: if kind == 'f' || kind == 'l' { md.size() } else { 0 },
        nlink: md.nlink(),
        dev: md.dev(),
        ino: md.ino(),
        blocks: md.blocks(),
        path,
    }
}

/// Everything below `root` (blocking). Names that are not UTF-8 are passed over and counted in the second value.
pub fn scan(root: &Path, cancel: &AtomicBool) -> Result<(Vec<Entry>, usize)> {
    let root_md = fs::symlink_metadata(root).map_err(|e| Error::io(format!("scan {}", root.display()), &e))?;
    let root_dev = root_md.dev();
    let mut out = Vec::new();
    let mut skipped = 0;
    // pre-order, depth first, in name order: a directory, then everything in it, then its next sibling
    let mut stack: Vec<String> = Vec::new();
    push_children(root, "", &mut stack, &mut skipped)?;
    while let Some(rel) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::failed("stopped"));
        }
        // a running system removes things while it is looked at
        let Ok(md) = fs::symlink_metadata(root.join(&rel)) else { continue };
        let e = entry(&md, rel, root_dev);
        let descend = e.kind == 'd';
        let rel = e.path.clone();
        out.push(e);
        if descend {
            push_children(root, &rel, &mut stack, &mut skipped)?;
        }
    }
    Ok((out, skipped))
}

/// Pushes what is in `dir` (relative) onto `stack` so that the first name pops first.
fn push_children(root: &Path, dir: &str, stack: &mut Vec<String>, skipped: &mut usize) -> Result<()> {
    let full = if dir.is_empty() { root.to_owned() } else { root.join(dir) };
    let rd = match fs::read_dir(&full) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::io(format!("scan {}", full.display()), &e)),
    };
    let mut names: Vec<_> = rd.filter_map(|d| d.ok().map(|d| d.file_name())).collect();
    names.sort_by(|a, b| b.as_bytes().cmp(a.as_bytes()));
    for n in names {
        let Some(n) = n.to_str() else {
            *skipped += 1;
            continue;
        };
        stack.push(if dir.is_empty() { n.to_owned() } else { format!("{dir}/{n}") });
    }
    Ok(())
}

/// What `rel` under `root` is, for the hash: a file's content, a link's target; `-` for anything else; `None` when
/// it is not there.
pub fn hash(root: &Path, rel: &str) -> Option<String> {
    let p = root.join(rel);
    let md = fs::symlink_metadata(&p).ok()?;
    let ft = md.file_type();
    if ft.is_symlink() {
        let t = fs::read_link(&p).ok()?;
        Some(sha256_hex(t.as_os_str().as_bytes()))
    } else if ft.is_file() {
        sha256_file(&p).ok()
    } else {
        Some("-".to_owned())
    }
}

/// The hashes of one side, computed when the plan first asks.
#[derive(Debug)]
pub struct Hashes<'a> {
    root: &'a Path,
    cache: HashMap<String, Option<String>>,
}

impl<'a> Hashes<'a> {
    pub fn new(root: &'a Path) -> Self {
        Hashes { root, cache: HashMap::new() }
    }

    pub fn get(&mut self, rel: &str) -> Option<String> {
        if let Some(h) = self.cache.get(rel) {
            return h.clone();
        }
        let h = hash(self.root, rel);
        self.cache.insert(rel.to_owned(), h.clone());
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_first_links_not_followed() {
        let root = std::env::temp_dir().join(format!("f50-update-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("etc/config")).expect("dirs");
        fs::write(root.join("etc/config/network"), "x").expect("file");
        std::os::unix::fs::symlink("/etc", root.join("link")).expect("link");
        let (v, skipped) = scan(&root, &AtomicBool::new(false)).expect("scan");
        let paths: Vec<_> = v.iter().map(|e| (e.kind, e.path.as_str())).collect();
        assert_eq!(paths, vec![('d', "etc"), ('d', "etc/config"), ('f', "etc/config/network"), ('l', "link")]);
        assert_eq!(skipped, 0);
        assert_eq!(hash(&root, "etc/config/network"), Some(sha256_hex(b"x")));
        assert_eq!(hash(&root, "link"), Some(sha256_hex(b"/etc")));
        assert_eq!(hash(&root, "etc"), Some("-".into()));
        assert_eq!(hash(&root, "nothing"), None);
        let _ = fs::remove_dir_all(&root);
    }
}
