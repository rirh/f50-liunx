//! What `f50.boot get` reports (`mu300-next-boot status`): read only, no lock taken, nothing written.

use serde::Serialize;

use super::mark::BootCtl;
use super::misc::{self, SlotInfo};
use crate::report::StageReport;
use crate::slot::Slot;

/// The boot control as it stands, and the last result of every boot stage.
#[derive(Debug, Clone, Serialize)]
pub struct BootInfo {
    /// The slot Linux runs from.
    pub slot: Slot,
    /// `linux` keeps Linux armed; `android` (also when the file is missing) leaves every Linux boot a one-shot trial.
    pub default_boot: String,
    /// Failed boots in a row before Android (only while a new boot image is on trial when locked).
    pub attempts: u8,
    /// Linux stays whatever happens (`.f50/boot-lock`).
    pub locked: bool,
    /// Boots since the last good one, as the initramfs counted them.
    pub boots_since_good: Option<u32>,
    /// This slot's pair of blocks is there, matches it, and misc can be written.
    pub writable: bool,
    /// The block in misc now, and the two slots it describes; `null` when misc cannot be read.
    pub misc: Option<MiscState>,
    pub stages: Vec<StageReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MiscState {
    pub hex: String,
    pub magic: bool,
    pub crc_ok: bool,
    pub slot_a: SlotInfo,
    pub slot_b: SlotInfo,
}

impl BootCtl {
    pub(crate) fn info(&self, stages: Vec<StageReport>) -> BootInfo {
        let pair = self.pair();
        let misc_dev = self.misc();
        let writable = misc::read_file(&pair.android_file).is_some()
            && misc::read_file(&pair.linux_file).is_some()
            && BootCtl::pair_ok(&pair)
            && misc_dev.as_deref().is_some_and(|m| m.exists() && crate::sys::writable(m));
        let misc = misc_dev.and_then(|m| misc::read_misc(&m).ok()).map(|b| MiscState {
            hex: b.iter().map(|x| format!("{x:02x}")).collect(),
            magic: misc::has_magic(&b),
            crc_ok: misc::crc_ok(&b),
            slot_a: misc::slot_info(&b, Slot::A),
            slot_b: misc::slot_info(&b, Slot::B),
        });
        BootInfo {
            slot: pair.linux,
            default_boot: self.default_boot().unwrap_or_else(|| "android".into()),
            attempts: self.attempts(),
            locked: self.lock_wanted(),
            boots_since_good: self
                .disk_dir()
                .and_then(|d| crate::slot::read_word(&d.join("boot-tries")))
                .and_then(|s| s.trim().parse().ok()),
            writable,
            misc,
            stages,
        }
    }
}
