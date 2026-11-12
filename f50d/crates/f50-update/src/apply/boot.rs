//! The boot image of the Linux slot (the shell's `boot_update_run`, `bootimg_assemble`, `write_boot`): an Android
//! boot image header v4 (the stock header page, "ANDROID!", version 4 at byte 40, the kernel's size at 8 and the
//! ramdisk's at 12), the kernel and the ramdisk page-aligned, the vbmeta right behind the body, and the AVB footer in
//! the last 64 bytes of the partition ("AVBf"; big-endian: the image's size at 12, the vbmeta's offset at 20 and
//! size at 28). The ramdisk is the device's own segment (taken once from the image an installer wrote) and the
//! release's generic segment, both LZ4 legacy (FINDINGS 2). The image must end before the persistent init log at
//! 48 MiB.
//!
//! Writing the partition is the one step that is neither atomic nor undoable by a rename. It is made restartable
//! instead: the parts of the new image are files in the staging directory and the image read from the slot is kept
//! (`boot/prev.*`) before the first byte is written, the journal names the hashes, and the write is verified by
//! reading it back. Writing the same bytes again is harmless, so recovery after a restart writes them again (or the
//! kept image back, when the parts are gone). A device that loses power half-way has a slot that does not boot: it
//! falls back to Android by itself after its failed attempts (f50d boot trial / f50-hw bootctl), and `rollback` puts the kept image
//! back from there.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;

use f50_core::{Error, Result};

use crate::util::{hex, sha256_hex};

pub const PAGE: u64 = 4096;
pub const PERSIST_LOG: u64 = 48 << 20;
const LZ4_LEGACY: [u8; 4] = [0x02, 0x21, 0x4c, 0x18];

/// What the slot holds now.
#[derive(Debug, Clone)]
pub struct SlotImage {
    pub hdr: Vec<u8>,
    pub footer: [u8; 64],
    pub ksz: u64,
    pub rsz: u64,
    /// The image's size (body), and its vbmeta's.
    pub osize: u64,
    pub vbs: u64,
}

fn u32le(b: &[u8], at: usize) -> u64 {
    u64::from(u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]))
}

fn u64be(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_be_bytes(a)
}

fn pad(n: u64) -> u64 {
    n.div_ceil(PAGE) * PAGE
}

pub fn read_at(dev: &Path, off: u64, len: u64) -> Result<Vec<u8>> {
    let f = File::open(dev).map_err(|e| Error::io(format!("open {}", dev.display()), &e))?;
    let n = usize::try_from(len).map_err(|_| Error::failed("too large"))?;
    let mut v = vec![0u8; n];
    f.read_exact_at(&mut v, off).map_err(|e| Error::io(format!("read {}", dev.display()), &e))?;
    Ok(v)
}

/// Parses the header page and the footer of the slot; refuses what is not a v4 image with an AVB footer.
pub fn read_slot(dev: &Path, psize: u64) -> Result<SlotImage> {
    if psize < 2 * PAGE {
        return Err(Error::failed("the boot partition is too small"));
    }
    let hdr = read_at(dev, 0, PAGE)?;
    let f = read_at(dev, psize - 64, 64)?;
    if &hdr[..8] != b"ANDROID!" || u32le(&hdr, 40) != 4 {
        return Err(Error::failed("the boot partition does not hold a boot image header v4, not touching it"));
    }
    if &f[..4] != b"AVBf" {
        return Err(Error::failed("the boot partition has no AVB footer, not touching it"));
    }
    let mut footer = [0u8; 64];
    footer.copy_from_slice(&f);
    let osize = u64be(&footer, 12);
    let vbs = u64be(&footer, 28);
    if osize == 0 || !osize.is_multiple_of(PAGE) || osize + vbs > psize - 64 {
        return Err(Error::failed("the boot image's footer does not fit its partition, not touching it"));
    }
    Ok(SlotImage { ksz: u32le(&hdr, 8), rsz: u32le(&hdr, 12), hdr, footer, osize, vbs })
}

impl SlotImage {
    /// The device's ramdisk segment out of the image in the slot (`body`), checked to be LZ4 legacy.
    pub fn ramdisk<'a>(&self, body: &'a [u8]) -> Result<&'a [u8]> {
        let off = usize::try_from(PAGE + pad(self.ksz)).map_err(|_| Error::failed("bad header"))?;
        let len = usize::try_from(self.rsz).map_err(|_| Error::failed("bad header"))?;
        let r = body.get(off..off + len).ok_or_else(|| Error::failed("the ramdisk is not inside the image"))?;
        if r.len() < 4 || r[..4] != LZ4_LEGACY {
            return Err(Error::failed("the ramdisk in the slot is not LZ4 legacy, not touching it"));
        }
        Ok(r)
    }
}

/// A new body (header page with the new sizes, kernel, ramdisks) and its footer.
pub fn assemble(slot: &SlotImage, kernel: &[u8], ramdisks: &[&[u8]]) -> Result<(Vec<u8>, [u8; 64])> {
    let rsz: usize = ramdisks.iter().map(|r| r.len()).sum();
    let (k32, r32) = (u32::try_from(kernel.len()), u32::try_from(rsz));
    let (Ok(k32), Ok(r32)) = (k32, r32) else { return Err(Error::failed("kernel or ramdisk too large")) };
    let mut body = slot.hdr.clone();
    body[8..12].copy_from_slice(&k32.to_le_bytes());
    body[12..16].copy_from_slice(&r32.to_le_bytes());
    body.extend_from_slice(kernel);
    body.resize(body.len().div_ceil(PAGE as usize) * PAGE as usize, 0);
    for r in ramdisks {
        body.extend_from_slice(r);
    }
    body.resize(body.len().div_ceil(PAGE as usize) * PAGE as usize, 0);
    let os = body.len() as u64;
    if os + slot.vbs > PERSIST_LOG {
        return Err(Error::failed(format!(
            "the new image ({os} bytes, with vbmeta {}) would reach the persistent log area at {PERSIST_LOG} bytes",
            slot.vbs
        )));
    }
    let mut footer = slot.footer;
    footer[12..20].copy_from_slice(&os.to_be_bytes());
    footer[20..28].copy_from_slice(&os.to_be_bytes());
    footer[28..36].copy_from_slice(&slot.vbs.to_be_bytes());
    Ok((body, footer))
}

/// The hash of body and vbmeta as they are on the partition after a write.
pub fn image_sha(body: &[u8], vbmeta: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(body);
    h.update(vbmeta);
    hex(&h.finalize())
}

/// Writes body, vbmeta behind it and the footer into the last 64 bytes, then reads all of it back (the page cache
/// dropped first, where we may). Blocking.
pub fn write(dev: &Path, psize: u64, body: &[u8], vbmeta: &[u8], footer: &[u8; 64], drop_caches: &Path) -> Result<()> {
    let io = |e: std::io::Error| Error::io(format!("write {}", dev.display()), &e);
    let mut f = OpenOptions::new().write(true).open(dev).map_err(io)?;
    f.seek(SeekFrom::Start(0)).map_err(io)?;
    f.write_all(body).map_err(io)?;
    f.write_all(vbmeta).map_err(io)?;
    f.write_all_at(footer, psize - 64).map_err(io)?;
    f.sync_all().map_err(io)?;
    drop(f);
    let _ = std::fs::write(drop_caches, "3");
    let mut back = Vec::with_capacity(body.len() + vbmeta.len());
    File::open(dev).map_err(io)?.take((body.len() + vbmeta.len()) as u64).read_to_end(&mut back).map_err(io)?;
    let foot = read_at(dev, psize - 64, 64)?;
    if sha256_hex(&back) != image_sha(body, vbmeta) || foot != footer {
        return Err(Error::failed("the boot image read back is not the one written"));
    }
    Ok(())
}

/// The banner `/proc/version` shows: the one with the build number (`#1 SMP ...`), which tells two builds apart.
pub fn banner(kernel: &[u8]) -> Option<String> {
    let re = regex::bytes::Regex::new(r"Linux version [0-9][\x20-\x7e]* #[0-9][\x20-\x7e]*").ok()?;
    let m = re.find(kernel)?;
    let s = String::from_utf8_lossy(m.as_bytes()).into_owned();
    Some(f50_core::sanitize::truncate_bytes(&s, 300).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot() -> SlotImage {
        let mut hdr = vec![0u8; PAGE as usize];
        hdr[..8].copy_from_slice(b"ANDROID!");
        hdr[40] = 4;
        let mut footer = [0u8; 64];
        footer[..4].copy_from_slice(b"AVBf");
        SlotImage { hdr, footer, ksz: 0, rsz: 0, osize: PAGE, vbs: 2000 }
    }

    #[test]
    fn assemble_and_read_back() {
        let s = slot();
        let rd = [0x02u8, 0x21, 0x4c, 0x18, 9, 9];
        let (body, footer) = assemble(&s, &[1u8; 5000], &[&rd, &rd]).expect("assemble");
        assert_eq!(body.len() as u64, PAGE + 2 * PAGE + PAGE);
        assert_eq!(u32le(&body, 8), 5000);
        assert_eq!(u32le(&body, 12), 12);
        assert_eq!(u64be(&footer, 12), body.len() as u64);
        assert_eq!(u64be(&footer, 28), 2000);
        // the image read back as a slot: its ramdisk is where the header says
        let parsed = SlotImage { ksz: 5000, rsz: 12, ..s.clone() };
        assert_eq!(parsed.ramdisk(&body).expect("ramdisk").len(), 12);
        assert!(assemble(&s, &vec![0u8; PERSIST_LOG as usize], &[]).is_err(), "would reach the log area");
        // on a file standing in for the partition
        let dev = std::env::temp_dir().join(format!("f50-update-boot-{}", std::process::id()));
        std::fs::write(&dev, vec![0u8; 64 * 1024]).expect("dev");
        write(&dev, 64 * 1024, &body, &[7u8; 2000], &footer, Path::new("/nonexistent")).expect("write");
        let r = read_slot(&dev, 64 * 1024).expect("slot");
        assert_eq!((r.ksz, r.rsz, r.osize), (5000, 12, body.len() as u64));
        let _ = std::fs::remove_file(&dev);
    }

    #[test]
    fn kernel_banners() {
        let k = b"xx Linux version 6.18.57 (gcc) # SMP\0yy Linux version 6.18.57 (b@h) (gcc) #4 SMP Tue Oct 27\0";
        assert_eq!(banner(k).as_deref(), Some("Linux version 6.18.57 (b@h) (gcc) #4 SMP Tue Oct 27"));
    }
}
