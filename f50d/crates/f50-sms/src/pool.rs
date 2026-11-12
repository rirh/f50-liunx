//! The SMS pool: every received message, and every one sent from here, on flash, so a reader never waits for the
//! modem.
//!
//! The SIM is where a received message is safe until the pool has it. Whatever comes in (read off the SIM, or
//! delivered directly with `+CMT`) is first *parked* as its PDU, durably, and only then deleted from the SIM or
//! acknowledged to the network (docs/research/FINDINGS.md 39, 51). Parked parts are then *absorbed*: decoded,
//! grouped into messages (`group`), deduplicated by fingerprint and tombstone, and pooled; the parts of a long
//! message wait for each other on the disk, and are pooled incomplete after the partial wait.
//!
//! The state lives in memory behind a std mutex, written through to the files of `store`. Every operation is
//! synchronous and short; async callers go through [`Pool::with`], which runs it on the blocking pool so flash
//! latency never stalls the runtime and the lock is never held across an `.await`.

mod absorb;
mod migrate;
mod store;
mod view;

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use f50_core::{Error, Result};

pub(crate) use absorb::{Absorbed, fingerprint, sim_messages};
pub use store::{Dir, Status};
use store::{Dirs, TOMBSTONES_MAX};
pub(crate) use store::{Message, Parked};
pub(crate) use view::{MessageView, Page};

use crate::pdu::{Coding, Decoded, decode};

/// The pool's in-memory state, written through to its files.
#[derive(Debug)]
pub(crate) struct State {
    dirs: Dirs,
    msgs: BTreeMap<u64, Message>,
    parts: Vec<Parked>,
    tombs: VecDeque<String>,
    next_id: u64,
    part_seq: u64,
    /// Seconds the parts of a long message wait for their siblings.
    partial_wait: u64,
}

/// The pool. Cheap to clone.
#[derive(Debug, Clone)]
pub(crate) struct Pool {
    state: Arc<Mutex<State>>,
}

impl Pool {
    /// Opens (creates) the pool in `dir`; on the first start the shell pool at `legacy` is copied in, if it exists.
    pub fn open(dir: PathBuf, legacy: &Path, partial_wait: u64) -> Result<Pool> {
        let dirs = Dirs::new(dir);
        dirs.ensure()?;
        let next = match dirs.read_next_id()? {
            Some(n) => n,
            None => {
                let n = migrate::from_shell_pool(&dirs, legacy)?;
                dirs.write_next_id(n)?;
                n
            }
        };
        let msgs: BTreeMap<u64, Message> = dirs.load_messages().into_iter().map(|m| (m.id, m)).collect();
        // the highest id in the pool counts too, for an id file that went back some other way
        let next_id = next.max(msgs.keys().next_back().copied().unwrap_or(0));
        let parts = dirs.load_parts();
        let tombs = dirs.load_tombstones()?;
        let state = State { dirs, msgs, parts, tombs, next_id, part_seq: 0, partial_wait };
        Ok(Pool { state: Arc::new(Mutex::new(state)) })
    }

    /// Runs `f` on the state, on the blocking pool.
    pub async fn with<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut State) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let mut st = state.lock().unwrap_or_else(PoisonError::into_inner);
            f(&mut st)
        })
        .await
        .map_err(|e| Error::failed(format!("the SMS pool task failed: {e}")))?
    }
}

/// A message to pool, before it has an id.
#[derive(Debug, Clone)]
pub(crate) struct NewMessage {
    pub dir: Dir,
    pub status: Status,
    pub peer: String,
    pub time: String,
    pub utc_offset: Option<i16>,
    pub coding: Coding,
    pub parts: u8,
    pub incomplete: Option<(u8, u8)>,
    pub partial: Option<String>,
    pub part_ids: Vec<String>,
    pub text: String,
    pub fingerprint: String,
}

impl State {
    /// Messages into the pool: each file on the disk, then the directory entries and next_id. Returns them with
    /// their ids.
    pub fn store(&mut self, new: Vec<NewMessage>, now: u64) -> Result<Vec<Message>> {
        let mut out = Vec::with_capacity(new.len());
        for n in new {
            let id = self.next_id + 1;
            let m = Message {
                id,
                dir: n.dir,
                status: n.status,
                peer: n.peer,
                time: n.time,
                utc_offset: n.utc_offset,
                received: now,
                coding: n.coding,
                fingerprint: n.fingerprint,
                parts: n.parts,
                incomplete: n.incomplete,
                partial: n.partial,
                part_ids: n.part_ids,
                text: n.text,
            };
            self.dirs.write_message(&m)?;
            self.next_id = id;
            self.msgs.insert(id, m.clone());
            out.push(m);
        }
        if !out.is_empty() {
            self.dirs.sync_messages()?;
            self.dirs.write_next_id(self.next_id)?;
        }
        Ok(out)
    }

    /// Parks a received PDU on the disk (file and directory entry). True when the pool now holds it, also when the
    /// same PDU was parked already; false for a PDU that is no SMS-DELIVER (it is not parked: it would wait for ever).
    pub fn park(&mut self, pdu: &str, unread: bool, origin: &str, now: u64) -> Result<bool> {
        let pdu = pdu.trim().to_ascii_uppercase();
        if !matches!(decode(&pdu), Some(Decoded::Deliver(_))) {
            return Ok(false);
        }
        if self.parts.iter().any(|p| p.pdu == pdu) {
            return Ok(true);
        }
        self.part_seq += 1;
        let p = Parked {
            name: format!("p{now}-{}", self.part_seq),
            pdu,
            unread,
            first_seen: now,
            origin: origin.to_owned(),
        };
        self.dirs.write_part(&p)?;
        self.dirs.sync_parts()?;
        self.parts.push(p);
        Ok(true)
    }

    pub fn get(&self, id: u64) -> Result<&Message> {
        self.msgs.get(&id).ok_or_else(|| Error::not_found("message"))
    }

    /// Marks a received message read (or unread); a sent one is left as it is. Written only when it changes.
    pub fn mark(&mut self, id: u64, read: bool) -> Result<Message> {
        let m = self.msgs.get_mut(&id).ok_or_else(|| Error::not_found("message"))?;
        let want = if read { Status::Read } else { Status::Unread };
        if m.dir == Dir::In && m.status != want {
            let mut changed = m.clone();
            changed.status = want;
            self.dirs.write_message(&changed)?;
            *m = changed;
        }
        Ok(m.clone())
    }

    /// Deletes one message (`Some(id)`) or all; each is remembered by its fingerprint and its parts, so the SIM or
    /// the network cannot bring it back. Returns the messages deleted.
    pub fn delete(&mut self, id: Option<u64>) -> Result<Vec<Message>> {
        let ids: Vec<u64> = match id {
            Some(i) => {
                self.get(i)?;
                vec![i]
            }
            None => self.msgs.keys().copied().collect(),
        };
        let gone: Vec<Message> = ids.iter().filter_map(|i| self.msgs.get(i).cloned()).collect();
        // the tombstones first: a message deleted with its tombstone lost would come back
        for m in &gone {
            self.tombs.push_back(m.fingerprint.clone());
            self.tombs.extend(m.part_ids.iter().cloned());
        }
        while self.tombs.len() > TOMBSTONES_MAX {
            self.tombs.pop_front();
        }
        if !gone.is_empty() {
            self.dirs.write_tombstones(&self.tombs)?;
        }
        for m in &gone {
            self.dirs.remove_message(m.id)?;
            self.msgs.remove(&m.id);
        }
        Ok(gone)
    }

    /// Messages in the pool, and the received ones not read yet.
    pub fn counts(&self) -> (usize, usize) {
        let unread = self.msgs.values().filter(|m| m.dir == Dir::In && m.status == Status::Unread).count();
        (self.msgs.len(), unread)
    }

    pub(crate) fn messages(&self) -> impl DoubleEndedIterator<Item = &Message> {
        self.msgs.values()
    }
}

#[cfg(test)]
pub(crate) fn test_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("f50-sms-pool-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(text: &str) -> NewMessage {
        NewMessage {
            dir: Dir::Out,
            status: Status::Sent,
            peer: "10086".into(),
            time: "2026-10-03 11:00:00".into(),
            utc_offset: Some(0),
            coding: Coding::Gsm,
            parts: 1,
            incomplete: None,
            partial: None,
            part_ids: vec![],
            text: text.into(),
            fingerprint: fingerprint("10086", "2026-10-03 11:00:00", text),
        }
    }

    #[test]
    fn ids_are_stable_and_never_reused() {
        let dir = test_dir("ids");
        let pool = Pool::open(dir.clone(), &dir.join("none"), 3600).expect("open");
        let mut st = pool.state.lock().expect("lock");
        let a = st.store(vec![sent("a"), sent("b")], 1).expect("store");
        assert_eq!(a.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 2]);
        st.delete(Some(2)).expect("delete");
        assert_eq!(st.store(vec![sent("c")], 2).expect("store")[0].id, 3);
        assert_eq!(st.delete(Some(9)).map(|_| ()).expect_err("none").code(), f50_core::Code::NotFound);
        drop(st);
        // a reopened pool reads the same messages and continues the ids
        let pool = Pool::open(dir.clone(), &dir.join("none"), 3600).expect("reopen");
        let mut st = pool.state.lock().expect("lock");
        assert_eq!(st.counts(), (2, 0));
        assert_eq!(st.store(vec![sent("d")], 3).expect("store")[0].id, 4);
        assert_eq!(st.delete(None).expect("all").len(), 3);
        assert_eq!(st.counts(), (0, 0));
        drop(st);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn park_refuses_what_is_no_message_and_keeps_one_copy() {
        let dir = test_dir("park");
        let pool = Pool::open(dir.clone(), &dir.join("none"), 3600).expect("open");
        let mut st = pool.state.lock().expect("lock");
        let pdu = "07911326040000F0040B911346610089F60000208062917314480CC8F71D14969741F977FD07";
        assert!(st.park(pdu, true, "air", 1).expect("park"));
        assert!(st.park(&pdu.to_lowercase(), true, "sim:SM:3", 2).expect("again"));
        assert_eq!(st.parts.len(), 1);
        assert!(!st.park("0011000B911346610089F60000AA05E8329BFD06", true, "air", 3).expect("submit"));
        assert_eq!(st.dirs.load_parts().len(), 1);
        drop(st);
        let _ = std::fs::remove_dir_all(dir);
    }
}
