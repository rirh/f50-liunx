//! The device's internal disk - an eMMC (`mmcblkN`) or a UFS (`sdX`) - and its GPT partitions, found by what they
//! are, never by number or name alone (`lib/storage.sh`, after kanoqwq's fe52afe3, issue #396).
//!
//! The eMMC is the disk whose card is of type `MMC`, else the first `mmcblk` disk that is not an `SD` card: the host
//! index follows probe order, and under mainline the card slot once took mmc0 (FINDINGS 31l). The UFS is a SCSI disk
//! below the UFS host (`.../22000000.ufs/host0/...`), the logical unit that holds `misc`. Never a card or a USB disk:
//! they can carry the same GPT names (a raw clone of a device backup), and what is found here is written to.

use std::path::{Path, PathBuf};

/// The internal disk and its partitions, under one sysfs root (`/sys`).
#[derive(Debug, Clone)]
pub(crate) struct Storage {
    sys: PathBuf,
}

/// The entries of a directory whose name starts with `prefix` and continues with digits only, sorted as a shell glob.
fn numbered(dir: &Path, prefix: &str, digits_then_anything: bool) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<String> = rd
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| {
            let Some(rest) = n.strip_prefix(prefix) else { return false };
            if digits_then_anything {
                rest.starts_with(|c: char| c.is_ascii_digit())
            } else {
                !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
            }
        })
        .collect();
    v.sort();
    v
}

impl Storage {
    pub(crate) fn new(sys: impl Into<PathBuf>) -> Self {
        Storage { sys: sys.into() }
    }

    fn block(&self) -> PathBuf {
        self.sys.join("block")
    }

    fn card_type(&self, disk: &str) -> Option<String> {
        f50_core::fsx::read_line(&self.block().join(disk).join("device/type"))
    }

    /// The whole `mmcblkN` disks (not `mmcblk0boot0`, `mmcblk0rpmb`).
    fn mmc_disks(&self) -> Vec<String> {
        numbered(&self.block(), "mmcblk", false)
    }

    fn is_ufs(&self, disk: &str) -> bool {
        let n = disk.strip_prefix("sd").unwrap_or("");
        if !(1..=2).contains(&n.len()) || !n.bytes().all(|b| b.is_ascii_lowercase()) {
            return false;
        }
        let Ok(real) = std::fs::canonicalize(self.block().join(disk)) else { return false };
        let s = real.to_string_lossy();
        // */*.ufs/host[0-9]*/*
        s.split('/').collect::<Vec<_>>().windows(3).any(|w| {
            w[0].ends_with(".ufs")
                && w[0].len() > 4
                && w[1].strip_prefix("host").is_some_and(|h| h.starts_with(|c: char| c.is_ascii_digit()))
        })
    }

    /// What a disk's partitions are named after: `mmcblk0p`, `sda`.
    pub(crate) fn prefix(disk: &str) -> String {
        if disk.ends_with(|c: char| c.is_ascii_digit()) { format!("{disk}p") } else { disk.to_owned() }
    }

    /// The partitions of a disk (`mmcblk0p1`, ...), in glob order.
    fn parts(&self, disk: &str) -> Vec<String> {
        let dir = self.block().join(disk);
        numbered(&dir, &Self::prefix(disk), true).into_iter().filter(|p| dir.join(p).is_dir()).collect()
    }

    /// The partition named `name` in the GPT of `disk`.
    pub(crate) fn part(&self, name: &str, disk: &str) -> Option<String> {
        let want = format!("PARTNAME={name}");
        self.parts(disk).into_iter().find(|p| {
            std::fs::read_to_string(self.block().join(disk).join(p).join("uevent"))
                .is_ok_and(|u| u.lines().any(|l| l == want))
        })
    }

    /// The internal disk's name; `None` while it is not there (yet).
    pub(crate) fn disk(&self) -> Option<String> {
        let mmc = self.mmc_disks();
        if let Some(d) = mmc.iter().find(|d| self.card_type(d).as_deref() == Some("MMC")) {
            return Some(d.clone());
        }
        let mut sd: Vec<String> = std::fs::read_dir(self.block())
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("sd"))
            .collect();
        sd.sort();
        if let Some(d) = sd.into_iter().find(|d| self.is_ufs(d) && self.part("misc", d).is_some()) {
            return Some(d);
        }
        mmc.into_iter().find(|d| self.card_type(d).as_deref() != Some("SD"))
    }

    /// The internal disk's partition `name` (`mmcblk0p38`) and its sysfs directory.
    pub(crate) fn find(&self, name: &str) -> Option<(String, PathBuf)> {
        let disk = self.disk()?;
        let p = self.part(name, &disk)?;
        let sys = self.block().join(&disk).join(&p);
        Some((p, sys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(root: &Path, disks: &[(&str, Option<&str>, &[&str])]) {
        for (disk, ty, parts) in disks {
            let d = root.join("block").join(disk);
            std::fs::create_dir_all(d.join("device")).expect("mkdir");
            if let Some(t) = ty {
                std::fs::write(d.join("device/type"), format!("{t}\n")).expect("type");
            }
            for (i, name) in parts.iter().enumerate() {
                let p = d.join(format!("{}{}", Storage::prefix(disk), i + 1));
                std::fs::create_dir_all(&p).expect("part");
                std::fs::write(p.join("uevent"), format!("MAJOR=179\nPARTNAME={name}\n")).expect("uevent");
            }
        }
    }

    #[test]
    fn a_card_with_an_emmc_gpt_is_never_chosen() {
        let root = std::env::temp_dir().join(format!("f50-hw-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        layout(
            &root,
            &[("mmcblk0", Some("SD"), &["misc", "boot_b"]), ("mmcblk1", Some("MMC"), &["misc", "boot_a", "boot_b"])],
        );
        let s = Storage::new(&root);
        assert_eq!(s.disk().as_deref(), Some("mmcblk1"));
        assert_eq!(s.find("boot_b").map(|(p, _)| p).as_deref(), Some("mmcblk1p3"));
        let _ = std::fs::remove_dir_all(&root);
        layout(&root, &[("mmcblk0", Some("SD"), &["misc"])]);
        assert_eq!(Storage::new(&root).disk(), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
