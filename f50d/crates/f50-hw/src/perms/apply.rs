//! Applies the node table with the ids of the moment.
//!
//! As the script did: every existing path a glob names (a link is followed, as `[ -e ]`, `chown` and `chmod` do -
//! the `by-name` links name the partitions), the owner first and the mode only when the owner could be set. A node
//! already as wanted is not written. Nothing here is fatal: a node that cannot be changed is counted and named.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::{Entry, Group, Owner, entries};
use crate::accounts::VendorIds;

/// What applying the table did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Applied {
    /// Paths that existed and were looked at.
    pub seen: usize,
    /// Paths whose owner or mode was changed.
    pub changed: usize,
    pub failed: usize,
    pub first_failure: Option<String>,
}

/// The paths under `root` a glob names (one `*` at most, in the last component), sorted as a shell expands them.
pub(crate) fn expand(root: &Path, glob: &str) -> Vec<PathBuf> {
    let rel = glob.trim_start_matches('/');
    let Some((dir, name)) = rel.rsplit_once('/') else { return vec![root.join(rel)] };
    let Some((pre, post)) = name.split_once('*') else { return vec![root.join(rel)] };
    let dir = root.join(dir);
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && n.len() >= pre.len() + post.len() && n.starts_with(pre) && n.ends_with(post))
        .collect();
    names.sort();
    names.into_iter().map(|n| dir.join(n)).collect()
}

fn ids(e: &Entry, v: VendorIds, gpu: u32) -> Option<(u32, u32)> {
    e.owner.map(|(o, g)| {
        let uid = match o {
            Owner::Root => 0,
            Owner::Vendor => v.uid(),
        };
        let gid = match g {
            Group::Root => 0,
            Group::Vendor => v.gid(),
            Group::Gpu => gpu,
        };
        (uid, gid)
    })
}

/// One path: `Ok(true)` changed, `Ok(false)` already right.
fn apply_one(p: &Path, owner: Option<(u32, u32)>, mode: Option<u32>) -> std::io::Result<bool> {
    let m = std::fs::metadata(p)?;
    let mut changed = false;
    if let Some((uid, gid)) = owner
        && (m.uid() != uid || m.gid() != gid)
    {
        std::os::unix::fs::chown(p, Some(uid), Some(gid))?;
        changed = true;
    }
    if let Some(mode) = mode
        && m.mode() & 0o7777 != mode
    {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))?;
        changed = true;
    }
    Ok(changed)
}

/// The whole table under `root` (`/`), for the vendor ids `v` and the GPU's group `gpu`.
pub(crate) fn apply_all(root: &Path, v: VendorIds, gpu: u32) -> Applied {
    let mut out = Applied::default();
    for e in entries() {
        let owner = ids(e, v, gpu);
        for p in expand(root, e.glob) {
            if !p.exists() {
                continue;
            }
            out.seen += 1;
            match apply_one(&p, owner, e.mode) {
                Ok(true) => out.changed += 1,
                Ok(false) => {}
                Err(err) => {
                    out.failed += 1;
                    if out.first_failure.is_none() {
                        out.first_failure = Some(format!("{}: {err}", e.glob));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_expand_like_the_shell() {
        let root = std::env::temp_dir().join(format!("f50-hw-perms-{}", std::process::id()));
        let dev = root.join("dev");
        std::fs::create_dir_all(dev.join("block/by-name")).expect("mkdir");
        for n in ["stty_nr1", "stty_nr10", "stty_nr4", "stty_lte0", ".stty_nrx", "mmcblk0rpmb"] {
            std::fs::write(dev.join(n), "").expect("node");
        }
        let names = |g: &str| {
            expand(&root, g).iter().map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect::<Vec<_>>()
        };
        assert_eq!(
            names("/dev/stty_nr*"),
            [Some("stty_nr1".into()), Some("stty_nr10".into()), Some("stty_nr4".into())]
        );
        assert_eq!(names("/dev/mmcblk*rpmb"), [Some("mmcblk0rpmb".into())]);
        assert_eq!(names("/dev/nothing*"), Vec::<Option<String>>::new());
        assert_eq!(expand(&root, "/dev/stty_nr1"), [dev.join("stty_nr1")]);
        // a mode only (ours, so no chown is needed): applied; again: nothing to change
        let p = dev.join("stty_lte0");
        assert_eq!(apply_one(&p, None, Some(0o640)).ok(), Some(true));
        assert_eq!(apply_one(&p, None, Some(0o640)).ok(), Some(false));
        let _ = std::fs::remove_dir_all(&root);
    }
}
