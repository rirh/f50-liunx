//! The imported Android files, as the vendor daemons need them whoever unpacked them and from where (issue #127).
//!
//! `modem_control` and `refnotify` run as the vendor account (or uid 1000) and keep no capability that reads past a
//! mode: with the property area 0700 they end at "can't get modem type!", with `vendor/etc` 0700 at "can't get modem
//! img!", and all the radio then shows is ENODEV on the AT ttys. An install from a FAT/exFAT folder on macOS stores
//! every file 0700. So: root's and nobody else's to write - directories 0755 (0711, as Android has the property area,
//! is left), whatever lies in a `bin` directory 0755, the property files 0444, every other file 0644. Links are not
//! touched and nothing mounted below is entered (`find -xdev`). One walk when all is right, and no write.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The trees the daemons read.
const TREES: &[&str] = &["dev-properties", "vendor", "system", "apex", "linkerconfig"];

/// What the walk found and did.
#[derive(Debug, Default)]
pub(crate) struct Modes {
    pub wrong: usize,
    pub first_wrong: Option<PathBuf>,
    /// Still wrong after the repair.
    pub left: usize,
    pub first_left: Option<PathBuf>,
}

/// The mode a path should have, `None` for "any of 0755/0711" (directories) or "only the owner" (other types).
fn wanted(root: &Path, p: &Path, m: &std::fs::Metadata) -> Option<u32> {
    let rel = p.strip_prefix(root).unwrap_or(p);
    let in_bin = format!("/{}", rel.display()).contains("/bin/");
    let props = rel.starts_with("dev-properties") && rel != Path::new("dev-properties");
    if m.is_file() {
        Some(if in_bin {
            0o755
        } else if props {
            0o444
        } else {
            0o644
        })
    } else {
        None
    }
}

fn is_wrong(root: &Path, p: &Path, m: &std::fs::Metadata) -> bool {
    let mode = m.mode() & 0o7777;
    if m.uid() != 0 || m.gid() != 0 {
        return true;
    }
    if m.is_dir() {
        return mode != 0o755 && mode != 0o711;
    }
    wanted(root, p, m).is_some_and(|w| w != mode)
}

fn fix(root: &Path, p: &Path, m: &std::fs::Metadata) {
    if m.uid() != 0 || m.gid() != 0 {
        let _ = std::os::unix::fs::lchown(p, Some(0), Some(0));
    }
    let mode = m.mode() & 0o7777;
    let want = if m.is_dir() {
        (mode != 0o755 && mode != 0o711).then_some(0o755)
    } else {
        wanted(root, p, m).filter(|w| *w != mode)
    };
    if let Some(w) = want {
        let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(w));
    }
}

/// Every path of the trees (no links followed, no other filesystem entered), with its metadata.
fn walk(root: &Path, mut f: impl FnMut(&Path, &std::fs::Metadata)) {
    for t in TREES {
        let top = root.join(t);
        let Ok(m) = std::fs::symlink_metadata(&top) else { continue };
        if !m.is_dir() {
            continue;
        }
        let dev = m.dev();
        let mut stack = vec![(top, m)];
        while let Some((p, m)) = stack.pop() {
            f(&p, &m);
            if !m.is_dir() {
                continue;
            }
            for e in std::fs::read_dir(&p).into_iter().flatten().flatten() {
                let Ok(cm) = std::fs::symlink_metadata(e.path()) else { continue };
                if cm.file_type().is_symlink() || cm.dev() != dev {
                    continue;
                }
                stack.push((e.path(), cm));
            }
        }
    }
}

/// Finds what is wrong and repairs it (blocking: run it off the runtime).
pub(crate) fn repair(root: &Path) -> Modes {
    let mut out = Modes::default();
    let mut wrong = Vec::new();
    walk(root, |p, m| {
        if is_wrong(root, p, m) {
            wrong.push(p.to_path_buf());
        }
    });
    out.wrong = wrong.len();
    out.first_wrong = wrong.first().cloned();
    if wrong.is_empty() {
        return out;
    }
    for p in &wrong {
        if let Ok(m) = std::fs::symlink_metadata(p) {
            fix(root, p, &m);
        }
    }
    walk(root, |p, m| {
        if is_wrong(root, p, m) {
            out.left += 1;
            if out.first_left.is_none() {
                out.first_left = Some(p.to_path_buf());
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_by_place() {
        let root = std::env::temp_dir().join(format!("f50-hw-modes-{}", std::process::id()));
        std::fs::create_dir_all(root.join("vendor/bin")).expect("mkdir");
        std::fs::create_dir_all(root.join("dev-properties")).expect("mkdir");
        std::fs::write(root.join("vendor/bin/modem_control"), "").expect("w");
        std::fs::write(root.join("dev-properties/property_info"), "").expect("w");
        std::fs::write(root.join("vendor/build.prop"), "").expect("w");
        let m = |p: &str| std::fs::metadata(root.join(p)).expect("meta");
        assert_eq!(wanted(&root, &root.join("vendor/bin/modem_control"), &m("vendor/bin/modem_control")), Some(0o755));
        assert_eq!(
            wanted(&root, &root.join("dev-properties/property_info"), &m("dev-properties/property_info")),
            Some(0o444)
        );
        assert_eq!(wanted(&root, &root.join("vendor/build.prop"), &m("vendor/build.prop")), Some(0o644));
        assert_eq!(wanted(&root, &root.join("vendor"), &m("vendor")), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
