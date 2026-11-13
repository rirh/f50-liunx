//! A system's new root filesystem: unpacked next to the old one as `<os>.new` on the Linux partition, the user's
//! files carried into it, and only then the directories swapped (`<os>` -> `<os>.old`, `<os>.new` -> `<os>`).
//! Renaming the directory a running system booted from is safe - its processes keep the old one until the reboot -
//! so an interrupted download, unpack or carry never leaves a half-updated system: it leaves an `<os>.new` that the
//! next run (or the recovery at start) removes.
//!
//! The swap is two renames on one filesystem. Between them `<os>` is missing; the journal names the step, and
//! recovery completes it when `<os>.new` carries the carry's done mark, else puts `<os>.old` back.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};

use super::journal::{self, Step};
use crate::carry::DONE_MARK;
use crate::config::Layout;
use crate::release::assets;
use crate::util::{is_running_root, release_of, remove_tree, sync_dir};

const UNPACK_TIMEOUT: Duration = Duration::from_secs(40 * 60);

/// The image a root filesystem tarball holds says it is release `tag` (every gate before judged a name; what is
/// installed is the files: FINDINGS 86, "payload_is").
pub async fn payload_is(tarball: &Path, tag: &str) -> Result<()> {
    let o = Cmd::new("tar")
        .arg("-xzOf")
        .arg(tarball)
        .arg(format!("./{}", assets::IMAGE_VERSION))
        .timeout(Duration::from_secs(600))
        .max_output(4096)
        .output()
        .await?;
    let v = o.stdout.lines().next().unwrap_or("").trim().to_owned();
    if o.success() && v == tag {
        return Ok(());
    }
    let name = tarball.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Err(Error::failed(format!(
        "{name} says it is {}, not {tag}: not installed",
        if v.is_empty() { "no release at all" } else { &v }
    )))
}

/// Unpacks `tarball` as `<os>.new` (what an earlier run left there goes first).
pub async fn unpack(l: &Layout, os: &str, tarball: &Path) -> Result<PathBuf> {
    let new = l.sibling(os, "new");
    remove_tree(&new).map_err(|e| Error::io(format!("remove {}", new.display()), &e))?;
    fs::create_dir(&new).map_err(|e| Error::io(format!("make {}", new.display()), &e))?;
    let o = Cmd::new("tar").arg("-xzpf").arg(tarball).arg("-C").arg(&new).timeout(UNPACK_TIMEOUT).output().await;
    let ok = matches!(&o, Ok(o) if o.success());
    let looks_right = ["sbin/init", "bin/busybox"].iter().any(|p| fs::symlink_metadata(new.join(p)).is_ok());
    if !ok || !looks_right {
        let _ = remove_tree(&new);
        let why = match o {
            Ok(o) if !o.success() => o.message(),
            Err(e) => e.message().to_owned(),
            _ => "the unpacked system looks wrong".into(),
        };
        return Err(Error::failed(format!("{os}: unpacking failed ({why}), nothing was changed")));
    }
    Ok(new)
}

fn mv(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).map_err(|e| Error::io(format!("rename {} to {}", from.display(), to.display()), &e))
}

/// `<os>.new` (carried) in place of `<os>`, which goes aside as `<os>.old`.
pub fn switch(l: &Layout, os: &str, tag: &str) -> Result<()> {
    let (cur, old, new) = (l.system(os), l.sibling(os, "old"), l.sibling(os, "new"));
    if !new.join(DONE_MARK).exists() {
        return Err(Error::failed(format!("{os}: the new system was not carried over: not used")));
    }
    if is_running_root(l.paths.root(), &old) {
        return Err(Error::failed(format!("{os} was updated already and is still running the old copy: reboot first")));
    }
    journal::write(l, &Step::Switch { os: os.into(), tag: tag.into() })?;
    remove_tree(&old).map_err(|e| Error::io(format!("remove {}", old.display()), &e))?;
    sync_dir(&l.disk);
    mv(&cur, &old)?;
    if let Err(e) = mv(&new, &cur) {
        let _ = mv(&old, &cur);
        journal::clear(l);
        return Err(e.context("could not put the new system in place (old one restored)"));
    }
    let _ = fs::remove_file(cur.join(DONE_MARK));
    sync_dir(&l.disk);
    journal::clear(l);
    Ok(())
}

/// After a restart in the middle of [`switch`]: finish it, or undo it. A line for the log.
pub fn recover_switch(l: &Layout, os: &str) -> String {
    let (cur, old, new) = (l.system(os), l.sibling(os, "old"), l.sibling(os, "new"));
    let msg = if cur.is_dir() {
        // the swap finished (or never began): what is left of the new one goes
        let _ = fs::remove_file(cur.join(DONE_MARK));
        let _ = remove_tree(&new);
        format!("{os}: the switch had finished")
    } else if new.join(DONE_MARK).exists() && mv(&new, &cur).is_ok() {
        let _ = fs::remove_file(cur.join(DONE_MARK));
        format!("{os}: the switch to the new system was completed after a restart")
    } else if old.is_dir() && mv(&old, &cur).is_ok() {
        let _ = remove_tree(&new);
        format!("{os}: the switch was undone after a restart; the previous system is in place")
    } else {
        format!("{os}: NOT FOUND after an interrupted switch: reinstall it")
    };
    sync_dir(&l.disk);
    journal::clear(l);
    msg
}

/// Goes back to `<os>.old`. The system rolled back from stays as `<os>.broken` while it may be running.
pub fn rollback(l: &Layout, os: &str) -> Result<String> {
    let (cur, old, broken) = (l.system(os), l.sibling(os, "old"), l.sibling(os, "broken"));
    if !old.is_dir() {
        return Err(Error::not_found(format!("previous version of {os}")));
    }
    if is_running_root(l.paths.root(), &old) {
        return Err(Error::failed(format!("{os}: the previous version is the one running now: reboot first")));
    }
    if !is_running_root(l.paths.root(), &broken) {
        remove_tree(&broken).map_err(|e| Error::io("remove the copy a rollback left", &e))?;
    } else {
        return Err(Error::failed(format!("{os}: a rolled-back copy is still running: reboot first")));
    }
    journal::write(l, &Step::Switch { os: os.into(), tag: "rollback".into() })?;
    mv(&cur, &broken)?;
    if let Err(e) = mv(&old, &cur) {
        let _ = mv(&broken, &cur);
        journal::clear(l);
        return Err(e.context(format!("{os}: rollback failed (current system restored)")));
    }
    if !is_running_root(l.paths.root(), &broken) {
        let _ = remove_tree(&broken);
    }
    sync_dir(&l.disk);
    journal::clear(l);
    Ok(format!("{os}: rolled back to {}", release_of(&cur).unwrap_or_else(|| "unknown".into())))
}

/// What rollbacks left (`<os>.broken`) once it is not running, and a half-made `<os>.new`.
pub fn clean_leftovers(l: &Layout, systems: &[String]) {
    for os in systems {
        for s in ["broken", "new"] {
            let p = l.sibling(os, s);
            if p.exists() && !is_running_root(l.paths.root(), &p) && remove_tree(&p).is_ok() {
                tracing::info!(path = %p.display(), "removed what an earlier update left");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(tag: &str) -> Layout {
        let root = std::env::temp_dir().join(format!("f50-update-rootfs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let l = Layout::new(&f50_core::paths::Paths::new(&root));
        for (d, v) in [("openwrt", "v1"), ("openwrt.new", "v2")] {
            fs::create_dir_all(l.disk.join(d).join("etc/f50")).expect("dir");
            fs::write(l.disk.join(d).join(assets::IMAGE_VERSION), v).expect("version");
        }
        l
    }

    #[test]
    fn switch_and_rollback() {
        let l = layout("switch");
        assert!(switch(&l, "openwrt", "v2").is_err(), "not carried");
        fs::write(l.sibling("openwrt", "new").join(DONE_MARK), "").expect("mark");
        switch(&l, "openwrt", "v2").expect("switch");
        assert_eq!(release_of(&l.system("openwrt")).as_deref(), Some("v2"));
        assert_eq!(release_of(&l.sibling("openwrt", "old")).as_deref(), Some("v1"));
        assert!(journal::read(&l).is_none());
        rollback(&l, "openwrt").expect("rollback");
        assert_eq!(release_of(&l.system("openwrt")).as_deref(), Some("v1"));
        let _ = fs::remove_dir_all(l.paths.root());
    }

    #[test]
    fn a_switch_cut_in_half_is_completed_or_undone() {
        let l = layout("recover");
        fs::write(l.sibling("openwrt", "new").join(DONE_MARK), "").expect("mark");
        // the first rename happened, the second did not
        fs::rename(l.system("openwrt"), l.sibling("openwrt", "old")).expect("first");
        recover_switch(&l, "openwrt");
        assert_eq!(release_of(&l.system("openwrt")).as_deref(), Some("v2"));
        // without the mark the new one is not trusted: the old one comes back
        fs::rename(l.system("openwrt"), l.sibling("openwrt", "new")).expect("back");
        fs::remove_file(l.sibling("openwrt", "new").join(DONE_MARK)).ok();
        recover_switch(&l, "openwrt");
        assert_eq!(release_of(&l.system("openwrt")).as_deref(), Some("v1"));
        let _ = fs::remove_dir_all(l.paths.root());
    }
}
