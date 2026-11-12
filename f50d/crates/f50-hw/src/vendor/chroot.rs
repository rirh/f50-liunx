//! The chroot the vendor daemons run in, made ready (`android-vendor-start`, between its account block and the exec).
//!
//! Each piece was found by failure (FINDINGS 5): Android's property area (bound, not copied: it is ~85 MiB and
//! OpenWrt's `/dev` is a 512 KiB tmpfs, where a copy silently became empty files and modem_control never booted the
//! modem), the `/dev/block/by-name` links, `/dev`, `/proc` and `/sys` inside, a liblog sink, and a command line that
//! names Android's slot (the daemons load the modem images of `androidboot.slot_suffix`; LK passes Linux's).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::slot::{self, Slot};
use crate::storage::Storage;
use crate::sys;

/// The paths the preparation touches, already under the platform's root.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub root: PathBuf,
    pub dev: PathBuf,
    pub sys: PathBuf,
    pub mountinfo: PathBuf,
    pub logdw: PathBuf,
    pub vendor_log: PathBuf,
    pub cmdline_out: PathBuf,
    pub proc_cmdline: PathBuf,
    pub bootargs: PathBuf,
    pub initramfs_run: PathBuf,
}

fn io(what: impl std::fmt::Display, e: &std::io::Error) -> String {
    format!("{what}: {e}")
}

impl Layout {
    fn mounted(&self, p: &Path) -> bool {
        sys::is_mount_point(&self.mountinfo, p)
    }

    /// `/dev/block/<part>` and `/dev/block/by-name/<PARTNAME>` for every partition of the internal disk (never a
    /// card's, which can carry the same names: FINDINGS 31l).
    pub(crate) fn link_partitions(&self) -> std::result::Result<usize, String> {
        let st = Storage::new(&self.sys);
        let Some(disk) = st.disk() else { return Ok(0) };
        let prefix = Storage::prefix(&disk);
        let by_name = self.dev.join("block/by-name");
        std::fs::create_dir_all(&by_name).map_err(|e| io(by_name.display(), &e))?;
        let class = self.sys.join("class/block");
        let mut n = 0;
        let mut names: Vec<String> = std::fs::read_dir(&class)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|d| d.strip_prefix(prefix.as_str()).is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit())))
            .collect();
        names.sort();
        for d in names {
            let Ok(uevent) = std::fs::read_to_string(class.join(&d).join("uevent")) else { continue };
            let target = Path::new("/dev").join(&d);
            relink(&target, &self.dev.join("block").join(&d));
            if let Some(name) = uevent.lines().find_map(|l| l.strip_prefix("PARTNAME=")).filter(|n| !n.contains('/'))
                && !name.is_empty()
            {
                relink(&target, &by_name.join(name));
            }
            n += 1;
        }
        Ok(n)
    }

    /// The liblog sink, unless one is listening already (FINDINGS 5, requirement 5). OpenWrt starts the vendor stage
    /// (S09) before its boot script makes `/var/log` (S10): without the directory the sink never ran and the daemons'
    /// reasons for a modem that did not boot were lost. It outlives this process on purpose (as `logdw &` did).
    pub(crate) fn start_logdw(&self) -> std::result::Result<bool, String> {
        use std::os::unix::fs::FileTypeExt;
        let sock = self.dev.join("socket/logdw");
        if std::fs::symlink_metadata(&sock).is_ok_and(|m| m.file_type().is_socket()) {
            return Ok(false);
        }
        let _ = std::fs::create_dir_all(self.dev.join("socket"));
        if let Some(d) = self.vendor_log.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if !f50_core::exec::executable(&self.logdw) {
            return Err(format!("no {}", self.logdw.display()));
        }
        let log = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.vendor_log)
            .map_err(|e| io(self.vendor_log.display(), &e))?;
        let err = log.try_clone().map_err(|e| io("log", &e))?;
        std::process::Command::new(&self.logdw)
            .arg(Path::new("/dev/socket/logdw"))
            .env_clear()
            .env("PATH", f50_core::exec::CHILD_PATH)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(err)
            .spawn()
            .map(|_| true)
            .map_err(|e| io(self.logdw.display(), &e))
    }

    /// `/dev/__properties__`: Android's property area bound read-only from the vendor tree.
    pub(crate) fn bind_properties(&self) -> std::result::Result<bool, String> {
        let props = self.dev.join("__properties__");
        if file_nonempty(&props.join("property_info")) {
            return Ok(false);
        }
        if self.mounted(&props) {
            return Err(format!("{} is mounted and has no property_info", props.display()));
        }
        let _ = std::fs::remove_dir_all(&props);
        let _ = std::fs::remove_file(&props);
        std::fs::create_dir_all(&props).map_err(|e| io(props.display(), &e))?;
        sys::bind(&self.root.join("dev-properties"), &props).map_err(|e| io("bind the property area", &e))?;
        Ok(true)
    }

    /// `/dev`, the property area, `/proc` and `/sys` inside the chroot, and its writable directories.
    pub(crate) fn mount_inside(&self) -> std::result::Result<(), String> {
        for d in ["dev", "proc", "sys", "data/vendor", "mnt", "tmp"] {
            let p = self.root.join(d);
            std::fs::create_dir_all(&p).map_err(|e| io(p.display(), &e))?;
        }
        let a = |d: &str| self.root.join(d);
        if !self.mounted(&a("dev")) {
            sys::rbind(&self.dev, &a("dev")).map_err(|e| io("rbind /dev", &e))?;
        }
        if !file_nonempty(&a("dev/__properties__/property_info")) {
            sys::bind(&a("dev-properties"), &a("dev/__properties__")).map_err(|e| io("bind the property area", &e))?;
        }
        if !self.mounted(&a("proc")) {
            sys::mount_fs("proc", &a("proc")).map_err(|e| io("mount proc", &e))?;
        }
        if !self.mounted(&a("sys")) {
            sys::mount_fs("sysfs", &a("sys")).map_err(|e| io("mount sysfs", &e))?;
        }
        Ok(())
    }

    /// The command line the daemons see: the slot Linux did not boot from (b when Linux runs from a). Linux's slot is
    /// the one the initramfs published, else the one in this command line.
    pub(crate) fn android_cmdline(&self) -> std::result::Result<Slot, String> {
        // the mainline kernel forces its own command line: LK's bootargs in the device tree then
        let proc_text = std::fs::read(&self.proc_cmdline).unwrap_or_default();
        let src = if contains(&proc_text, b"androidboot.slot_suffix") || std::fs::metadata(&self.bootargs).is_err() {
            proc_text
        } else {
            std::fs::read(&self.bootargs).unwrap_or_default()
        };
        let linux = slot::published(&self.initramfs_run).or_else(|| slot::slot_in_cmdline(&src)).unwrap_or(Slot::B);
        let android = linux.other();
        let target = self.root.join("proc/cmdline");
        if self.mounted(&target) {
            return Ok(android);
        }
        let text = rewrite_slot(&src, linux, android);
        std::fs::write(&self.cmdline_out, text).map_err(|e| io(self.cmdline_out.display(), &e))?;
        sys::bind(&self.cmdline_out, &target).map_err(|e| io("bind the command line", &e))?;
        Ok(android)
    }
}

/// `tr -d '\000' | sed "s/androidboot.slot_suffix=_L/androidboot.slot_suffix=_A/"`: NULs gone, the first
/// occurrence on each line replaced.
pub(crate) fn rewrite_slot(src: &[u8], linux: Slot, android: Slot) -> Vec<u8> {
    let from = format!("androidboot.slot_suffix=_{}", linux.letter()).into_bytes();
    let to = format!("androidboot.slot_suffix=_{}", android.letter()).into_bytes();
    let clean: Vec<u8> = src.iter().copied().filter(|&b| b != 0).collect();
    let mut out = Vec::with_capacity(clean.len() + 1);
    let lines: Vec<&[u8]> = clean.split(|&b| b == b'\n').collect();
    for (i, line) in lines.iter().enumerate() {
        match line.windows(from.len()).position(|w| w == from.as_slice()) {
            Some(p) => {
                out.extend_from_slice(&line[..p]);
                out.extend_from_slice(&to);
                out.extend_from_slice(&line[p + from.len()..]);
            }
            None => out.extend_from_slice(line),
        }
        if i + 1 < lines.len() {
            out.push(b'\n');
        }
    }
    // sed ends its last line with a newline
    if out.last() != Some(&b'\n') {
        out.push(b'\n');
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn file_nonempty(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.len() > 0)
}

/// `ln -sfn target link`.
fn relink(target: &Path, link: &Path) {
    if std::fs::read_link(link).is_ok_and(|t| t == target) {
        return;
    }
    let _ = std::fs::remove_file(link);
    let _ = std::os::unix::fs::symlink(target, link);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daemons_get_androids_slot() {
        let src = b"console=ttyS1 androidboot.slot_suffix=_b androidboot.mode=normal\0";
        assert_eq!(
            rewrite_slot(src, Slot::B, Slot::A),
            b"console=ttyS1 androidboot.slot_suffix=_a androidboot.mode=normal\n"
        );
        assert_eq!(rewrite_slot(b"x\n", Slot::B, Slot::A), b"x\n");
    }

    #[test]
    fn partition_links_from_a_fake_sysfs() {
        let root = std::env::temp_dir().join(format!("f50-hw-links-{}", std::process::id()));
        let (sysd, dev) = (root.join("sys"), root.join("dev"));
        std::fs::create_dir_all(sysd.join("block/mmcblk0/device")).expect("mkdir");
        std::fs::write(sysd.join("block/mmcblk0/device/type"), "MMC\n").expect("type");
        for (n, u) in [("mmcblk0p1", "PARTNAME=misc\n"), ("mmcblk0p3", "MAJOR=179\n"), ("mmcblk0boot0", "")] {
            std::fs::create_dir_all(sysd.join("class/block").join(n)).expect("mkdir");
            std::fs::write(sysd.join("class/block").join(n).join("uevent"), u).expect("uevent");
        }
        let l = Layout {
            root: root.join("a"),
            dev: dev.clone(),
            sys: sysd,
            mountinfo: root.join("mountinfo"),
            logdw: root.join("logdw"),
            vendor_log: root.join("log"),
            cmdline_out: root.join("cmdline"),
            proc_cmdline: root.join("proc-cmdline"),
            bootargs: root.join("bootargs"),
            initramfs_run: root.join("run"),
        };
        assert_eq!(l.link_partitions(), Ok(2));
        assert_eq!(std::fs::read_link(dev.join("block/by-name/misc")).ok(), Some(PathBuf::from("/dev/mmcblk0p1")));
        assert_eq!(std::fs::read_link(dev.join("block/mmcblk0p3")).ok(), Some(PathBuf::from("/dev/mmcblk0p3")));
        assert_eq!(l.link_partitions(), Ok(2));
        let _ = std::fs::remove_dir_all(&root);
    }
}
