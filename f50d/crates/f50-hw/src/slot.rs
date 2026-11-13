//! Which A/B slot Linux runs from, and so which one is Android's.
//!
//! After an OTA Android often runs from b and the installer then puts Linux on a (FINDINGS 1, "Update 2026-10-07").
//! The initramfs decides and publishes it (`/run/f50/linux-slot`); without that file (an older boot image, or a
//! garbled one) the slot LK booted is on the kernel command line, or - the mainline kernel replaces that line - in
//! the bootargs LK put in the device tree. b only when nothing says.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// An A/B slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn letter(self) -> char {
        match self {
            Slot::A => 'a',
            Slot::B => 'b',
        }
    }

    pub fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    /// `_a` / `_b`, as the first bytes of a boot control block name the slot.
    pub fn suffix(self) -> &'static [u8; 2] {
        match self {
            Slot::A => b"_a",
            Slot::B => b"_b",
        }
    }

    pub(crate) fn from_word(s: &str) -> Option<Slot> {
        match s {
            "a" => Some(Slot::A),
            "b" => Some(Slot::B),
            _ => None,
        }
    }

    pub(crate) fn from_suffix(b: &[u8]) -> Option<Slot> {
        match b.get(..2) {
            Some(b"_a") => Some(Slot::A),
            Some(b"_b") => Some(Slot::B),
            _ => None,
        }
    }
}

/// `$(cat file)`: the contents without trailing newlines.
pub(crate) fn read_word(p: &Path) -> Option<String> {
    let s = std::fs::read_to_string(p).ok()?;
    Some(s.trim_end_matches('\n').to_owned())
}

/// The slot the initramfs published (`linux-slot`: exactly `a` or `b`).
pub(crate) fn published(run: &Path) -> Option<Slot> {
    Slot::from_word(&read_word(&run.join("linux-slot"))?)
}

/// `androidboot.slot_suffix=_X` of a command line, as `sed 's/.*androidboot\.slot_suffix=_\([ab]\).*/\1/p'` finds it
/// (NULs as spaces; per line, the last occurrence; the first line that has one).
pub(crate) fn slot_in_cmdline(text: &[u8]) -> Option<Slot> {
    const KEY: &[u8] = b"androidboot.slot_suffix=_";
    let text: Vec<u8> = text.iter().map(|&b| if b == 0 { b' ' } else { b }).collect();
    text.split(|&b| b == b'\n').find_map(|line| {
        let mut found = None;
        let mut i = 0;
        while i + KEY.len() < line.len() {
            if &line[i..i + KEY.len()] == KEY {
                match line[i + KEY.len()] {
                    b'a' => found = Some(Slot::A),
                    b'b' => found = Some(Slot::B),
                    _ => {}
                }
            }
            i += 1;
        }
        found
    })
}

/// The slot LK booted, from the first readable source that names one.
pub(crate) fn from_cmdline(sources: &[PathBuf]) -> Option<Slot> {
    sources.iter().filter_map(|f| std::fs::read(f).ok()).find_map(|t| slot_in_cmdline(&t))
}

/// Linux's slot: the published one, else the command line's, else b (early-recorder, android-vendor-start).
pub(crate) fn linux_slot(run: &Path, sources: &[PathBuf]) -> Slot {
    published(run).or_else(|| from_cmdline(sources)).unwrap_or(Slot::B)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_names_the_slot() {
        assert_eq!(slot_in_cmdline(b"console=x androidboot.slot_suffix=_a quiet\n"), Some(Slot::A));
        assert_eq!(slot_in_cmdline(b"androidboot.slot_suffix=_a androidboot.slot_suffix=_b"), Some(Slot::B));
        assert_eq!(slot_in_cmdline(b"bootargs\0androidboot.slot_suffix=_b\0"), Some(Slot::B));
        assert_eq!(slot_in_cmdline(b"androidboot.slot_suffix=_c"), None);
        assert_eq!(slot_in_cmdline(b""), None);
    }
}
