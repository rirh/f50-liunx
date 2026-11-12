//! The pool's files on flash: one JSON file per message, the parked parts, `next_id`, the tombstones.
//!
//! Every file is written aside, fsynced and renamed (`fsx::write_atomic`); a new name is made durable by an fsync of
//! its directory, never by a global sync, which flushes the whole flash and can stall for seconds
//! (docs/research/FINDINGS.md 39). Nothing is written unless something changed: a sync of an unchanged SIM writes
//! nothing at all.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use f50_core::{Error, Result, fsx};
use serde::{Deserialize, Serialize};

use crate::pdu::Coding;

/// The tombstones kept (fingerprints and part ids of deleted messages), oldest first out.
pub(crate) const TOMBSTONES_MAX: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unread,
    Read,
    Sent,
}

/// One message in the pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Message {
    /// Stable for the message's life; never reused (next_id only grows).
    pub id: u64,
    pub dir: Dir,
    pub status: Status,
    /// The sender of a received message, the recipient of a sent one.
    pub peer: String,
    /// The service centre's time of a received message (the sender's local time), the send time (UTC) of a sent one;
    /// `YYYY-MM-DD HH:MM:SS`.
    pub time: String,
    #[serde(default)]
    pub utc_offset: Option<i16>,
    /// When the pool took it (Unix seconds).
    pub received: u64,
    pub coding: Coding,
    /// md5 of "peer|time|text" with tabs and newlines as spaces: the identity across the pool and the SIM (SIM
    /// indexes are none, they shift whenever anything is deleted). The shell pool's own sum.
    pub fingerprint: String,
    #[serde(default = "one")]
    pub parts: u8,
    /// Parts that arrived and the total, for a long message pooled without all of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<(u8, u8)>,
    /// md5 of `multi:SENDER:REF:TOTAL` of a message pooled incomplete: a later part of it is pooled at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<String>,
    /// md5 of each part's PDU: a part delivered again is known as one of this message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub part_ids: Vec<String>,
    pub text: String,
}

const fn one() -> u8 {
    1
}

/// A received PDU on the disk until the pool has made a message of it (a part waiting for its siblings, a message
/// read off the SIM or delivered directly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Parked {
    /// The file's stem (`p<epoch>-<seq>`).
    pub name: String,
    /// Uppercase hexadecimal, with the SMSC prefix.
    pub pdu: String,
    /// Whether it was unread where it came from (the SIM's stat; delivered directly: unread).
    pub unread: bool,
    /// When it was parked (Unix seconds): a long message waits from its first part.
    pub first_seen: u64,
    /// "sim:SM:3", "air", "legacy" (for the log only).
    pub origin: String,
}

/// The pool's directory and its files.
#[derive(Debug, Clone)]
pub(crate) struct Dirs {
    pub root: PathBuf,
    msg: PathBuf,
    parts: PathBuf,
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Dirs { msg: root.join("msg"), parts: root.join("parts"), root }
    }

    /// The directories, root's alone (message text is private).
    pub fn ensure(&self) -> Result<()> {
        fsx::private_dir(&self.root)?;
        fsx::private_dir(&self.msg)?;
        fsx::private_dir(&self.parts)
    }

    fn msg_file(&self, id: u64) -> PathBuf {
        self.msg.join(format!("{id:06}.json"))
    }

    pub fn write_message(&self, m: &Message) -> Result<()> {
        let b = serde_json::to_vec(m).map_err(|e| Error::failed(format!("message {}: {e}", m.id)))?;
        fsx::write_atomic(&self.msg_file(m.id), &b, 0o600)
    }

    pub fn remove_message(&self, id: u64) -> Result<()> {
        remove(&self.msg_file(id))
    }

    /// Every message file that reads; one that does not is logged and left where it is.
    pub fn load_messages(&self) -> Vec<Message> {
        regular_files(&self.msg, |n| n.ends_with(".json") && !n.starts_with('.'))
            .into_iter()
            .filter_map(|p| match fs::read(&p).map(|b| serde_json::from_slice::<Message>(&b)) {
                Ok(Ok(m)) => Some(m),
                _ => {
                    tracing::warn!(file = %p.display(), "an SMS pool file does not read; left alone");
                    None
                }
            })
            .collect()
    }

    pub fn write_part(&self, p: &Parked) -> Result<()> {
        let b = serde_json::to_vec(p).map_err(|e| Error::failed(format!("part {}: {e}", p.name)))?;
        fsx::write_atomic(&self.parts.join(format!("{}.json", p.name)), &b, 0o600)
    }

    pub fn remove_part(&self, name: &str) -> Result<()> {
        remove(&self.parts.join(format!("{name}.json")))
    }

    pub fn load_parts(&self) -> Vec<Parked> {
        regular_files(&self.parts, |n| n.starts_with('p') && n.ends_with(".json"))
            .into_iter()
            .filter_map(|p| fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Parked>(&b).ok()))
            .collect()
    }

    /// The new names in the message directory on the disk.
    pub fn sync_messages(&self) -> Result<()> {
        sync_dir(&self.msg)
    }

    pub fn sync_parts(&self) -> Result<()> {
        sync_dir(&self.parts)
    }

    fn next_id_file(&self) -> PathBuf {
        self.root.join("next_id")
    }

    /// `None` before the pool was ever set up (the marker that a migration may run).
    pub fn read_next_id(&self) -> Result<Option<u64>> {
        Ok(fsx::read_opt(&self.next_id_file())?.map(|s| s.trim().parse().unwrap_or(0)))
    }

    /// Replaced whole: an id file a power cut left empty would start the ids over (FINDINGS 39).
    pub fn write_next_id(&self, n: u64) -> Result<()> {
        fsx::write_atomic(&self.next_id_file(), format!("{n}\n").as_bytes(), 0o600)
    }

    pub fn load_tombstones(&self) -> Result<VecDeque<String>> {
        Ok(fsx::read_opt(&self.root.join("deleted"))?
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn write_tombstones(&self, t: &VecDeque<String>) -> Result<()> {
        let mut s = String::new();
        for l in t {
            s.push_str(l);
            s.push('\n');
        }
        fsx::write_atomic(&self.root.join("deleted"), s.as_bytes(), 0o600)
    }
}

fn remove(p: &Path) -> Result<()> {
    match fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(format!("remove {}", p.display()), &e)),
    }
}

/// The regular files of `dir` (never a link) whose names pass `want`, sorted by name.
pub(crate) fn regular_files(dir: &Path, want: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(&want))
        .filter(|e| fs::symlink_metadata(e.path()).is_ok_and(|m| m.is_file()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// A directory's entries on the disk (an fsync of the directory alone).
pub(crate) fn sync_dir(dir: &Path) -> Result<()> {
    fs::File::open(dir).and_then(|f| f.sync_all()).map_err(|e| Error::io(format!("sync {}", dir.display()), &e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f50-sms-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn files_round_trip() {
        let d = Dirs::new(tmp("store"));
        d.ensure().expect("dirs");
        assert_eq!(d.read_next_id().expect("read"), None);
        d.write_next_id(7).expect("write");
        assert_eq!(d.read_next_id().expect("read"), Some(7));
        let m = Message {
            id: 3,
            dir: Dir::In,
            status: Status::Unread,
            peer: "ADANA BLD".into(),
            time: "2026-09-15 17:18:00".into(),
            utc_offset: Some(180),
            received: 1,
            coding: Coding::Ucs2,
            fingerprint: "f".into(),
            parts: 1,
            incomplete: Some((1, 2)),
            partial: None,
            part_ids: vec![],
            text: "a\nb".into(),
        };
        d.write_message(&m).expect("write");
        assert_eq!(d.load_messages(), vec![m]);
        d.remove_message(3).expect("remove");
        d.remove_message(3).expect("a missing one is fine");
        assert!(d.load_messages().is_empty());
        let mut t = VecDeque::new();
        t.push_back("x".to_owned());
        d.write_tombstones(&t).expect("write");
        assert_eq!(d.load_tombstones().expect("read"), t);
        let _ = fs::remove_dir_all(&d.root);
    }
}
