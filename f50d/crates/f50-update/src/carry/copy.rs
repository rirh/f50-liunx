//! Carrying out a plan (the `copy` and `link` of `device-tools/carry/mu300-carry.c`): make `DST/PATH` what
//! `SRC/PATH` is - content, owner, mode, times; names of one file stay one file; a directory is made, not filled.
//!
//! Nothing is ever written through a link of the destination: every directory on the way is checked to be a real
//! directory (made when missing), and only a non-directory at the path itself is replaced. A path gone from the
//! source (a running system removes files of its own) is passed over and counted. Not kept, unlike the C tool:
//! holes (a sparse file is written dense) and extended attributes (OpenWrt's images use neither); a device node,
//! FIFO or socket is not made (counted, and said).

use std::collections::HashMap;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, lchown};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// What a run of copies did.
#[derive(Debug, Default)]
pub struct Done {
    /// Paths gone from the source meanwhile.
    pub gone: u64,
    /// Device nodes, FIFOs and sockets that were not made.
    pub special: Vec<String>,
    /// Directories whose times are set at the end (after what went into them).
    dirs: Vec<(PathBuf, fs::Metadata)>,
}

fn time(secs: i64, nsec: i64) -> SystemTime {
    let d = Duration::new(secs.unsigned_abs(), u32::try_from(nsec).unwrap_or(0));
    if secs >= 0 { UNIX_EPOCH + d } else { UNIX_EPOCH - d }
}

fn times(md: &fs::Metadata) -> FileTimes {
    FileTimes::new().set_accessed(time(md.atime(), md.atime_nsec())).set_modified(time(md.mtime(), md.mtime_nsec()))
}

/// The directory `rel`'s last part goes into, below `root`: every part a real directory (not a link), made when
/// missing with the owner and mode of the same directory in `like` (0700 root's when it has none).
pub fn parent_dir(root: &Path, rel: &str, like: Option<&Path>) -> io::Result<PathBuf> {
    let mut cur = root.to_owned();
    let mut sofar = PathBuf::new();
    let parts: Vec<&str> = rel.split('/').collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        if part.is_empty() || *part == "." || *part == ".." {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "a path that cannot be"));
        }
        cur.push(part);
        sofar.push(part);
        match fs::symlink_metadata(&cur) {
            Ok(m) if m.file_type().is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} is not a directory", cur.display()),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&cur)?;
                fs::set_permissions(&cur, fs::Permissions::from_mode(0o700))?;
                if let Some(m) = like.and_then(|l| fs::symlink_metadata(l.join(&sofar)).ok()).filter(|m| m.is_dir()) {
                    lchown(&cur, Some(m.uid()), Some(m.gid()))?;
                    fs::set_permissions(&cur, fs::Permissions::from_mode(m.mode() & 0o7777))?;
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(cur)
}

/// Removes what is at `root/rel` (a tree, a file or a link as itself).
pub fn remove(root: &Path, rel: &str) -> io::Result<()> {
    let dir = parent_dir(root, rel, None)?;
    let p = dir.join(rel.rsplit('/').next().unwrap_or(rel));
    crate::util::remove_tree(&p)
}

/// A copier from one root to another; it keeps the names of one file one file (`links`).
#[derive(Debug)]
pub struct Copier<'a> {
    src: &'a Path,
    dst: &'a Path,
    /// Second names instead of copies (the device's own files: never written in place).
    link: bool,
    links: HashMap<(u64, u64), PathBuf>,
    pub done: Done,
}

impl<'a> Copier<'a> {
    pub fn new(src: &'a Path, dst: &'a Path, link: bool) -> Self {
        Copier { src, dst, link, links: HashMap::new(), done: Done::default() }
    }

    /// Makes `dst/rel` what `src/rel` is.
    pub fn one(&mut self, rel: &str) -> io::Result<()> {
        let from = self.src.join(rel);
        let md = match fs::symlink_metadata(&from) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.done.gone += 1;
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let dir = parent_dir(self.dst, rel, Some(self.src))?;
        let to = dir.join(rel.rsplit('/').next().unwrap_or(rel));
        let ft = md.file_type();
        let existing = fs::symlink_metadata(&to).ok();
        if ft.is_dir() {
            match existing {
                Some(m) if m.is_dir() => {}
                Some(_) => return Err(io::Error::other(format!("{rel}: a file is in the way of a directory"))),
                None => fs::create_dir(&to)?,
            }
            lchown(&to, Some(md.uid()), Some(md.gid()))?;
            fs::set_permissions(&to, fs::Permissions::from_mode(md.mode() & 0o7777))?;
            self.done.dirs.push((to, md));
            return Ok(());
        }
        if existing.as_ref().is_some_and(fs::Metadata::is_dir) {
            return Err(io::Error::other(format!("{rel}: a directory is in the way")));
        }
        if existing.is_some() {
            fs::remove_file(&to)?;
        }
        if ft.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
            return lchown(&to, Some(md.uid()), Some(md.gid()));
        }
        if !ft.is_file() {
            self.done.special.push(rel.to_owned());
            return Ok(());
        }
        let key = (md.dev(), md.ino());
        if let Some(first) = self.links.get(&key) {
            return fs::hard_link(first, &to);
        }
        if self.link && fs::hard_link(&from, &to).is_ok() {
            return Ok(());
        }
        let mut src = File::open(&from)?;
        let mut out =
            OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&to)?;
        io::copy(&mut src, &mut out)?;
        // a chown clears set-id bits: it comes first
        lchown(&to, Some(md.uid()), Some(md.gid()))?;
        out.set_permissions(fs::Permissions::from_mode(md.mode() & 0o7777))?;
        out.set_times(times(&md))?;
        out.sync_data()?;
        if md.nlink() > 1 {
            self.links.insert(key, to);
        }
        Ok(())
    }

    /// The directories get their times back (what went into them changed them). Deepest first.
    pub fn finish(mut self) -> Done {
        for (p, md) in self.done.dirs.iter().rev() {
            if let Ok(f) = File::open(p) {
                let _ = f.set_times(times(md));
            }
        }
        self.done.dirs.clear();
        self.done
    }
}

/// Owner and mode on `root/rel` (never through or on a link).
pub fn set_meta(root: &Path, rel: &str, mode: u32, uid: u32, gid: u32) -> io::Result<()> {
    let dir = parent_dir(root, rel, None)?;
    let p = dir.join(rel.rsplit('/').next().unwrap_or(rel));
    match fs::symlink_metadata(&p) {
        Ok(m) if m.file_type().is_symlink() => Ok(()),
        Ok(_) => {
            lchown(&p, Some(uid), Some(gid))?;
            fs::set_permissions(&p, fs::Permissions::from_mode(mode & 0o7777))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_keep_content_mode_links_and_never_write_through_a_link() {
        let root = std::env::temp_dir().join(format!("f50-update-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (src, dst) = (root.join("src"), root.join("dst"));
        fs::create_dir_all(src.join("etc/d")).expect("src");
        fs::create_dir_all(&dst).expect("dst");
        fs::write(src.join("etc/d/a"), "hello").expect("a");
        fs::set_permissions(src.join("etc/d/a"), fs::Permissions::from_mode(0o640)).expect("mode");
        fs::hard_link(src.join("etc/d/a"), src.join("etc/d/b")).expect("b");
        std::os::unix::fs::symlink("a", src.join("etc/d/l")).expect("l");
        let mut c = Copier::new(&src, &dst, false);
        for p in ["etc", "etc/d", "etc/d/a", "etc/d/b", "etc/d/l", "etc/gone"] {
            c.one(p).expect(p);
        }
        let done = c.finish();
        assert_eq!(done.gone, 1);
        assert_eq!(fs::read_to_string(dst.join("etc/d/a")).expect("a"), "hello");
        let (ma, mb) = (fs::metadata(dst.join("etc/d/a")).expect("a"), fs::metadata(dst.join("etc/d/b")).expect("b"));
        assert_eq!((ma.mode() & 0o777, ma.ino()), (0o640, mb.ino()));
        assert_eq!(fs::read_link(dst.join("etc/d/l")).expect("l"), PathBuf::from("a"));
        // a link in the destination where a directory should be: refused, nothing written through it
        let out = root.join("outside");
        fs::create_dir_all(&out).expect("out");
        let dst2 = root.join("dst2");
        fs::create_dir_all(&dst2).expect("dst2");
        std::os::unix::fs::symlink(&out, dst2.join("etc")).expect("planted");
        assert!(Copier::new(&src, &dst2, false).one("etc/d/a").is_err());
        assert!(fs::read_dir(&out).expect("out").next().is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
