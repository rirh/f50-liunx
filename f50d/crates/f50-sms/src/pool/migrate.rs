//! The shell pool (`/etc/f50/sms-pool`, mu300-sms) copied into a new pool on its first start, and left in place.
//!
//! Its format: `msg/<6-digit id>`, "field: value" headers, a blank line, the body (with a newline after it); `next_id`;
//! `deleted` (tombstones, one md5 per line); `parts/p*` (PDUs of direct deliveries waiting for siblings) and
//! `incoming/*.ready` (+CMT frames spooled but perhaps not yet pooled). Ids, fingerprints, part ids and tombstones
//! carry over unchanged, so nothing pooled there comes back from the SIM, and nothing deleted there either.

use std::collections::VecDeque;
use std::fs;
use std::path::Path;

use f50_core::Result;
use f50_core::time::unix_now;

use super::store::{Dir, Dirs, Message, Parked, Status, regular_files};
use crate::pdu::{Coding, is_pdu_line};

/// Copies the shell pool at `legacy` into `dirs` when there is one; the next id to continue from.
pub(super) fn from_shell_pool(dirs: &Dirs, legacy: &Path) -> Result<u64> {
    let old_msgs = legacy.join("msg");
    if !fs::symlink_metadata(&old_msgs).is_ok_and(|m| m.is_dir()) {
        return Ok(0);
    }
    let mut top = fs::read_to_string(legacy.join("next_id")).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0u64);
    let mut n = 0;
    for p in regular_files(&old_msgs, |name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit())) {
        let id = p.file_name().and_then(|n| n.to_str()).and_then(|n| n.parse::<u64>().ok());
        let Some(m) = id.and_then(|id| fs::read_to_string(&p).ok().and_then(|s| parse_message(id, &s))) else {
            tracing::warn!(file = %p.display(), "a message of the shell pool does not read; not copied");
            continue;
        };
        top = top.max(m.id);
        dirs.write_message(&m)?;
        n += 1;
    }
    if let Ok(t) = fs::read_to_string(legacy.join("deleted")) {
        let tombs: VecDeque<String> = t.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
        dirs.write_tombstones(&tombs)?;
    }
    let now = unix_now();
    let mut seq = 0;
    let parked = regular_files(&legacy.join("parts"), |n| n.starts_with('p'))
        .into_iter()
        .chain(regular_files(&legacy.join("incoming"), |n| n.ends_with(".ready")));
    for p in parked {
        let Some(pdu) = fs::read_to_string(&p).ok().and_then(|s| s.lines().next().map(|l| l.trim().to_owned())) else {
            continue;
        };
        if !is_pdu_line(&pdu) {
            continue;
        }
        seq += 1;
        let part = Parked {
            name: format!("p{now}-m{seq}"),
            pdu: pdu.to_ascii_uppercase(),
            unread: true,
            first_seen: now,
            origin: "legacy".into(),
        };
        dirs.write_part(&part)?;
    }
    dirs.sync_messages()?;
    dirs.sync_parts()?;
    tracing::info!(messages = n, parts = seq, from = %legacy.display(), "the shell SMS pool was copied");
    Ok(top)
}

/// One message file of the shell pool.
fn parse_message(id: u64, s: &str) -> Option<Message> {
    let (head, body) = s.split_once("\n\n").unwrap_or((s, ""));
    let field = |name: &str| {
        head.lines().find_map(|l| l.strip_prefix(name).and_then(|r| r.strip_prefix(": "))).map(str::to_owned)
    };
    let dir = match field("dir")?.as_str() {
        "mt" => Dir::In,
        "mo" => Dir::Out,
        _ => return None,
    };
    let status = match field("status").as_deref() {
        Some("unread") => Status::Unread,
        Some("sent") => Status::Sent,
        _ if dir == Dir::Out => Status::Sent,
        _ => Status::Read,
    };
    let peer = if dir == Dir::In { field("from") } else { field("to") }.unwrap_or_default();
    let text = body.strip_suffix('\n').unwrap_or(body).to_owned();
    let time = field("scts").unwrap_or_default();
    let incomplete = field("incomplete").and_then(|v| {
        let (h, t) = v.split_once('/')?;
        Some((h.trim().parse().ok()?, t.trim().parse().ok()?))
    });
    let part_ids: Vec<String> =
        field("part_ids").map(|v| v.split_whitespace().map(str::to_owned).collect()).unwrap_or_default();
    Some(Message {
        id,
        dir,
        status,
        fingerprint: field("fingerprint").unwrap_or_else(|| super::fingerprint(&peer, &time, &text)),
        peer,
        time,
        utc_offset: None,
        received: field("received").and_then(|v| v.parse().ok()).unwrap_or(0),
        coding: if field("coding").as_deref() == Some("ucs2") { Coding::Ucs2 } else { Coding::Gsm },
        parts: incomplete.map_or_else(|| u8::try_from(part_ids.len().max(1)).unwrap_or(1), |(_, t)| t),
        incomplete,
        partial: field("partial"),
        part_ids,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{Pool, test_dir};
    use super::*;

    #[test]
    fn the_shell_pool_is_copied_once_and_kept() {
        let root = test_dir("migrate");
        let legacy = root.join("sms-pool");
        fs::create_dir_all(legacy.join("msg")).expect("mkdir");
        fs::create_dir_all(legacy.join("parts")).expect("mkdir");
        fs::write(legacy.join("next_id"), "41\n").expect("write");
        fs::write(
            legacy.join("msg/000042"),
            "id: 000042\ndir: mt\nstatus: unread\nfrom: ADANA BLD\nscts: 2026-10-02 10:00:00\nreceived: 1790000000\n\
             sim_index: 3\ncoding: gsm\nfingerprint: abc\nlength: 9\nincomplete: 2/3\npartial: def\npart_ids: a b\n\n\
             line one\nstatus: not a header\n",
        )
        .expect("write");
        fs::write(legacy.join("msg/000007"), "id: 000007\ndir: mo\nstatus: sent\nto: 10086\nscts: x\n\nhi\n")
            .expect("write");
        fs::write(legacy.join("deleted"), "dead1\ndead2\n").expect("write");
        fs::write(
            legacy.join("parts/p1-2"),
            "07911326040000F0040B911346610089F60000208062917314480CC8F71D14969741F977FD07\n",
        )
        .expect("write");
        let pool = Pool::open(root.join("new"), &legacy, 3600).expect("open");
        let st = pool.state.lock().expect("lock");
        let m = st.get(42).expect("kept its id");
        assert_eq!((m.dir, m.status, m.peer.as_str()), (Dir::In, Status::Unread, "ADANA BLD"));
        assert_eq!((m.fingerprint.as_str(), m.incomplete, m.parts), ("abc", Some((2, 3)), 3));
        assert_eq!(m.text, "line one\nstatus: not a header");
        assert_eq!(st.get(7).expect("sent").status, Status::Sent);
        assert_eq!(st.next_id, 42);
        assert_eq!(st.tombs.len(), 2);
        assert_eq!(st.parts.len(), 1);
        drop(st);
        assert!(legacy.join("msg/000042").exists(), "the shell pool is left in place");
        // a second start does not copy again
        fs::remove_file(legacy.join("msg/000007")).expect("rm");
        let pool = Pool::open(root.join("new"), &legacy, 3600).expect("reopen");
        assert_eq!(pool.state.lock().expect("lock").counts(), (2, 1));
        let _ = fs::remove_dir_all(root);
    }
}
