//! The AOSP `bootloader_control` block LK reads: 32 bytes at offset 2048 of the `misc` partition.
//!
//! Layout (FINDINGS 1): `slot_suffix[4]`, magic `BCAB` (bytes 4..8), version, flags, reserved; `slot_info[4]` of 2
//! bytes each from byte 12, so slot a's byte is 12 and slot b's 14: `priority | tries_remaining << 4 |
//! successful_boot << 7`; the last four bytes the CRC-32 (zlib's, little-endian) of the first 28. Unisoc LK treats
//! `tries == 1 && !successful` as an already failed boot and rolls back to the other slot, so an armed slot needs
//! tries >= 2; LK never counts down a successful slot. Every write is read back before anything else relies on it.

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::slot::Slot;

/// Where the block lives in misc.
pub(crate) const OFFSET: u64 = 2048;
pub(crate) const LEN: usize = 32;

/// One block.
pub(crate) type Block = [u8; LEN];

/// zlib's CRC-32 (IEEE 802.3, reflected, as gzip ends every stream with: what the shell used to compute it).
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// The byte of a slot's `slot_info`.
pub(crate) const fn slot_byte(s: Slot) -> usize {
    match s {
        Slot::A => 12,
        Slot::B => 14,
    }
}

/// The block with `slot`'s info byte set to priority 15, `tries` and `successful`, and its CRC made again
/// (`make_bc_b`). `tries` must fit 3 bits.
pub(crate) fn with_slot(base: &Block, slot: Slot, tries: u8, successful: bool) -> Option<Block> {
    if tries > 7 {
        return None;
    }
    let mut b = *base;
    b[slot_byte(slot)] = 0x0f | (tries << 4) | (u8::from(successful) << 7);
    let crc = crc32(&b[..28]);
    b[28..].copy_from_slice(&crc.to_le_bytes());
    Some(b)
}

/// A slot's info, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SlotInfo {
    pub priority: u8,
    pub tries: u8,
    pub successful: bool,
}

pub(crate) fn slot_info(b: &Block, s: Slot) -> SlotInfo {
    let v = b[slot_byte(s)];
    SlotInfo { priority: v & 0x0f, tries: (v >> 4) & 7, successful: v & 0x80 != 0 }
}

pub(crate) fn has_magic(b: &Block) -> bool {
    &b[4..8] == b"BCAB"
}

pub(crate) fn crc_ok(b: &Block) -> bool {
    crc32(&b[..28]).to_le_bytes() == b[28..]
}

/// A block file the initramfs published: exactly 32 bytes, else `None`.
pub(crate) fn read_file(p: &Path) -> Option<Block> {
    std::fs::read(p).ok()?.try_into().ok()
}

/// The block in misc now.
pub(crate) fn read_misc(misc: &Path) -> std::io::Result<Block> {
    let mut f = std::fs::File::open(misc)?;
    f.seek(SeekFrom::Start(OFFSET))?;
    let mut b = [0u8; LEN];
    f.read_exact(&mut b)?;
    Ok(b)
}

/// Writes the block into misc (never creating a file there), flushes it, and reads it back.
pub(crate) fn write_misc(misc: &Path, b: &Block) -> Result<(), &'static str> {
    let w = || -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).open(misc)?;
        f.seek(SeekFrom::Start(OFFSET))?;
        f.write_all(b)?;
        f.sync_all()
    };
    w().map_err(|_| "writing misc failed")?;
    match read_misc(misc) {
        Ok(back) if back == *b => Ok(()),
        _ => Err("misc verify failed"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn hex(s: &str) -> Block {
        let v: Vec<u8> = (0..s.len()).step_by(2).filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
        v.try_into().unwrap_or([0; LEN])
    }

    /// The live block of tests/test_device_scripts.py (NextBoot): Android on a, successful.
    pub(crate) const LIVE: &str = "5f61000042434142010200009f001e000000000000000000000000000be17146";

    /// build-boot-image.py's `with_slots`: the four blocks the initramfs carries.
    pub(crate) fn with_slots(suffix: &[u8; 2], a: u8, b: u8) -> Block {
        let mut x = hex(LIVE);
        x[0..4].copy_from_slice(&[suffix[0], suffix[1], 0, 0]);
        x[12] = a;
        x[14] = b;
        let crc = crc32(&x[..28]);
        x[28..].copy_from_slice(&crc.to_le_bytes());
        x
    }

    #[test]
    fn crc_is_zlibs() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
        let live = hex(LIVE);
        assert!(has_magic(&live) && crc_ok(&live));
        assert_eq!(slot_info(&live, Slot::A), SlotInfo { priority: 15, tries: 1, successful: true });
    }

    #[test]
    fn the_rebuilt_trial_block_is_the_initramfs_byte_for_byte() {
        let linux_b = with_slots(b"_b", 0x9e, 0x2f);
        let linux_a = with_slots(b"_a", 0x2f, 0x9e);
        assert_eq!(with_slot(&linux_b, Slot::B, 2, false), Some(linux_b));
        assert_eq!(with_slot(&linux_a, Slot::A, 2, false), Some(linux_a));
        // 5 attempts: tries 6 in the slot's byte, the rest as in the trial block, the CRC made again
        let armed = with_slot(&linux_b, Slot::B, 6, false).unwrap_or([0; LEN]);
        assert_eq!(armed[14], 0x6f);
        assert_eq!(armed[..14], linux_b[..14]);
        assert_eq!(armed[15..28], linux_b[15..28]);
        assert!(crc_ok(&armed));
        // locked: successful, prio 15, tries 6
        assert_eq!(with_slot(&linux_b, Slot::B, 6, true).map(|b| b[14]), Some(0xef));
        assert_eq!(with_slot(&linux_a, Slot::A, 6, false).map(|b| b[12]), Some(0x6f));
        assert_eq!(with_slot(&linux_a, Slot::A, 8, false), None);
    }

    #[test]
    fn a_write_is_read_back_and_never_creates_misc() {
        let d = std::env::temp_dir().join(format!("f50-hw-misc-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("mkdir");
        let misc = d.join("misc");
        let mut img = vec![0u8; 0x800];
        img.extend_from_slice(&hex(LIVE));
        img.extend_from_slice(&[0u8; 2016]);
        std::fs::write(&misc, &img).expect("misc");
        let blk = with_slots(b"_b", 0x9e, 0x6f);
        assert_eq!(write_misc(&misc, &blk), Ok(()));
        assert_eq!(read_misc(&misc).ok(), Some(blk));
        assert_eq!(std::fs::metadata(&misc).map(|m| m.len()).ok(), Some(4096));
        assert_eq!(write_misc(&d.join("none"), &blk), Err("writing misc failed"));
        assert!(!d.join("none").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
