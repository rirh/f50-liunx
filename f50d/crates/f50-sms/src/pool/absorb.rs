//! Parked parts into messages: decoded, grouped, deduplicated, pooled, and the parts let go.
//!
//! A long message waits for its parts on the disk. Parts whose siblings never come (a part the network lost) would
//! wait for ever: after the partial wait (three minutes) the parts that did arrive are pooled as one message, "[...]" where
//! a part is missing, marked incomplete. Late parts can repair a uniquely matching message in place. A part delivered
//! once more (an acknowledgement that did not reach the network) is known by its PDU and dropped (FINDINGS 39, 63).

use std::collections::{BTreeMap, HashSet};

use f50_core::Result;

use super::store::{Dir, Message, Parked, Status};
use super::{NewMessage, State};
use crate::group::{GROUP_WINDOW, Rec, group, ts_near, ts_secs};
use crate::md5;
use crate::pdu::{Coding, Decoded, Deliver, decode};

/// The identity of a message: md5 of "peer|time|text", tabs and newlines as spaces and CRs dropped, as the shell
/// pool computed it from its codec's records (so a migrated pool's fingerprints and tombstones still match).
pub(crate) fn fingerprint(peer: &str, time: &str, text: &str) -> String {
    let flat = |s: &str| s.replace(['\t', '\n'], " ").replace('\r', "");
    md5::hex(format!("{}|{}|{}", flat(peer), time, flat(text)).as_bytes())
}

/// The id of one part: md5 of its PDU in uppercase hexadecimal.
fn part_id(pdu: &str) -> String {
    md5::hex(pdu.to_ascii_uppercase().as_bytes())
}

/// What one absorb did.
#[derive(Debug, Default)]
pub(crate) struct Absorbed {
    /// The received messages it pooled (for the hook).
    pub new: Vec<Message>,
    /// Long messages still waiting for parts.
    pub waiting: usize,
}

/// The parts of one group put together.
#[derive(Debug)]
struct Assembled {
    peer: String,
    time: String,
    utc_offset: Option<i16>,
    coding: Coding,
    total: u8,
    fragments: BTreeMap<u8, String>,
    /// The text with "[...]" for each missing part (the whole text when none is missing).
    text: String,
    unread: bool,
    first_seen: u64,
    part_ids: Vec<String>,
    base: Option<String>,
    members: Vec<usize>,
}

impl Assembled {
    fn have(&self) -> u8 {
        u8::try_from(self.fragments.len()).unwrap_or(u8::MAX)
    }

    fn complete(&self) -> bool {
        self.have() == self.total
    }
    fn fingerprint(&self) -> String {
        fingerprint(&self.peer, &self.time, &self.text)
    }
}

fn joined_text(total: u8, fragments: &BTreeMap<u8, String>) -> String {
    (1..=total).map(|n| fragments.get(&n).map(String::as_str).unwrap_or("[...]")).collect()
}

/// Groups decoded parts and assembles each group. `meta[i]` is (unread, first seen, pdu) of `parts[i]`.
fn assemble(parts: &[Deliver], meta: &[(bool, u64, &str)]) -> Vec<Assembled> {
    let recs: Vec<Rec<'_>> = parts
        .iter()
        .map(|d| Rec { sender: &d.sender, ts: &d.timestamp.local, concat: d.concat, text: &d.text })
        .collect();
    group(&recs)
        .into_iter()
        .filter_map(|g| {
            let first = &parts[*g.members.first()?];
            // the first part dates the message
            let dating = g.members.iter().map(|&i| &parts[i]).find(|d| d.concat.is_some_and(|c| c.part == 1));
            let dating = dating.unwrap_or(first);
            let mut fragments = BTreeMap::new();
            for &i in &g.members {
                fragments.entry(parts[i].concat.map_or(1, |c| c.part)).or_insert_with(|| parts[i].text.clone());
            }
            let text = joined_text(g.total, &fragments);
            let coding =
                if g.members.iter().any(|&i| parts[i].coding == Coding::Ucs2) { Coding::Ucs2 } else { first.coding };
            Some(Assembled {
                peer: first.sender.clone(),
                time: dating.timestamp.local.clone(),
                utc_offset: dating.timestamp.offset_min,
                coding,
                total: g.total,
                fragments,
                text,
                unread: g.members.iter().any(|&i| meta[i].0),
                first_seen: g.members.iter().map(|&i| meta[i].1).min().unwrap_or(0),
                part_ids: if g.base.is_some() {
                    g.members.iter().map(|&i| part_id(meta[i].2)).collect()
                } else {
                    vec![]
                },
                base: g.base,
                members: g.members,
            })
        })
        .collect()
}

/// The messages among SIM slots (`(slot, pdu)`): each one's fingerprint and its slots. For `delete --sim`.
pub(crate) fn sim_messages(slots: &[(u32, String)]) -> Vec<(String, Vec<u32>)> {
    let mut decoded = Vec::new();
    let mut idx = Vec::new();
    for (slot, pdu) in slots {
        if let Some(Decoded::Deliver(d)) = decode(pdu) {
            decoded.push(d);
            idx.push((*slot, pdu.as_str()));
        }
    }
    let meta: Vec<(bool, u64, &str)> = idx.iter().map(|(_, p)| (false, 0, *p)).collect();
    assemble(&decoded, &meta)
        .into_iter()
        .map(|a| (a.fingerprint(), a.members.iter().map(|&i| idx[i].0).collect()))
        .collect()
}

impl State {
    /// Build a repair only when the late parts identify a single incomplete message.
    fn repair(&self, incoming: &Assembled, key: &str, decoded: &[Deliver]) -> Option<Message> {
        let times = incoming
            .members
            .iter()
            .map(|&i| {
                let t = &decoded[i].timestamp;
                Some((ts_secs(&t.local)?, t.offset_min))
            })
            .collect::<Option<Vec<_>>>()?;
        let candidates: Vec<_> = self
            .msgs
            .values()
            .filter(|m| {
                if m.dir != Dir::In
                    || m.peer != incoming.peer
                    || m.partial.as_deref() != Some(key)
                    || !m.incomplete.is_some_and(|(_, total)| total == incoming.total)
                {
                    return false;
                }
                let Some(at) = ts_secs(&m.time) else { return false };
                if !times.iter().all(|&(t, offset)| {
                    let shift = offset.zip(m.utc_offset).map_or(0, |(new, old)| (i64::from(new) - i64::from(old)) * 60);
                    (t - at - shift).abs() <= GROUP_WINDOW
                }) {
                    return false;
                }
                if m.fragments.is_empty() {
                    // Legacy rows have placeholder text but no trustworthy fragment boundaries.
                    incoming.complete()
                        && !m.part_ids.is_empty()
                        && m.part_ids.iter().all(|p| incoming.part_ids.contains(p))
                } else {
                    // A new complete message with a reused reference can have the same surrounding text.
                    (!incoming.complete() || m.part_ids.iter().any(|p| incoming.part_ids.contains(p)))
                        && incoming.fragments.iter().all(|(n, text)| m.fragments.get(n).is_none_or(|old| old == text))
                }
            })
            .collect();
        let mut overlapping = candidates
            .iter()
            .copied()
            .filter(|m| m.part_ids.iter().any(|p| incoming.part_ids.contains(p)));
        let old = match (overlapping.next(), overlapping.next(), candidates.as_slice()) {
            (Some(m), None, _) => m,
            (None, None, [m]) => *m,
            _ => return None,
        };
        if incoming.fragments.keys().all(|n| old.fragments.contains_key(n)) {
            return None;
        }

        let mut repaired = (*old).clone();
        if !repaired.fragments.contains_key(&1) && incoming.fragments.contains_key(&1) {
            repaired.time = incoming.time.clone();
            repaired.utc_offset = incoming.utc_offset;
        }
        repaired.fragments.extend(incoming.fragments.iter().map(|(&n, text)| (n, text.clone())));
        let have = u8::try_from(repaired.fragments.len()).ok()?;
        repaired.text = joined_text(incoming.total, &repaired.fragments);
        repaired.fingerprint = fingerprint(&repaired.peer, &repaired.time, &repaired.text);
        if incoming.coding == Coding::Ucs2 {
            repaired.coding = Coding::Ucs2;
        }
        for id in &incoming.part_ids {
            if !repaired.part_ids.contains(id) {
                repaired.part_ids.push(id.clone());
            }
        }
        repaired.incomplete = (have < incoming.total).then_some((have, incoming.total));
        if repaired.incomplete.is_none() {
            repaired.partial = None;
            repaired.fragments.clear();
        }
        Some(repaired)
    }

    /// Every parked part into the pool that can go there now; the parts the pool took, or knew, are let go once the
    /// new and repaired messages are on the disk.
    pub fn absorb(&mut self, now: u64) -> Result<Absorbed> {
        self.parts.sort_by(|a, b| (a.first_seen, &a.name).cmp(&(b.first_seen, &b.name)));
        let mut decoded = Vec::new();
        let mut owners: Vec<&Parked> = Vec::new();
        let mut undecodable = Vec::new();
        for p in &self.parts {
            match decode(&p.pdu) {
                Some(Decoded::Deliver(d)) => {
                    decoded.push(d);
                    owners.push(p);
                }
                _ => undecodable.push(p.name.clone()),
            }
        }
        let meta: Vec<(bool, u64, &str)> = owners.iter().map(|p| (p.unread, p.first_seen, p.pdu.as_str())).collect();
        let groups = assemble(&decoded, &meta);

        let mut known: HashSet<String> = self.msgs.values().map(|m| m.fingerprint.clone()).collect();
        let dead: HashSet<&str> = self.tombs.iter().map(String::as_str).collect();
        let mut pooled_parts: HashSet<String> = self
            .msgs
            .values()
            .flat_map(|m| m.part_ids.iter().cloned())
            .chain(self.tombs.iter().cloned())
            .collect();
        let mut new = Vec::new();
        let mut done: Vec<String> = undecodable;
        let mut waiting = 0;
        for a in groups {
            let names = a.members.iter().map(|&i| owners[i].name.clone());
            let fp = a.fingerprint();
            // Exact PDU identities also cover a replay after a repair whose displayed time changed.
            let all_known = !a.part_ids.is_empty() && a.part_ids.iter().all(|p| pooled_parts.contains(p));
            if dead.contains(fp.as_str()) || (!a.complete() && all_known) {
                done.extend(names);
                continue;
            }
            let partial = a.base.as_deref().map(|b| md5::hex(b.as_bytes()));
            if let Some(repaired) = partial.as_deref().and_then(|key| self.repair(&a, key, &decoded)) {
                let original = &self.msgs[&repaired.id];
                if !dead.contains(repaired.fingerprint.as_str())
                    && (repaired.fingerprint == original.fingerprint || !known.contains(&repaired.fingerprint))
                {
                    self.dirs.write_message(&repaired)?;
                    self.dirs.sync_messages()?;
                    known.insert(repaired.fingerprint.clone());
                    pooled_parts.extend(repaired.part_ids.iter().cloned());
                    self.msgs.insert(repaired.id, repaired);
                }
                done.extend(names);
                continue;
            }
            if known.contains(&fp) || all_known {
                done.extend(names);
                continue;
            }
            let mut extra = None;
            if !a.complete() {
                let legacy = partial.as_deref().is_some_and(|key| {
                    self.msgs.values().any(|m| {
                        m.fragments.is_empty() && m.partial.as_deref() == Some(key) && ts_near(&m.time, &a.time)
                    })
                });
                if now.saturating_sub(a.first_seen) < self.partial_wait && !legacy {
                    waiting += 1;
                    continue;
                }
                extra = Some((a.have(), a.total));
            }
            done.extend(names);
            known.insert(fp.clone());
            new.push(NewMessage {
                dir: Dir::In,
                // the read state where it came from carries over: one read before the pool saw it is not new
                status: if a.unread { Status::Unread } else { Status::Read },
                peer: a.peer,
                time: a.time,
                utc_offset: a.utc_offset,
                coding: a.coding,
                parts: a.total,
                incomplete: extra,
                partial: if extra.is_some() { partial } else { None },
                part_ids: a.part_ids,
                fragments: if extra.is_some() { a.fragments } else { BTreeMap::new() },
                text: a.text,
                fingerprint: fp,
            });
        }
        // the messages on the disk first, then the parts they came from may go
        let stored = self.store(new, now)?;
        for m in &stored {
            if let Some((have, total)) = m.incomplete {
                let missing: Vec<_> = (1..=total).filter(|n| !m.fragments.contains_key(n)).collect();
                tracing::info!(have, total, ?missing, "an incomplete SMS was pooled");
            }
        }
        if !done.is_empty() {
            for name in &done {
                self.dirs.remove_part(name)?;
            }
            self.parts.retain(|p| !done.contains(&p.name));
        }
        Ok(Absorbed { new: stored, waiting })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Pool, State, test_dir};
    use super::*;
    use crate::pdu::test_part;

    const PDU_GSM: &str = "07911326040000F0040B911346610089F60000208062917314480CC8F71D14969741F977FD07";
    const PDU_UCS2: &str = "000410D04162D0190409994400086290517181004024004B006100720067006F006E0075007A00200079006F006C006400610020011F00FC015F";

    fn with_pool(tag: &str, wait: u64, f: impl FnOnce(&mut State)) {
        let dir = test_dir(tag);
        let pool = Pool::open(dir.clone(), &dir.join("none"), wait).expect("open");
        f(&mut pool.state.lock().expect("lock"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn fingerprints_match_the_shell_pool() {
        // printf '%s|%s|%s' '+31641600986' '2002-08-26 19:37:41' 'How are you?' | md5sum
        assert_eq!(
            fingerprint("+31641600986", "2002-08-26 19:37:41", "How are you?"),
            md5::hex(b"+31641600986|2002-08-26 19:37:41|How are you?")
        );
        assert_eq!(fingerprint("a", "t", "x\ny\r"), fingerprint("a", "t", "x y"));
    }

    #[test]
    fn singles_are_pooled_once_with_their_read_state() {
        with_pool("singles", 3600, |st| {
            assert!(st.park(PDU_GSM, true, "sim:SM:3", 10).expect("park"));
            assert!(st.park(PDU_UCS2, false, "sim:SM:7", 10).expect("park"));
            let a = st.absorb(10).expect("absorb");
            let got: Vec<_> = a.new.iter().map(|m| (m.peer.as_str(), m.status, m.text.as_str())).collect();
            assert_eq!(
                got,
                [("+31641600986", Status::Unread, "How are you?"), ("ADANA BLD", Status::Read, "Kargonuz yolda ğüş")]
            );
            assert_eq!(a.new[0].utc_offset, Some(-60));
            assert!(st.parts.is_empty());
            // the same message again (a lost acknowledgement): nothing new, the part goes
            st.park(PDU_GSM, true, "air", 11).expect("park");
            assert!(st.absorb(11).expect("absorb").new.is_empty());
            assert!(st.parts.is_empty());
            // deleted from the pool, it stays deleted
            st.delete(Some(1)).expect("delete");
            st.park(PDU_GSM, true, "air", 12).expect("park");
            assert!(st.absorb(12).expect("absorb").new.is_empty());
            assert_eq!(st.counts(), (1, 0));
        });
    }

    #[test]
    fn a_long_message_waits_for_its_parts_then_joins() {
        with_pool("join", 3600, |st| {
            let at = "2026-10-12 08:00:00";
            st.park(&test_part(0x42, 2, 2, " world", at), true, "sim:SM:1", 10).expect("park");
            st.park(&test_part(0x43, 2, 1, "half", at), true, "sim:SM:4", 10).expect("park");
            let a = st.absorb(10).expect("absorb");
            assert_eq!((a.new.len(), a.waiting), (0, 2));
            st.park(&test_part(0x42, 2, 1, "hello", at), true, "air", 20).expect("park");
            let a = st.absorb(20).expect("absorb");
            assert_eq!((a.new.len(), a.waiting), (1, 1));
            assert_eq!((a.new[0].text.as_str(), a.new[0].parts, a.new[0].part_ids.len()), ("hello world", 2, 2));
            // part 2 delivered again: known as part of a message the pool has
            st.park(&test_part(0x42, 2, 2, " world", at), true, "air", 30).expect("park");
            assert_eq!(st.absorb(30).expect("absorb").waiting, 1);
            assert_eq!(st.parts.len(), 1);
        });
    }

    #[test]
    fn partial_messages_are_pooled_after_the_wait_and_repaired_in_place() {
        with_pool("partial", 3600, |st| {
            let at = "2026-10-12 08:00:00";
            st.park(&test_part(7, 3, 1, "first ", at), true, "air", 100).expect("park");
            st.park(&test_part(7, 3, 3, " third", at), true, "air", 100).expect("park");
            assert_eq!(st.absorb(100 + 3599).expect("absorb").waiting, 1);
            let a = st.absorb(100 + 3600).expect("absorb");
            assert_eq!(a.new.len(), 1);
            assert_eq!((a.new[0].text.as_str(), a.new[0].incomplete), ("first [...] third", Some((2, 3))));
            assert!(a.new[0].partial.is_some());
            let id = a.new[0].id;
            st.mark(id, true).expect("read");
            // The missing part comes later and completes the existing read message.
            let middle = test_part(7, 3, 2, "second", "2026-10-12 08:00:05");
            st.park(&middle, true, "air", 5000).expect("park");
            let a = st.absorb(5000).expect("absorb");
            assert!(a.new.is_empty());
            let message = st.get(id).expect("same message");
            assert_eq!(
                (message.text.as_str(), message.incomplete, message.status),
                ("first second third", None, Status::Read)
            );
            assert_eq!(st.counts(), (1, 0));
            st.park(&middle, true, "air", 5001).expect("park retry");
            assert!(st.absorb(5001).expect("absorb retry").new.is_empty());
            assert!(st.parts.is_empty());
            // a later message under the same reference (a day later) waits for its own parts
            st.park(&test_part(7, 3, 1, "x", "2026-10-13 08:00:00"), true, "air", 6000).expect("park");
            assert_eq!(st.absorb(6000).expect("absorb").waiting, 1);
        });
    }

    #[test]
    fn sim_messages_name_their_slots() {
        let at = "2026-10-12 08:00:00";
        let slots = vec![
            (1, test_part(0x42, 2, 2, " world", at)),
            (2, PDU_GSM.to_owned()),
            (3, test_part(0x42, 2, 1, "hello", at)),
            (4, "0011000B911346610089F60000AA05E8329BFD06".to_owned()),
        ];
        let got = sim_messages(&slots);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, [1, 3]);
        assert_eq!(got[1], (fingerprint("+31641600986", "2002-08-26 19:37:41", "How are you?"), vec![2]));
    }
}
