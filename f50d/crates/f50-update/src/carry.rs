//! An update keeps everything the user made (docs/research/FINDINGS.md 85; the shell's carry block and
//! `device-tools/carry/f50-carry.c`, in Rust). Between the unpacking of the new system and the switch, everything
//! of the user's in the old system goes into the new one, by [`plan`]'s rules; what cannot go where it was is set
//! aside under `etc/f50/update-kept/<old release>/` (`user/`, `image/`) and named in the report. The old system is
//! never written to: it stays whole beside the new one, for a rollback.
//!
//! Left out of the shell's: Ubuntu's rules (dpkg, apt sources, systemd units, pam), the reinstall of packages added on
//! the device (their files are not carried; the report says how many, and their settings are carried as the user's),
//! the link mode of `--no-rollback`, renumbering accounts whose number differs between the images (OpenWrt's images
//! keep theirs), and the check of a package's files against apk's own sums (no sha1 here: such a file counts as the
//! user's change).

pub mod accounts;
pub mod copy;
pub mod manifest;
pub mod plan;
pub mod report;
pub mod rules;
pub mod scan;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use f50_core::{Error, Result, fsx};
use regex::Regex;

use crate::release::assets;
use crate::util::{blocking, release_of};
use plan::{Act, Plan};
use rules::PkgDb;

/// Written into the new system when all of the carry was done; the switch looks for it.
pub const DONE_MARK: &str = ".f50-carried";

struct Both<'a> {
    old: scan::Hashes<'a>,
    new: scan::Hashes<'a>,
}

impl plan::Sums for Both<'_> {
    fn old(&mut self, p: &str) -> Option<String> {
        self.old.get(p)
    }
    fn new_side(&mut self, p: &str) -> Option<String> {
        self.new.get(p)
    }
}

fn pkgdb(root: &Path) -> PkgDb {
    let installed = fs::read_to_string(root.join("lib/apk/db/installed")).unwrap_or_default();
    let mut conf = Vec::new();
    if let Ok(rd) = fs::read_dir(root.join("lib/apk/packages")) {
        for d in rd.flatten() {
            let n = d.file_name();
            let n = n.to_string_lossy();
            if n.ends_with(".conffiles") || n.ends_with(".conffiles_static") {
                conf.push(fs::read_to_string(d.path()).unwrap_or_default());
            }
        }
    }
    PkgDb::parse(&installed, &conf)
}

/// The OpenWrt release and kmods directory of the image's apk feeds (`etc/apk/repositories.d/distfeeds.list`).
fn feeds_of(new: &Path) -> Option<(String, Option<String>)> {
    let text = fs::read_to_string(new.join("etc/apk/repositories.d/distfeeds.list")).ok()?;
    let v = Regex::new(r"/releases/([0-9][0-9A-Za-z.-]*)/targets/").ok()?;
    let k = Regex::new(r"/releases/[^/]+/targets/.*/kmods/([0-9A-Za-z._-]+)/").ok()?;
    let ver = v.captures(&text)?.get(1)?.as_str().to_owned();
    Some((ver, k.captures(&text).and_then(|c| c.get(1)).map(|m| m.as_str().to_owned())))
}

/// The user's feeds (carried) follow the new release (#420): the version and kmods directory of every line of an
/// OpenWrt release feed; the host, the scheme and any line of their own stay theirs.
fn feeds_follow(new: &Path, ver: &str, kmods: Option<&str>) -> Option<()> {
    let path = new.join("etc/apk/repositories.d/distfeeds.list");
    let m = fs::symlink_metadata(&path).ok()?;
    if !m.file_type().is_file() {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    let feed =
        Regex::new(r"/releases/[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?/(targets/[^/]+/[^/]+/|packages/[^/]+/)").ok()?;
    let rel = Regex::new(r"/releases/[^/]+/").ok()?;
    let km = Regex::new(r"/kmods/[^/]+/").ok()?;
    let out: Vec<String> = text
        .lines()
        .map(|l| {
            if !feed.is_match(l) {
                return l.to_owned();
            }
            let l = rel.replace(l, format!("/releases/{ver}/").as_str()).into_owned();
            match kmods {
                Some(k) => km.replace(&l, format!("/kmods/{k}/").as_str()).into_owned(),
                None => l,
            }
        })
        .collect();
    let mut joined = out.join("\n");
    joined.push('\n');
    if joined != text {
        // a new file, never a write into it: it may be a second name of the old system's
        let _ = fs::remove_file(&path);
        fsx::write_atomic(&path, joined.as_bytes(), m.permissions().mode_bits()).ok()?;
    }
    Some(())
}

trait ModeBits {
    fn mode_bits(&self) -> u32;
}

impl ModeBits for fs::Permissions {
    fn mode_bits(&self) -> u32 {
        std::os::unix::fs::PermissionsExt::mode(self) & 0o7777
    }
}

/// What the images of v2026.10.18 left in etc/config: apk's `.apk-new` copies, and attendedsysupgrade of a package
/// the images remove on purpose.
fn stale_config_clean(new: &Path, db: &PkgDb) {
    if let Ok(rd) = fs::read_dir(new.join("etc/config")) {
        for d in rd.flatten() {
            if d.file_name().to_string_lossy().ends_with(".apk-new") {
                let _ = fs::remove_file(d.path());
            }
        }
    }
    if !db.installed.contains("attendedsysupgrade-common") && !db.installed.contains("luci-app-attendedsysupgrade") {
        let _ = fs::remove_file(new.join("etc/config/attendedsysupgrade"));
    }
}

/// The room the plan writes beside the old system (KiB): what is copied and set aside, 5 % and 32 MB more.
pub fn need_kb(p: &Plan) -> u64 {
    let kb = (p.blocks_live + p.blocks_hold + p.blocks_save) / 2;
    kb + kb / 20 + 32 * 1024
}

fn execute(old: &Path, new: &Path, from: &str, plan: &Plan, cancel: &AtomicBool) -> Result<(u64, Vec<String>)> {
    let io = |what: &str, p: &str, e: std::io::Error| Error::io(format!("{what} {p}"), &e);
    let kept_root = new.join(assets::KEPT);
    let kept = kept_root.join(from);
    if plan.acts.iter().any(|a| matches!(a, Act::Save(_) | Act::Hold(_))) {
        if fs::symlink_metadata(&kept_root).is_ok_and(|m| m.file_type().is_symlink()) {
            let _ = fs::remove_file(&kept_root);
        }
        fsx::private_dir(&kept_root)?;
        fs::create_dir_all(kept.join("image"))
            .and_then(|()| fs::create_dir_all(kept.join("user")))
            .map_err(|e| io("make", "update-kept", e))?;
    }
    let (ki, ku) = (kept.join("image"), kept.join("user"));
    let mut save = copy::Copier::new(new, &ki, false);
    for a in &plan.acts {
        if let Act::Save(p) = a {
            save.one(p).map_err(|e| io("set aside", p, e))?;
        }
    }
    save.finish();
    for a in &plan.acts {
        if let Act::Remove(p) = a {
            copy::remove(new, p).map_err(|e| io("remove", p, e))?;
        }
    }
    let mut live = copy::Copier::new(old, new, false);
    let mut vendor = copy::Copier::new(old, new, true);
    let mut hold = copy::Copier::new(old, &ku, false);
    for a in &plan.acts {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::failed("stopped"));
        }
        match a {
            Act::Live(p) => live.one(p).map_err(|e| io("carry", p, e))?,
            Act::Vendor(p) => vendor.one(p).map_err(|e| io("carry", p, e))?,
            Act::Hold(p) => hold.one(p).map_err(|e| io("set aside", p, e))?,
            _ => {}
        }
    }
    let (l, v, h) = (live.finish(), vendor.finish(), hold.finish());
    for a in &plan.acts {
        if let Act::Meta { mode, uid, gid, path } = a {
            copy::set_meta(new, path, *mode, *uid, *gid).map_err(|e| io("owner and mode of", path, e))?;
        }
    }
    if kept_root.is_dir() {
        fsx::private_dir(&kept_root)?;
    }
    let special = [l.special, v.special, h.special].concat();
    Ok((l.gone + v.gone + h.gone, special))
}

/// Carries everything of the user's from `old` into `new` (a system `os`, unpacked and not in use); `disk` is the
/// partition both are on. On success the report is written into `new` and returned, and `new` has [`DONE_MARK`].
/// `cancel` stops it between files; what it wrote is in `new` only, which the caller then removes.
pub async fn carry(old: PathBuf, new: PathBuf, os: String, disk: PathBuf, cancel: Arc<AtomicBool>) -> Result<String> {
    let from = release_of(&old).unwrap_or_else(|| "unknown".into());
    let to = release_of(&new).unwrap_or_else(|| "unknown".into());
    let manifest = if from == "unknown" { None } else { manifest::load(&old, &new, &os, &from).await };
    let when = f50_core::time::unix_now();
    blocking(move || {
        let _ = fs::remove_file(new.join(DONE_MARK));
        let feeds = feeds_of(&new);
        let (old_scan, skipped) = scan::scan(&old, &cancel)?;
        let (new_scan, _) = scan::scan(&new, &cancel)?;
        let new_map: HashMap<String, scan::Entry> = new_scan.into_iter().map(|e| (e.path.clone(), e)).collect();
        let (old_pkg, new_pkg) = (pkgdb(&old), pkgdb(&new));
        let cmds: HashSet<String> = fs::read_to_string(new.join(assets::PATH_COMMANDS))
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let (old_ids, new_ids) = (accounts::ids(&old), accounts::ids(&new));
        let inputs = plan::Inputs {
            old: &old_scan,
            new: &new_map,
            manifest: manifest.as_ref(),
            old_pkg: &old_pkg,
            new_pkg: &new_pkg,
            cmds: &cmds,
            old_ids: &old_ids,
            new_ids: &new_ids,
        };
        let mut sums = Both { old: scan::Hashes::new(&old), new: scan::Hashes::new(&new) };
        let mut plan = plan::make(&inputs, &mut sums);
        let (need, free) = (need_kb(&plan), crate::sys::free_kb(&disk));
        if free < need {
            return Err(Error::failed(format!(
                "not enough room to carry your files over beside the previous system: {} MB needed, {} MB free. Nothing was changed",
                need / 1024,
                free / 1024
            )));
        }
        let (gone, special) = execute(&old, &new, &from, &plan, &cancel)?;
        plan.notes.extend(special.into_iter().map(|p| ('n', p)));
        stale_config_clean(&new, &new_pkg);
        if let Some((ver, kmods)) = feeds {
            feeds_follow(&new, &ver, kmods.as_deref());
        }
        plan.notes.extend(accounts::merge(&old, &new)?);
        if skipped > 0 {
            plan.notes.push(('n', format!("{skipped} names that are not UTF-8 (not carried)")));
        }
        let text = report::text(&os, &from, &to, &format!("{when} (Unix time)"), &plan, gone);
        report::write(&new, &text, !plan.notes.is_empty() || plan.fallback)?;
        fsx::write_atomic(&new.join(DONE_MARK), b"", 0o600)?;
        Ok(text)
    })
    .await
}
