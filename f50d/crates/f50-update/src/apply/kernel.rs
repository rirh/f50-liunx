//! The kernel step: the release's kernel bundle for this device's kernel choice into the boot image of the Linux
//! slot, its modules into the systems, and the image in place kept for `rollback` (the shell's `boot_update`,
//! `rollback_boot`). The bundle is refused, before anything changes, when it does not run on this device (`./devices`),
//! cannot read the SD card the system runs from (`sdcard`), runs only from slot b while Linux is on a (`linux-slot`),
//! or cannot find a root in a GPT partition (`gpt-root`); a root in an image file is never found by our init (#290).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result, fsx};

use super::boot::{self, SlotImage};
use super::journal::{self, Step};
use super::modules;
use super::slot::{self, RootLayout};
use crate::config::{KERNELS_RETIRED, Layout, UpdateConfig};
use crate::release::assets::bundle as member;
use crate::util::{blocking, file_len, remove_tree, sha256_hex};

/// How the step ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The release cannot update the boot image on the device (a line why).
    Unchanged(String),
    UpToDate,
    Written,
}

fn tar(bundle: &Path) -> Cmd {
    Cmd::new("tar").arg("-xzf").arg(bundle).timeout(Duration::from_secs(20 * 60))
}

async fn member_text(bundle: &Path, name: &str) -> Option<String> {
    let o = Cmd::new("tar")
        .arg("-xzOf")
        .arg(bundle)
        .arg(name)
        .max_output(64 * 1024)
        .timeout(Duration::from_secs(300))
        .output()
        .await
        .ok()?;
    o.success().then_some(o.stdout)
}

/// Whether a bundle (`list`: its members) may go into this device's boot image; the refusal says why.
pub async fn fits(l: &Layout, cfg: &UpdateConfig, bundle: &Path, list: &HashSet<String>) -> Result<()> {
    if list.contains(member::DEVICES) {
        let d = member_text(bundle, member::DEVICES).await.unwrap_or_default();
        if !d.split_whitespace().any(|w| w == cfg.device) {
            return Err(Error::unsupported(format!("this kernel on the {}", cfg.device)));
        }
    }
    let features = if list.contains(member::FEATURES) {
        member_text(bundle, member::FEATURES).await.unwrap_or_default()
    } else {
        String::new()
    };
    let has = |f: &str| features.lines().any(|x| x.trim() == f);
    let layout = slot::root_layout(l);
    if layout == RootLayout::Sd && !has("sdcard") {
        return Err(Error::failed(
            "this kernel bundle cannot read the SD card, and this system runs from it; nothing was changed",
        ));
    }
    if slot::linux_slot(l) == 'a' && !has("linux-slot") {
        return Err(Error::failed(
            "this kernel bundle's init runs only from slot b, and Linux is on slot a; nothing was changed",
        ));
    }
    match layout {
        RootLayout::Image => Err(Error::failed(
            "this system runs from an image file inside Android's userdata, which the init of this kernel cannot find (issue #290); nothing was changed",
        )),
        RootLayout::Part if !has("gpt-root") => Err(Error::failed(
            "this system runs from the GPT partition mu300root, which the init of this kernel bundle cannot find (issue #290); nothing was changed",
        )),
        _ => Ok(()),
    }
}

pub async fn list(bundle: &Path) -> Result<HashSet<String>> {
    let o =
        Cmd::new("tar").arg("-tzf").arg(bundle).max_output(8 << 20).timeout(Duration::from_secs(600)).output().await?;
    if !o.success() {
        return Err(Error::failed("the kernel bundle is not a readable tar.gz"));
    }
    Ok(o.stdout.lines().map(|s| s.trim_end_matches('/').to_owned()).collect())
}

/// KiB the boot image kept for rollback takes (given up first when the room is short).
fn kept_kb(l: &Layout) -> u64 {
    file_len(&l.boot_file("prev.body")) / 1024
}

fn drop_kept(l: &Layout) {
    for f in ["prev.body", "prev.vbmeta", "prev.footer", "prev.slot", "prev.tag", "prev.kernel"] {
        let _ = fs::remove_file(l.boot_file(f));
    }
    let _ = remove_tree(&l.boot_file("prev.modules"));
}

fn put(l: &Layout, name: &str, v: &str) -> Result<()> {
    fsx::write_atomic(&l.boot_file(name), format!("{v}\n").as_bytes(), 0o644)
}

/// The kernel step for release `tag` with the verified `bundle`. `tmp` is its working directory (staging).
pub async fn update(
    l: &Layout,
    cfg: &UpdateConfig,
    systems: &[String],
    tag: &str,
    bundle: &Path,
    tmp: &Path,
) -> Result<Outcome> {
    let (dev, psize) = slot::partition(l, &format!("boot_{}", slot::linux_slot(l)))?;
    let list = list(bundle).await?;
    if !list.contains(member::RAMDISK) {
        return Ok(Outcome::Unchanged(format!(
            "release {tag} predates on-device boot updates; the boot image is unchanged"
        )));
    }
    fits(l, cfg, bundle, &list).await?;
    // room: the unpacked bundle (about three times its download), and two images
    let need = 3 * (file_len(bundle) / 1024) + 64 * 1024 + 16 * 1024;
    let free = crate::sys::free_kb(&l.disk);
    if free < need {
        if free + kept_kb(l) < need {
            return Err(Error::failed(format!(
                "not enough free space for the kernel: about {} MB needed, {} MB free. Nothing was changed",
                need / 1024,
                free / 1024
            )));
        }
        tracing::info!("the room is short: the boot image kept from before the one in place is given up");
        drop_kept(l);
    }
    let _ = remove_tree(tmp);
    fs::create_dir_all(tmp).map_err(|e| Error::io("make the kernel's working directory", &e))?;
    let mods = list.contains(member::RELEASE);
    let mut c = tar(bundle).arg("-C").arg(tmp).arg(member::IMAGE).arg(member::RAMDISK);
    if mods {
        c = c.arg(member::RELEASE).arg(member::MODULES);
    }
    let o = c.output().await?;
    let listed = list.iter().filter(|n| n.starts_with("./modules/") && n.ends_with(".ko")).count();
    let krel = fsx::read_line(&tmp.join("kernel.release")).filter(|k| modules::release_ok(k));
    if !o.success()
        || file_len(&tmp.join("Image")) == 0
        || file_len(&tmp.join("ramdisk-generic.lz4")) == 0
        || (mods && (krel.is_none() || !modules::complete(tmp, listed)))
    {
        return Err(Error::failed("unpacking the kernel bundle failed; nothing was changed"));
    }
    let (l2, cfg2, sys2, tag2, tmp2, choice) =
        (l.clone(), cfg.clone(), systems.to_vec(), tag.to_owned(), tmp.to_owned(), l.kernel_choice().to_owned());
    blocking(move || write_step(&l2, &cfg2, &sys2, &tag2, &tmp2, &dev, psize, krel.as_deref(), &choice)).await
}

#[allow(clippy::too_many_arguments)]
fn write_step(
    l: &Layout,
    cfg: &UpdateConfig,
    systems: &[String],
    tag: &str,
    tmp: &Path,
    dev: &Path,
    psize: u64,
    krel: Option<&str>,
    choice: &str,
) -> Result<Outcome> {
    let rd = |p: &Path| fs::read(p).map_err(|e| Error::io(p.display(), &e));
    let image = rd(&tmp.join("Image"))?;
    let generic = rd(&tmp.join("ramdisk-generic.lz4"))?;
    let banner = boot::banner(&image).unwrap_or_default();
    let mut idh = image.clone();
    idh.extend_from_slice(&generic);
    let id = sha256_hex(&idh);
    drop(idh);
    let s: SlotImage = boot::read_slot(dev, psize)?;
    let body = boot::read_at(dev, 0, s.osize)?;
    let vbmeta = boot::read_at(dev, s.osize, s.vbs)?;
    let cur = sha256_hex(&body);
    let written = fsx::read_line(&l.boot_file("written.sha256")).unwrap_or_default();
    fs::create_dir_all(&l.boot).map_err(|e| Error::io("make the boot directory", &e))?;
    if cur == written && fsx::read_line(&l.boot_file("installed.id")).as_deref() == Some(id.as_str()) {
        if let Some(k) = krel {
            modules::pending(l, k, &banner)?;
            modules::into_systems(l, systems, tmp, k)?;
            modules::done(l);
        }
        put(l, "installed.tag", tag)?;
        settle_choice(l, krel);
        return Ok(Outcome::UpToDate);
    }
    let dr = l.boot_file("device-ramdisk.lz4");
    if cur != written || file_len(&dr) == 0 {
        // the image comes from an installer: its whole ramdisk becomes the device segment
        fsx::write_atomic(&dr, s.ramdisk(&body)?, 0o644)?;
    }
    let device = rd(&dr)?;
    let (new_body, footer) = boot::assemble(&s, &image, &[&device, &generic])?;
    drop(image);
    // the parts of the new image are files: what a restart writes again
    for (n, d) in [("new", &new_body[..]), ("vbmeta", &vbmeta[..]), ("newfooter", &footer[..])] {
        fsx::write_atomic(&tmp.join(n), d, 0o600)?;
    }
    // the image in place and its modules are kept, then the new modules, then the write
    if let Some(k) = krel {
        modules::save_prev(l, systems, k)?;
        modules::pending(l, k, &banner)?;
        if let Err(e) = modules::into_systems(l, systems, tmp, k) {
            let _ = modules::restore_prev(l, systems, Some(&l.boot_file("prev.modules.new")));
            modules::done(l);
            return Err(e.context("the boot image is unchanged"));
        }
    }
    modules::commit_prev(l);
    fsx::write_atomic(&l.boot_file("prev.body"), &body, 0o600)?;
    fsx::write_atomic(&l.boot_file("prev.vbmeta"), &vbmeta, 0o600)?;
    fsx::write_atomic(&l.boot_file("prev.footer"), &s.footer, 0o600)?;
    put(l, "prev.slot", &slot::linux_slot(l).to_string())?;
    put(l, "prev.tag", &fsx::read_line(&l.boot_file("installed.tag")).unwrap_or_else(|| "installer".into()))?;
    put(l, "prev.kernel", &fsx::read_line(&l.boot_file("installed.kernel")).unwrap_or_else(|| "5.4".into()))?;
    arm_trial(l, cfg);
    let step = Step::BootWrite {
        tag: tag.into(),
        dev: dev.to_owned(),
        psize,
        image_sha: boot::image_sha(&new_body, &vbmeta),
        footer_sha: sha256_hex(&footer),
        body_sha: sha256_hex(&new_body),
        id,
        kernel: choice.into(),
    };
    journal::write(l, &step)?;
    finish_write(l, systems, &step, &new_body, &vbmeta, &footer, krel)?;
    Ok(Outcome::Written)
}

/// Writes the image the journal names and does the bookkeeping; on a failed write the kept image goes back.
#[allow(clippy::too_many_arguments)]
fn finish_write(
    l: &Layout,
    systems: &[String],
    step: &Step,
    body: &[u8],
    vbmeta: &[u8],
    footer: &[u8; 64],
    krel: Option<&str>,
) -> Result<()> {
    let Step::BootWrite { tag, dev, psize, body_sha, id, kernel, .. } = step else {
        return Err(Error::failed("journal"));
    };
    let drop_caches = l.sys("/proc/sys/vm/drop_caches");
    if let Err(e) = boot::write(dev, *psize, body, vbmeta, footer, &drop_caches) {
        let back = restore_kept(l, dev, *psize);
        let _ = modules::restore_prev(l, systems, None);
        modules::done(l);
        journal::clear(l);
        return Err(match back {
            Ok(()) => e.context("writing the boot image failed verification; the previous image is back"),
            Err(_) => e.context("RESTORE FAILED - do not reboot into Linux; reinstall from a computer"),
        });
    }
    put(l, "written.sha256", body_sha)?;
    put(l, "installed.id", id)?;
    put(l, "installed.tag", tag)?;
    put(l, "installed.kernel", kernel)?;
    settle_choice(l, krel);
    modules::done(l);
    journal::clear(l);
    Ok(())
}

fn restore_kept(l: &Layout, dev: &Path, psize: u64) -> Result<()> {
    let rd = |n: &str| fs::read(l.boot_file(n)).map_err(|e| Error::io(n, &e));
    let (body, vb, f) = (rd("prev.body")?, rd("prev.vbmeta")?, rd("prev.footer")?);
    let footer: [u8; 64] = f.try_into().map_err(|_| Error::failed("the kept footer is not 64 bytes"))?;
    boot::write(dev, psize, &body, &vb, &footer, &l.sys("/proc/sys/vm/drop_caches"))
}

/// A retired kernel choice whose successor is now in place is written down as that successor.
fn settle_choice(l: &Layout, krel: Option<&str>) {
    let chosen = fsx::read_line(&l.boot_file("kernel")).unwrap_or_default();
    if let Some((_, new)) = KERNELS_RETIRED.iter().find(|(o, _)| *o == chosen)
        && krel.is_none_or(|k| k.starts_with(&format!("{new}.")))
    {
        let _ = put(l, "kernel", new);
    }
}

/// A new boot image must meet an armed slot: its first boots are trials, and the first that comes up locks again.
fn arm_trial(l: &Layout, cfg: &UpdateConfig) {
    let Some((program, args)) = cfg.trial_command.split_first() else { return };
    let tool = l.sys(program);
    if !f50_core::exec::executable(&tool) {
        return;
    }
    // (on the blocking pool, where waiting for a program is allowed)
    let out = tokio::runtime::Handle::try_current()
        .ok()
        .map(|h| h.block_on(Cmd::new(&tool).args(args).timeout(Duration::from_secs(30)).run()));
    match out {
        Some(Ok(o)) if o.success() => {}
        Some(Ok(o)) if o.status == Some(3) => tracing::warn!(
            "the default boot is Android: this boot image gets no trial; only a boot from Android tries it"
        ),
        _ => tracing::warn!("could not arm the Linux slot for a trial boot"),
    }
}

/// Puts the kept boot image back (and its modules).
pub fn rollback(l: &Layout, systems: &[String]) -> Result<String> {
    if file_len(&l.boot_file("prev.body")) == 0 {
        return Err(Error::not_found("previous boot image kept"));
    }
    let slot = slot::linux_slot(l);
    let ps = fsx::read_line(&l.boot_file("prev.slot")).unwrap_or_else(|| "b".into());
    if ps != slot.to_string() {
        return Err(Error::failed(format!(
            "the previous boot image was taken from slot {ps}, Linux now runs from {slot}; nothing was written"
        )));
    }
    let (dev, psize) = slot::partition(l, &format!("boot_{slot}"))?;
    restore_kept(l, &dev, psize).map_err(|e| e.context("writing the previous boot image failed verification"))?;
    modules::restore_prev(l, systems, None)
        .map_err(|e| e.context("the previous boot image is back but its modules are not"))?;
    let body = fs::read(l.boot_file("prev.body")).map_err(|e| Error::io("prev.body", &e))?;
    put(l, "written.sha256", &sha256_hex(&body))?;
    let _ = fs::remove_file(l.boot_file("installed.id"));
    let prev_tag = fsx::read_line(&l.boot_file("prev.tag")).unwrap_or_else(|| "unknown".into());
    put(l, "installed.tag", &prev_tag)?;
    if let Some(k) = fsx::read_line(&l.boot_file("prev.kernel")) {
        put(l, "kernel", &k)?;
        put(l, "installed.kernel", &k)?;
    }
    Ok(format!("boot: previous boot image ({prev_tag}) restored"))
}

/// After a restart: a boot write the journal names is done again from its parts (or the kept image goes back when
/// they are gone); then a kernel step that stopped with its modules half-way is settled. Blocking.
pub fn recover(l: &Layout, systems: &[String], tmp: &Path) -> Vec<String> {
    let mut said = Vec::new();
    if let Some(step @ Step::BootWrite { .. }) = journal::read(l) {
        let Step::BootWrite { dev, psize, image_sha, footer_sha, .. } = &step else { return said };
        let parts = (fs::read(tmp.join("new")), fs::read(tmp.join("vbmeta")), fs::read(tmp.join("newfooter")));
        let ok = match parts {
            (Ok(b), Ok(v), Ok(f)) if boot::image_sha(&b, &v) == *image_sha && sha256_hex(&f) == *footer_sha => {
                let footer: Option<[u8; 64]> = f.try_into().ok();
                footer.is_some_and(|f| finish_write(l, systems, &step, &b, &v, &f, None).is_ok())
            }
            _ => false,
        };
        if ok {
            said.push("boot: the boot image whose write was cut off by a restart was written again".to_owned());
        } else {
            let r = restore_kept(l, dev, *psize);
            let _ = modules::restore_prev(l, systems, None);
            modules::done(l);
            journal::clear(l);
            said.push(match r {
                Ok(()) => "boot: a boot image write was cut off by a restart; the previous image is back".to_owned(),
                Err(_) => "boot: a boot image write was cut off and the previous image could not be written back: do not reboot into Linux".to_owned(),
            });
        }
    }
    if l.boot_file("modules.pending").exists() {
        let banner = slot::partition(l, &format!("boot_{}", slot::linux_slot(l)))
            .and_then(|(d, p)| boot::read_slot(&d, p).and_then(|s| boot::read_at(&d, boot::PAGE, s.ksz.min(64 << 20))))
            .ok()
            .and_then(|k| boot::banner(&k));
        match modules::settle(l, systems, banner.as_deref(), tmp) {
            Ok(Some(m)) => said.push(format!("boot: {m}")),
            Ok(None) => {}
            Err(e) => said.push(format!("boot: {}", e.message())),
        }
    }
    said
}

/// The kernel step's working directory.
pub fn workdir(l: &Layout) -> PathBuf {
    l.stage.join("boot")
}
