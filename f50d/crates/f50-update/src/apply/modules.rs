//! The module sets of a mainline kernel (docs/research/FINDINGS.md 177, 235). A bundle brings its modules; they go
//! into every system (`lib/modules/<release>/`, flat, as OpenWrt loads them) as second names of the unpacked files,
//! never copies (a set is ~120 MB). The set in place belongs to the image in place: it is kept with that image
//! (`boot/prev.modules`) and goes back with it.
//!
//! From the moment the systems' set changes until the new image is written, `boot/modules.pending` names the new
//! kernel's release and banner. Whoever finds it next - the boot image's init, or the next run - puts in the set of
//! the image in the slot: the unpacked bundle's when that is the new kernel, else the kept set (a module of one build
//! in the kernel of another oopses, and a restart once left 307 empty modules: FINDINGS 235).

use std::fs;
use std::path::{Path, PathBuf};

use f50_core::{Error, Result, fsx};

use crate::config::Layout;
use crate::util::{remove_tree, sync_dir};

/// A kernel release usable as a directory name.
pub fn release_ok(r: &str) -> bool {
    !r.is_empty() && r.len() < 64 && !r.starts_with('.') && !r.contains('/')
}

fn modules_dir(l: &Layout, os: &str, krel: &str) -> PathBuf {
    l.system(os).join("lib/modules").join(krel)
}

fn kos(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|d| d.path()).filter(|p| p.extension().is_some_and(|e| e == "ko")).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The `*.ko` of `from` into `to` as second names (a copy where a link cannot be made).
fn link_set(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for k in kos(from) {
        let Some(n) = k.file_name() else { continue };
        let t = to.join(n);
        let _ = fs::remove_file(&t);
        if fs::hard_link(&k, &t).is_err() {
            fs::copy(&k, &t)?;
        }
    }
    Ok(())
}

fn clear_set(dir: &Path) {
    for k in kos(dir) {
        let _ = fs::remove_file(k);
    }
}

/// The unpacked bundle's set replaces each system's set of that release.
pub fn into_systems(l: &Layout, systems: &[String], unpacked: &Path, krel: &str) -> Result<()> {
    for os in systems {
        let d = modules_dir(l, os, krel);
        clear_set(&d);
        link_set(&unpacked.join("modules"), &d).map_err(|e| Error::io(format!("install the modules into {os}"), &e))?;
    }
    Ok(())
}

/// The set in place of `krel` is kept, into `prev.modules.new` (it becomes `prev.modules` with the kept image).
pub fn save_prev(l: &Layout, systems: &[String], krel: &str) -> Result<()> {
    let pm = l.boot_file("prev.modules.new");
    let _ = remove_tree(&pm);
    fs::create_dir_all(&pm).map_err(|e| Error::io("keep the modules in place", &e))?;
    fs::write(pm.join("kernel.release"), krel).map_err(|e| Error::io("keep the modules in place", &e))?;
    for os in systems {
        let d = modules_dir(l, os, krel);
        if !kos(&d).is_empty() {
            link_set(&d, &pm.join(os)).map_err(|e| Error::io("keep the modules in place", &e))?;
        }
    }
    Ok(())
}

pub fn commit_prev(l: &Layout) {
    let _ = remove_tree(&l.boot_file("prev.modules"));
    let _ = fs::rename(l.boot_file("prev.modules.new"), l.boot_file("prev.modules"));
}

/// A kept set (`prev.modules`, or `dir`) back into the systems; it stays kept.
pub fn restore_prev(l: &Layout, systems: &[String], dir: Option<&Path>) -> Result<()> {
    let pm = dir.map_or_else(|| l.boot_file("prev.modules"), Path::to_path_buf);
    let Some(krel) = fsx::read_line(&pm.join("kernel.release")).filter(|k| release_ok(k)) else { return Ok(()) };
    for os in systems {
        let d = modules_dir(l, os, &krel);
        if !d.is_dir() {
            continue;
        }
        clear_set(&d);
        if pm.join(os).is_dir() {
            link_set(&pm.join(os), &d)
                .map_err(|e| Error::io(format!("put the previous modules back into {os}"), &e))?;
        }
    }
    Ok(())
}

pub fn pending(l: &Layout, krel: &str, banner: &str) -> Result<()> {
    fsx::write_atomic(&l.boot_file("modules.pending"), format!("{krel}\n{banner}\n").as_bytes(), 0o644)?;
    sync_dir(&l.boot);
    Ok(())
}

pub fn done(l: &Layout) {
    let _ = fs::remove_file(l.boot_file("modules.pending"));
    sync_dir(&l.boot);
}

/// After a kernel step that stopped half-way: the set of the image in the slot (`slot_banner`), from the unpacked
/// bundle in `unpacked` when that is the new kernel, else the kept set.
pub fn settle(l: &Layout, systems: &[String], slot_banner: Option<&str>, unpacked: &Path) -> Result<Option<String>> {
    let Some(text) = fsx::read_opt(&l.boot_file("modules.pending"))? else { return Ok(None) };
    let mut lines = text.lines();
    let krel = lines.next().unwrap_or("").to_owned();
    let want = lines.next().unwrap_or("").to_owned();
    if !release_ok(&krel) {
        done(l);
        return Ok(None);
    }
    let msg = if !want.is_empty() && slot_banner == Some(want.as_str()) {
        let ok = fsx::read_line(&unpacked.join("kernel.release")).as_deref() == Some(krel.as_str())
            && !kos(&unpacked.join("modules")).is_empty();
        if !ok {
            done(l);
            return Err(Error::failed(
                "the last kernel step stopped after writing its image and its modules are gone: update again before restarting",
            ));
        }
        into_systems(l, systems, unpacked, &krel)?;
        "the last kernel step stopped after writing its image: its modules went in again"
    } else {
        let kept = [l.boot_file("prev.modules.new"), l.boot_file("prev.modules")]
            .into_iter()
            .find(|d| fsx::read_line(&d.join("kernel.release")).as_deref() == Some(krel.as_str()));
        match kept {
            Some(d) => restore_prev(l, systems, Some(&d))?,
            None => systems.iter().for_each(|os| clear_set(&modules_dir(l, os, &krel))),
        }
        "the last kernel step stopped before writing its image: the modules of the image in place went back"
    };
    done(l);
    Ok(Some(msg.to_owned()))
}

/// All `./modules/*.ko` the bundle lists were unpacked, none empty.
pub fn complete(unpacked: &Path, listed: usize) -> bool {
    let k = kos(&unpacked.join("modules"));
    k.len() == listed && k.iter().all(|p| fs::metadata(p).is_ok_and(|m| m.len() > 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_install_and_settle() {
        let root = std::env::temp_dir().join(format!("f50-update-mods-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let l = Layout::new(&f50_core::paths::Paths::new(&root));
        let sys = vec!["openwrt".to_owned()];
        let d = modules_dir(&l, "openwrt", "6.18.57");
        fs::create_dir_all(&d).expect("set");
        fs::write(d.join("old.ko"), "old").expect("old");
        let un = root.join("unpacked");
        fs::create_dir_all(un.join("modules")).expect("bundle");
        fs::write(un.join("modules/new.ko"), "new").expect("new");
        fs::write(un.join("kernel.release"), "6.18.57").expect("rel");
        fs::create_dir_all(&l.boot).expect("boot");
        save_prev(&l, &sys, "6.18.57").expect("save");
        pending(&l, "6.18.57", "Linux version 6.18.57 #2").expect("pending");
        into_systems(&l, &sys, &un, "6.18.57").expect("into");
        assert!(d.join("new.ko").exists() && !d.join("old.ko").exists());
        // the restart came before the image: the kept set goes back
        settle(&l, &sys, Some("Linux version 6.18.57 #1"), &un).expect("settle");
        assert!(d.join("old.ko").exists() && !d.join("new.ko").exists());
        assert!(!l.boot_file("modules.pending").exists());
        assert!(complete(&un, 1) && !complete(&un, 2));
        let _ = fs::remove_dir_all(&root);
    }
}
