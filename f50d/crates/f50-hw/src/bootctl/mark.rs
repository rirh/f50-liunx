//! A good boot re-arms the Linux slot (`mu300-next-boot --rearm`), and a new boot image is armed as a trial
//! (`--trial`); both under the misc lock, both byte for byte what the shell wrote.

use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use f50_core::paths::Paths;
use f50_core::{Error, Result};
use serde::Serialize;

use super::misc::{self, Block};
use crate::slot::{self, Slot, read_word};

/// What an arm did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ArmOutcome {
    /// The Linux slot is armed: `attempts` failed boots in a row before Android, or none when `locked`.
    Armed { slot: Slot, attempts: u8, locked: bool },
    /// The default boot is not Linux: nothing to arm (every Linux boot is a one-shot trial armed from Android, and a
    /// reboot goes there by itself; the shell's `--trial` exited 3 for this).
    NotDefaultLinux,
}

/// The boot control files, resolved under the platform's root.
#[derive(Debug, Clone)]
pub(crate) struct BootCtl {
    pub paths: Paths,
    pub run: PathBuf,
    pub conf: PathBuf,
    pub disk_dirs: Vec<PathBuf>,
    pub cmdline_sources: Vec<PathBuf>,
    pub default_attempts: u8,
    pub lock_wait: Duration,
}

/// The pair of blocks for Linux's slot.
#[derive(Debug, Clone)]
pub(crate) struct Pair {
    pub linux: Slot,
    pub android_file: PathBuf,
    pub linux_file: PathBuf,
}

fn nonempty(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.len() > 0)
}

fn suffix_of(p: &Path) -> Option<Slot> {
    Slot::from_suffix(&std::fs::read(p).ok()?)
}

impl BootCtl {
    /// Linux's slot: the initramfs' file; between that and the command line, the slot its Linux block names.
    pub(crate) fn linux_slot(&self) -> Slot {
        slot::published(&self.run)
            .or_else(|| suffix_of(&self.run.join("misc-bc-linux-trial.bin")))
            .or_else(|| slot::from_cmdline(&self.cmdline_sources))
            .unwrap_or(Slot::B)
    }

    pub(crate) fn pair(&self) -> Pair {
        let linux = self.linux_slot();
        let mut android_file = self.run.join("misc-bc-android.bin");
        let mut linux_file = self.run.join("misc-bc-linux-trial.bin");
        // an initramfs from before the slot work publishes only the old names, and only ever for Linux on b
        if linux == Slot::B {
            if !nonempty(&android_file) {
                android_file = self.run.join("misc-bc-slot-a.bin");
            }
            if !nonempty(&linux_file) {
                linux_file = self.run.join("misc-bc-slot-b-trial.bin");
            }
        }
        Pair { linux, android_file, linux_file }
    }

    /// Linux's block names Linux's slot and Android's the other (a missing one is the write's to refuse).
    pub(crate) fn pair_ok(p: &Pair) -> bool {
        (!nonempty(&p.linux_file) || suffix_of(&p.linux_file) == Some(p.linux))
            && (!nonempty(&p.android_file) || suffix_of(&p.android_file) == Some(p.linux.other()))
    }

    /// misc, as the initramfs found it (`misc-dev`), else the by-name link.
    pub(crate) fn misc(&self) -> Option<PathBuf> {
        let dev = match read_word(&self.run.join("misc-dev")) {
            Some(d) => PathBuf::from(d),
            None => std::fs::canonicalize(self.paths.sys("/dev/block/by-name/misc")).ok()?,
        };
        if dev.as_os_str().is_empty() {
            return None;
        }
        Some(if dev.starts_with(self.paths.root()) { dev } else { self.paths.sys(dev) })
    }

    /// `.f50` on the Linux disk.
    pub(crate) fn disk_dir(&self) -> Option<PathBuf> {
        self.disk_dirs.iter().map(|d| self.paths.sys(d.join(".f50"))).find(|d| d.is_dir())
    }

    pub(crate) fn attempts(&self) -> u8 {
        self.disk_dir()
            .and_then(|d| read_word(&d.join("boot-attempts")))
            .filter(|n| matches!(n.as_str(), "1" | "2" | "3" | "4" | "5" | "6"))
            .and_then(|n| n.parse().ok())
            .unwrap_or(self.default_attempts)
    }

    pub(crate) fn lock_wanted(&self) -> bool {
        self.disk_dir().is_some_and(|d| d.join("boot-lock").is_file())
    }

    pub(crate) fn default_boot(&self) -> Option<String> {
        read_word(&self.conf)
    }

    fn default_is_linux(&self) -> bool {
        self.default_boot().as_deref() == Some("linux")
    }

    /// The initramfs counts every boot in `.f50/boot-tries`; a good boot clears it.
    fn clear_boot_tries(&self) {
        if let Some(d) = self.disk_dir()
            && let Ok(mut f) = File::create(d.join("boot-tries"))
        {
            use std::io::Write;
            let _ = f.write_all(b"0\n");
            let _ = f.sync_all();
        }
    }

    /// One change of misc at a time: `next-boot.lock` in the runtime directory, as the shell's `flock -n 9`. Without
    /// the directory, or with a link in the lock's place, nothing is serialised (as before).
    async fn take_lock(&self) -> Result<Option<File>> {
        let f = self.run.join("next-boot.lock");
        if !self.run.is_dir() || std::fs::symlink_metadata(&f).is_ok_and(|m| m.file_type().is_symlink()) {
            return Ok(None);
        }
        let Ok(file) = OpenOptions::new().append(true).create(true).custom_flags(libc::O_NOFOLLOW).open(&f) else {
            return Ok(None);
        };
        let t0 = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(file)),
                Err(TryLockError::WouldBlock) if t0.elapsed() < self.lock_wait => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(TryLockError::WouldBlock) => {
                    return Err(Error::busy("another change of the boot control is still running"));
                }
                Err(TryLockError::Error(_)) => return Ok(None),
            }
        }
    }

    fn checked_pair(&self) -> Result<Pair> {
        let p = self.pair();
        if Self::pair_ok(&p) {
            Ok(p)
        } else {
            Err(Error::failed(format!(
                "the boot control blocks do not match Linux on slot {}; boot control unchanged",
                p.linux.letter()
            )))
        }
    }

    fn write(&self, b: &Block) -> Result<()> {
        let misc = self.misc().ok_or_else(|| Error::failed("writing misc failed: no misc partition"))?;
        misc::write_misc(&misc, b).map_err(Error::failed)
    }

    /// `arm_linux [trial]`: locked when that is wanted, unless this is the trial of a new boot image.
    fn arm(&self, p: &Pair, trial: bool) -> Result<ArmOutcome> {
        let n = self.attempts();
        let locked = self.lock_wanted() && !trial;
        // a new arm starts a new count: a count left from before would let the initramfs' backstop send an early boot
        // of this arm to Android, before the N failed boots it now promises
        self.clear_boot_tries();
        let Some(base) = misc::read_file(&p.linux_file) else {
            return Err(Error::failed(format!(
                "no boot control block {} (initramfs too old for this slot?)",
                p.linux_file.display()
            )));
        };
        if misc::with_slot(&base, p.linux, 2, false) != Some(base) {
            // the arithmetic is wrong for this misc: the initramfs' own one-shot block, and a failure, so a caller
            // promised N attempts is not promised one in silence (FINDINGS 32)
            self.write(&base)?;
            return Err(Error::failed(format!(
                "cannot rebuild the slot {} block on this system; armed with 1 attempt, not {n}{}",
                p.linux.letter(),
                if locked { " (and not locked)" } else { "" }
            )));
        }
        let armed = misc::with_slot(&base, p.linux, n + 1, locked)
            .ok_or_else(|| Error::failed("boot attempts out of range"))?;
        self.write(&armed)?;
        Ok(ArmOutcome::Armed { slot: p.linux, attempts: n, locked })
    }

    /// `--rearm`: the boot got far enough to count as good.
    pub(crate) async fn rearm(&self) -> Result<ArmOutcome> {
        let _lock = self.take_lock().await?;
        let p = self.checked_pair()?;
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.clear_boot_tries();
            if this.default_is_linux() { this.arm(&p, false) } else { Ok(ArmOutcome::NotDefaultLinux) }
        })
        .await
        .map_err(|e| Error::failed(format!("rearm: {e}")))?
    }

    /// `--trial`: before a new boot image is written, the next boots are the attempts, locked or not.
    pub(crate) async fn trial(&self) -> Result<ArmOutcome> {
        let _lock = self.take_lock().await?;
        let p = self.checked_pair()?;
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            if this.default_is_linux() { this.arm(&p, true) } else { Ok(ArmOutcome::NotDefaultLinux) }
        })
        .await
        .map_err(|e| Error::failed(format!("trial: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::super::misc::tests::{LIVE, hex, with_slots};
    use super::*;

    struct Fx {
        root: PathBuf,
        bc: BootCtl,
    }

    fn fixture(name: &str, linux: Slot) -> Fx {
        let root = std::env::temp_dir().join(format!("f50-hw-bc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let run = root.join("run/f50");
        std::fs::create_dir_all(&run).expect("run");
        std::fs::create_dir_all(root.join("etc/f50")).expect("etc");
        std::fs::create_dir_all(root.join("mnt/f50-disk/.f50")).expect("disk");
        let mut img = vec![0u8; 0x800];
        img.extend_from_slice(&hex(LIVE));
        img.extend_from_slice(&[0u8; 2016]);
        std::fs::write(root.join("misc"), &img).expect("misc");
        std::fs::write(run.join("misc-dev"), format!("{}\n", root.join("misc").display())).expect("misc-dev");
        std::fs::write(run.join("linux-slot"), format!("{}\n", linux.letter())).expect("slot");
        let (android, trial) = match linux {
            Slot::B => (with_slots(b"_a", 0x9f, 0x1e), with_slots(b"_b", 0x9e, 0x2f)),
            Slot::A => (with_slots(b"_b", 0x1e, 0x9f), with_slots(b"_a", 0x2f, 0x9e)),
        };
        std::fs::write(run.join("misc-bc-android.bin"), android).expect("a");
        std::fs::write(run.join("misc-bc-linux-trial.bin"), trial).expect("b");
        std::fs::write(root.join("etc/f50/default-boot"), "linux\n").expect("conf");
        let paths = Paths::new(&root);
        let bc = BootCtl {
            run,
            conf: paths.sys("/etc/f50/default-boot"),
            disk_dirs: vec!["/mnt/f50-disk".into(), "/".into()],
            cmdline_sources: vec![root.join("cmdline")],
            default_attempts: 5,
            lock_wait: Duration::from_secs(1),
            paths,
        };
        Fx { root, bc }
    }

    fn bc_now(f: &Fx) -> Block {
        misc::read_misc(&f.root.join("misc")).unwrap_or([0; 32])
    }

    #[tokio::test]
    async fn slot_a_rearm_is_tries_six_in_byte_twelve() {
        let f = fixture("a", Slot::A);
        assert_eq!(f.bc.rearm().await.ok(), Some(ArmOutcome::Armed { slot: Slot::A, attempts: 5, locked: false }));
        let want = misc::with_slot(&with_slots(b"_a", 0x2f, 0x9e), Slot::A, 6, false);
        assert_eq!(Some(bc_now(&f)), want);
        assert_eq!(bc_now(&f)[12], 0x6f);
        let _ = std::fs::remove_dir_all(&f.root);
    }

    #[tokio::test]
    async fn locked_and_trial_and_tries_cleared() {
        let f = fixture("lock", Slot::B);
        let disk = f.root.join("mnt/f50-disk/.f50");
        std::fs::write(disk.join("boot-lock"), "").expect("lock");
        std::fs::write(disk.join("boot-tries"), "3\n").expect("tries");
        f.bc.rearm().await.ok();
        assert_eq!(bc_now(&f)[14], 0xef);
        assert_eq!(std::fs::read_to_string(disk.join("boot-tries")).ok().as_deref(), Some("0\n"));
        assert_eq!(f.bc.trial().await.ok(), Some(ArmOutcome::Armed { slot: Slot::B, attempts: 5, locked: false }));
        assert_eq!(bc_now(&f)[14], 0x6f);
        std::fs::write(disk.join("boot-attempts"), "2\n").expect("n");
        f.bc.rearm().await.ok();
        assert_eq!(bc_now(&f)[14], 0xbf);
        let _ = std::fs::remove_dir_all(&f.root);
    }

    #[tokio::test]
    async fn nothing_armed_unless_linux_is_the_default() {
        let f = fixture("android", Slot::B);
        std::fs::write(f.root.join("etc/f50/default-boot"), "android\n").expect("conf");
        let before = bc_now(&f);
        assert_eq!(f.bc.rearm().await.ok(), Some(ArmOutcome::NotDefaultLinux));
        assert_eq!(f.bc.trial().await.ok(), Some(ArmOutcome::NotDefaultLinux));
        assert_eq!(bc_now(&f), before);
        let _ = std::fs::remove_dir_all(&f.root);
    }

    #[tokio::test]
    async fn blocks_of_another_slot_are_never_written() {
        let f = fixture("mismatch", Slot::A);
        // the old names, which are only ever Linux-on-b's, with Linux on a: refused, misc unchanged
        std::fs::remove_file(f.bc.run.join("misc-bc-linux-trial.bin")).expect("rm");
        std::fs::write(f.bc.run.join("misc-bc-linux-trial.bin"), with_slots(b"_b", 0x9e, 0x2f)).expect("w");
        let before = bc_now(&f);
        assert!(f.bc.rearm().await.is_err());
        assert_eq!(bc_now(&f), before);
        let _ = std::fs::remove_dir_all(&f.root);
    }

    #[tokio::test]
    async fn a_block_that_cannot_be_rebuilt_is_one_boot_and_a_failure() {
        let f = fixture("degraded", Slot::B);
        let mut odd = with_slots(b"_b", 0x9e, 0x2e); // priority 14: not what the arithmetic makes
        let crc = misc::crc32(&odd[..28]);
        odd[28..].copy_from_slice(&crc.to_le_bytes());
        std::fs::write(f.bc.run.join("misc-bc-linux-trial.bin"), odd).expect("w");
        let e = f.bc.rearm().await.err().map(|e| e.message().to_owned()).unwrap_or_default();
        assert!(e.contains("armed with 1 attempt, not 5"), "{e}");
        assert_eq!(bc_now(&f), odd);
        let _ = std::fs::remove_dir_all(&f.root);
    }
}
