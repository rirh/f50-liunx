//! Where the boot image goes and where the system lives, as the boot image's init must find it.
//!
//! - The slot Linux runs from (Android may be on either; an A/B device): what the initramfs published
//!   (`/run/f50/linux-slot`), else the slot LK booted (`androidboot.slot_suffix` on the command line or in the
//!   device tree's bootargs), and b when nothing says (an initramfs from before slot a).
//! - The boot partition of that slot, on the internal disk only (the eMMC whatever its number, FINDINGS 31l; never
//!   a card, which can carry the same GPT names), and only when /dev has a block device for it (FINDINGS 86: a /dev
//!   rebuilt under a writer once made it a plain file in RAM).
//! - Where the root is (`/run/f50-root-dev`): the region after the eMMC's last partition, the SD card, a GPT
//!   partition `f50root` (`mu300root` before the rename), or an image file (another installer's, which no init of ours mounts: issue #290).

use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

use f50_core::fsx::read_line;
use f50_core::{Error, Result};

use crate::config::Layout;

pub fn linux_slot(l: &Layout) -> char {
    if let Some(s) = read_line(&l.sys("/run/f50/linux-slot"))
        && (s == "a" || s == "b")
    {
        return if s == "a" { 'a' } else { 'b' };
    }
    for f in ["/proc/cmdline", "/proc/device-tree/chosen/bootargs"] {
        let Ok(raw) = fs::read(l.sys(f)) else { continue };
        let text = String::from_utf8_lossy(&raw).replace('\0', " ");
        if let Some(i) = text.find("androidboot.slot_suffix=_") {
            match text.as_bytes().get(i + 25) {
                Some(b'a') => return 'a',
                Some(b'b') => return 'b',
                _ => {}
            }
        }
    }
    'b'
}

/// Where the running system's disk is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootLayout {
    Region,
    Sd,
    Part,
    Image,
    Unknown,
}

pub fn root_layout(l: &Layout) -> RootLayout {
    match read_line(&l.sys("/run/f50-root-dev")) {
        Some(s) if s.ends_with("@f50root") || s.ends_with("@mu300root") => RootLayout::Part,
        Some(s) if s.contains('@') => RootLayout::Region,
        Some(s) if !s.is_empty() => RootLayout::Sd,
        _ => {
            let dd = read_line(&l.sys("/run/f50/disk-dev")).unwrap_or_default();
            let dd = dd.rsplit('/').next().unwrap_or("").to_owned();
            if dd.is_empty() {
                RootLayout::Unknown
            } else if dd.starts_with("loop") {
                match read_line(&l.sys(format!("/sys/class/block/{dd}/loop/backing_file"))) {
                    Some(b) if b.starts_with("/dev/") => RootLayout::Region,
                    Some(b) if !b.is_empty() => RootLayout::Image,
                    _ => RootLayout::Unknown,
                }
            } else {
                RootLayout::Unknown
            }
        }
    }
}

/// The internal disk (an mmcblk whose card is MMC).
fn internal_disk(l: &Layout) -> Option<String> {
    let mut disks: Vec<String> = fs::read_dir(l.sys("/sys/block"))
        .ok()?
        .flatten()
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .filter(|n| n.strip_prefix("mmcblk").is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit())))
        .collect();
    disks.sort();
    disks.into_iter().find(|d| read_line(&l.sys(format!("/sys/block/{d}/device/type"))).as_deref() == Some("MMC"))
}

/// The block device of the partition `name` on the internal disk, and its size in bytes.
pub fn partition(l: &Layout, name: &str) -> Result<(PathBuf, u64)> {
    let disk = internal_disk(l).ok_or_else(|| Error::failed("the internal disk was not found"))?;
    let dir = l.sys(format!("/sys/block/{disk}"));
    let prefix = format!("{disk}p");
    let mut parts: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| Error::io(dir.display(), &e))?
        .flatten()
        .map(|d| d.file_name().to_string_lossy().into_owned())
        .filter(|n| n.strip_prefix(&prefix).is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit())))
        .collect();
    parts.sort();
    for p in parts {
        let uevent = fs::read_to_string(dir.join(&p).join("uevent")).unwrap_or_default();
        if !uevent.lines().any(|line| line == format!("PARTNAME={name}")) {
            continue;
        }
        let dev = l.sys(format!("/dev/{p}"));
        let is_block = fs::metadata(&dev).is_ok_and(|m| m.file_type().is_block_device());
        if !is_block {
            return Err(Error::failed(format!(
                "{} is not a block device (a reboot makes /dev anew): {name} was not touched",
                dev.display()
            )));
        }
        let sectors: u64 = f50_core::fsx::read_num(&dir.join(&p).join("size")).unwrap_or(0);
        if sectors == 0 {
            return Err(Error::failed(format!("{name}: its size is not known")));
        }
        return Ok((dev, sectors * 512));
    }
    Err(Error::failed(format!("no {name} partition found")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::paths::Paths;

    #[test]
    fn slot_and_layout() {
        let root = std::env::temp_dir().join(format!("f50-update-slot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let l = Layout::new(&Paths::new(&root));
        assert_eq!(linux_slot(&l), 'b');
        fs::create_dir_all(root.join("proc")).expect("proc");
        fs::write(root.join("proc/cmdline"), "console=ttyS1 androidboot.slot_suffix=_a quiet").expect("cmdline");
        assert_eq!(linux_slot(&l), 'a');
        fs::create_dir_all(root.join("run")).expect("run");
        fs::write(root.join("run/f50-root-dev"), "mmcblk0@123456\n").expect("dev");
        assert_eq!(root_layout(&l), RootLayout::Region);
        fs::write(root.join("run/f50-root-dev"), "mmcblk1p1\n").expect("dev");
        assert_eq!(root_layout(&l), RootLayout::Sd);
        assert!(partition(&l, "boot_b").is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
