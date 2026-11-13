//! Applying a release (the shell's `apply`), and going back (`rollback`).
//!
//! Everything is downloaded and verified first, and only then is anything changed: the root filesystem of every
//! system that is not the release yet, and the kernel bundle of the device's kernel choice (a release without one
//! leaves the boot image alone). Then, per system, the new root is unpacked beside the old one and the user's files
//! are carried into it ([`rootfs`], [`crate::carry`]), the two are swapped, and last the kernel step writes the boot
//! image ([`kernel`]).
//!
//! **If f50d dies mid-way.** Until the swap nothing but `<os>.new` and the staging directory was written: the next
//! run removes them. The swap and the boot write are *commit steps*: each runs as a task of its own on the tracker
//! (dropping the job does not stop it, and shutdown waits for it), and each is named in the journal before it
//! starts. Killed anyway (a crash, `kill -9`, power), the next start's recovery ([`recover`]) finishes or undoes the
//! swap and writes the boot image again from its parts (or the kept image back), and settles the modules. The job
//! itself is then reported `interrupted`; `status` says what recovery did.

pub mod boot;
pub mod journal;
pub mod kernel;
pub mod modules;
pub mod rootfs;
pub mod slot;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use f50_core::jobs::JobCtx;
use f50_core::{Code, Error, Result, fsx};
use serde::Serialize;

use crate::Inner;
use crate::download::{Fetcher, Provider};
use crate::offline::Bundle;
use crate::release::assets;
use crate::release::index::Checked;
use crate::tag::newer;
use crate::util::{blocking, is_running_root, release_of};
use crate::{lock, stage};

/// Lines for the job's log (and the daemon's).
#[derive(Debug, Clone, Default)]
pub struct Say(pub Option<JobCtx>);

impl Say {
    pub fn say(&self, line: &str) {
        tracing::info!("{line}");
        if let Some(c) = &self.0 {
            c.log(line);
        }
    }
}

/// What an update did, as its job's result.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub tag: String,
    pub systems: Vec<String>,
    pub boot: String,
    /// A reboot starts what was installed.
    pub reboot_needed: bool,
}

/// Where the release comes from, and the owner's words for this run.
#[derive(Debug, Clone)]
pub struct Request {
    pub tag: Option<String>,
    /// The staged upload (`apply_file`).
    pub offline: Option<PathBuf>,
    pub allow_unsigned: bool,
    pub allow_downgrade: bool,
}

/// Sets the flag when dropped: a carry running on the blocking pool stops between files when its job goes away.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Runs a commit step to its end whatever happens to the caller: its own task on the tracker.
async fn commit<T, F>(inner: &Inner, step: F) -> Result<T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    inner.tracker.spawn(step).await.map_err(|e| Error::failed(format!("internal: {e}")))?
}

/// An older release only when asked for by name (FINDINGS 86): the systems and the boot image.
fn downgrade_ok(inner: &Inner, tag: &str, systems: &[String], allowed: bool) -> Result<()> {
    if allowed {
        return Ok(());
    }
    for os in systems {
        if let Some(v) = release_of(&inner.layout.system(os))
            && newer(&v, tag)
        {
            return Err(Error::forbidden(format!(
                "{os} is {v} and {tag} is older: going back needs the downgrade option (rollback goes back to the kept version without one)"
            )));
        }
    }
    if let Some(b) = fsx::read_line(&inner.layout.boot_file("installed.tag"))
        && newer(&b, tag)
    {
        return Err(Error::forbidden(format!(
            "the boot image is {b} and {tag} is older: going back needs the downgrade option"
        )));
    }
    Ok(())
}

fn cached(inner: &Inner) -> Option<Checked> {
    serde_json::from_slice(&std::fs::read(&inner.layout.releases).ok()?).ok()
}

/// The tag an online apply takes: the one asked for, which the last check must have offered (what is installed is
/// what was shown), or the newest the last check found.
fn online_tag(inner: &Inner, asked: Option<&str>) -> Result<String> {
    let c = cached(inner).ok_or_else(|| Error::missing("check").context("check for updates first"))?;
    match asked {
        Some(t) if c.offered(t).is_some() => Ok(t.to_owned()),
        Some(_) => {
            Err(Error::new(Code::BadValue, "that release was not offered by the last check: check for updates first"))
        }
        None => Ok(c.latest.tag),
    }
}

/// The first of `names` (a file's F50 name, then its older one) the release has, downloaded and verified.
async fn first_file(f: &mut Fetcher, tag: &str, names: &[String]) -> Result<Option<std::path::PathBuf>> {
    for n in names {
        if let Some(p) = f.file(tag, n).await? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// The whole update. Holds the update lock throughout.
pub async fn apply(inner: Arc<Inner>, req: Request, say: Say) -> Result<Applied> {
    let _lock = lock::take(&inner.layout, &inner.cfg.legacy_locks)?;
    recover_now(&inner).await;
    let installed = inner.installed();
    if installed.is_empty() {
        return Err(Error::failed("the Linux partition is not mounted, or has no system"));
    }
    let stage_dir = stage::prepare(&inner.layout)?;
    let (provider, tag) = match &req.offline {
        Some(p) => {
            let (allowed, p2) = (assets::offline_names(&inner.cfg.systems), p.clone());
            let b = blocking(move || Bundle::open(&p2, &allowed)).await?;
            let b2 = b.clone();
            let t = blocking(move || {
                for n in assets::RELEASE_TAGS {
                    if let Some(t) = b2.read(n, 64)? {
                        return Ok(Some(t));
                    }
                }
                Ok(None)
            })
            .await?
            .ok_or_else(|| Error::failed("the bundle does not say which release it is"))?;
            let t = String::from_utf8_lossy(&t).trim().to_owned();
            if !crate::tag::tag_ok(&t) {
                return Err(Error::failed("the bundle does not say which release it is"));
            }
            (Provider::Offline(b), t)
        }
        None => (
            Provider::Online { fetch: inner.platform.fetch(), source: inner.source() },
            online_tag(&inner, req.tag.as_deref())?,
        ),
    };
    let offline_systems: Option<Vec<String>> = match &provider {
        Provider::Offline(b) => {
            Some(installed.iter().filter(|s| assets::rootfs(s).iter().any(|n| b.has(n))).cloned().collect())
        }
        Provider::Online { .. } => None,
    };
    let targets = offline_systems.clone().unwrap_or_else(|| installed.clone());
    downgrade_ok(&inner, &tag, &targets, req.allow_downgrade)?;
    let mut todo = Vec::new();
    for os in &targets {
        if release_of(&inner.layout.system(os)).as_deref() == Some(tag.as_str()) {
            say.say(&format!("{os}: already {tag}"));
            continue;
        }
        if is_running_root(inner.layout.paths.root(), &inner.layout.sibling(os, "old")) {
            return Err(Error::failed(format!(
                "{os} was updated already and is still running the old copy: reboot first"
            )));
        }
        todo.push(os.clone());
    }
    rootfs::clean_leftovers(&inner.layout, &inner.cfg.systems);
    // room: about 1 GB, and 1 GB for each system to update (download and unpack; the carry counts its own exactly)
    let need = 1_000_000 + 1_000_000 * todo.len() as u64;
    let free = crate::sys::free_kb(&inner.layout.disk);
    if !todo.is_empty() && free < need {
        return Err(Error::failed(format!(
            "not enough free space on the Linux partition: {} MB needed, {} MB free. Nothing was changed",
            need / 1024,
            free / 1024
        )));
    }
    say.say(&format!("downloading {tag}"));
    let mut f = Fetcher::new(provider, stage_dir.clone(), inner.trust(req.allow_unsigned), say.0.clone());
    let mut tarballs = Vec::new();
    for os in &todo {
        let names = assets::rootfs(os);
        let p = first_file(&mut f, &tag, &names)
            .await?
            .ok_or_else(|| Error::not_found(format!("{} in release {tag}", names[0])))?;
        rootfs::payload_is(&p, &tag).await?;
        tarballs.push((os.clone(), p));
    }
    let bundle = first_file(&mut f, &tag, &assets::kernel(inner.layout.kernel_choice())).await?;
    if let Some(b) = &bundle {
        let list = kernel::list(b).await?;
        if list.contains(assets::bundle::RAMDISK) {
            kernel::fits(&inner.layout, &inner.cfg, b, &list).await?;
        }
    }
    say.say("all files are here and verified; from now on the network is not needed");
    for (os, tarball) in tarballs {
        say.say(&format!("{os}: unpacking"));
        let new = rootfs::unpack(&inner.layout, &os, &tarball).await?;
        let _ = std::fs::remove_file(&tarball);
        say.say(&format!("{os}: carrying your files over"));
        let stop = StopOnDrop(Arc::new(AtomicBool::new(false)));
        let carried = crate::carry::carry(
            inner.layout.system(&os),
            new.clone(),
            os.clone(),
            inner.layout.disk.clone(),
            stop.0.clone(),
        )
        .await;
        drop(stop);
        match carried {
            Ok(report) => report.lines().take(40).for_each(|l| say.say(l)),
            Err(e) => {
                let _ = crate::util::remove_tree(&new);
                return Err(e.context(format!("{os}: carrying your files over failed, nothing was changed")));
            }
        }
        say.say(&format!("{os}: switching over"));
        let (l, o, t) = (inner.layout.clone(), os.clone(), tag.clone());
        commit(&inner, async move { blocking(move || rootfs::switch(&l, &o, &t)).await }).await?;
        say.say(&format!("{os}: updated to {tag}; the previous version is kept for a rollback"));
    }
    let boot = match bundle {
        None => format!(
            "release {tag} has no kernel bundle for kernel {}: the boot image is unchanged",
            inner.layout.kernel_choice()
        ),
        Some(b) => {
            say.say("boot: kernel and boot image");
            let (i2, t, sys) = (inner.clone(), tag.clone(), inner.installed());
            let out = commit(&inner, async move {
                let tmp = kernel::workdir(&i2.layout);
                kernel::update(&i2.layout, &i2.cfg, &sys, &t, &b, &tmp).await
            })
            .await;
            match out {
                Ok(kernel::Outcome::Written) => {
                    format!("boot: updated to {tag}; rollback with what=boot restores the previous image")
                }
                Ok(kernel::Outcome::UpToDate) => format!("boot: already up to date ({tag})"),
                Ok(kernel::Outcome::Unchanged(why)) => why,
                Err(e) if todo.is_empty() => return Err(e),
                Err(e) => format!("the boot image was not updated ({}); the systems were", e.message()),
            }
        }
    };
    say.say(&boot);
    stage::clear(&inner.layout);
    let reboot_needed = !todo.is_empty() || boot.starts_with("boot: updated");
    Ok(Applied { tag, systems: todo, boot, reboot_needed })
}

/// Goes back to the kept system (`what` = system; every installed system that has one) or boot image.
pub async fn rollback(inner: Arc<Inner>, what: &str, say: Say) -> Result<Vec<String>> {
    let _lock = lock::take(&inner.layout, &inner.cfg.legacy_locks)?;
    recover_now(&inner).await;
    let (i2, what) = (inner.clone(), what.to_owned());
    let lines = commit(&inner, async move {
        blocking(move || {
            let systems = i2.installed();
            if what == "boot" {
                return kernel::rollback(&i2.layout, &systems).map(|l| vec![l]);
            }
            let with_old: Vec<&String> = systems.iter().filter(|os| i2.layout.sibling(os, "old").is_dir()).collect();
            if with_old.is_empty() {
                return Err(Error::not_found("previous version kept"));
            }
            with_old.into_iter().map(|os| rootfs::rollback(&i2.layout, os)).collect()
        })
        .await
    })
    .await?;
    lines.iter().for_each(|l| say.say(l));
    Ok(lines)
}

/// What an earlier run left half-way (see the module's doc), under the lock the caller holds. A line for each
/// thing done, kept for `status`.
pub async fn recover_now(inner: &Arc<Inner>) {
    let i2 = inner.clone();
    let lines = blocking(move || Ok(recover(&i2))).await.unwrap_or_default();
    if !lines.is_empty() {
        for l in &lines {
            tracing::warn!("{l}");
        }
        let v = serde_json::json!({ "at": f50_core::time::unix_now(), "lines": lines });
        let _ = fsx::private_dir(&inner.layout.lock_dir)
            .and_then(|()| fsx::write_atomic(&inner.layout.recovery, v.to_string().as_bytes(), 0o600));
    }
}

/// Blocking.
pub fn recover(inner: &Inner) -> Vec<String> {
    let mut said = Vec::new();
    if let Some(journal::Step::Switch { os, .. }) = journal::read(&inner.layout)
        && inner.cfg.systems.contains(&os)
    {
        said.push(rootfs::recover_switch(&inner.layout, &os));
    }
    said.extend(kernel::recover(&inner.layout, &inner.installed(), &kernel::workdir(&inner.layout)));
    said
}
