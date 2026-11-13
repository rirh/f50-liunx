//! Boot control: what keeps the device booting Linux instead of falling back to Android (`mu300-next-boot`).
//!
//! In default-Linux mode (`/etc/f50/default-boot` says `linux`) the Linux slot is armed with `tries = N + 1`,
//! `successful = 0`: LK counts down on every boot and rolls back to Android's slot when it finds `tries == 1`, so N
//! boots in a row that never reach [`crate::Hw::mark_boot_good`] land in Android on the next one (FINDINGS 1, 32). A
//! good boot arms N + 1 again. N is `.f50/boot-attempts` on the Linux disk (1-6, else 5), shared with the
//! initramfs, which counts boots in `.f50/boot-tries` as its own backstop and is cleared here. "Locked"
//! (`.f50/boot-lock`) arms the slot `successful = 1`: LK never counts it down. A new boot image is armed as a trial
//! (the attempts, not the lock) until its first good boot.
//!
//! The blocks are never made from scratch: the initramfs publishes this slot's pair (`misc-bc-android.bin`,
//! `misc-bc-linux-trial.bin`, the older `misc-bc-slot-a.bin`/`misc-bc-slot-b-trial.bin` only for Linux on b), and the
//! armed block is the trial block with one byte and the CRC changed - only after rebuilding the trial block itself
//! reproduces it byte for byte. Otherwise the initramfs' own one-shot block goes in (one boot, not N; FINDINGS 32) and
//! the arm reports failure. A block that names another slot than the one it is for is never written (kanoqwq's
//! b8d4f222). One change of misc at a time (the lock file the shell used, so an old `mu300-next-boot` and f50d exclude
//! each other).
//!
//! Switching the OS (`linux`, `android`, `lock`, `unlock`, `attempts N`) was removed from the API and is not ported;
//! what they leave on the disk is honoured.

pub(crate) mod mark;
pub(crate) mod misc;
pub(crate) mod state;

pub use mark::ArmOutcome;
pub use misc::SlotInfo;
pub use state::BootInfo;
