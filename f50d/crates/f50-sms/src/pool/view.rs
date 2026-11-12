//! What the API shows of the pool: conversations (one per peer, newest first, with unread counts), the messages of
//! one conversation, and one message whole.

use serde::Serialize;

use super::State;
use super::store::{Dir, Message, Status};
use crate::pdu::Coding;

/// The first line of a text, at most this many characters (the shell list's 44 bytes, in characters).
const PREVIEW_CHARS: usize = 44;

/// One message as the API shows it (never its fingerprint or part ids).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MessageView {
    pub id: u64,
    pub dir: Dir,
    pub status: Status,
    pub peer: String,
    /// The service centre's time (received) or the send time in UTC (sent), `YYYY-MM-DD HH:MM:SS`.
    pub time: String,
    /// Minutes east of UTC of `time`, when known.
    pub utc_offset: Option<i16>,
    /// When the pool took it (Unix seconds).
    pub pooled: u64,
    pub coding: Coding,
    pub parts: u8,
    /// `{"have":2,"total":3}` for a long message pooled without all its parts.
    pub incomplete: Option<Incomplete>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct Incomplete {
    pub have: u8,
    pub total: u8,
}

impl From<&Message> for MessageView {
    fn from(m: &Message) -> Self {
        MessageView {
            id: m.id,
            dir: m.dir,
            status: m.status,
            peer: m.peer.clone(),
            time: m.time.clone(),
            utc_offset: m.utc_offset,
            pooled: m.received,
            coding: m.coding,
            parts: m.parts,
            incomplete: m.incomplete.map(|(have, total)| Incomplete { have, total }),
            text: m.text.clone(),
        }
    }
}

/// A conversation: every message to and from one peer.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Conversation {
    pub peer: String,
    pub messages: usize,
    pub unread: usize,
    /// The newest message's id, direction, status, time and the first line of its text.
    pub last_id: u64,
    pub last_dir: Dir,
    pub last_status: Status,
    pub last_time: String,
    pub preview: String,
}

/// A page of a list.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Page {
    pub page: usize,
    pub per_page: usize,
}

impl Page {
    fn pages(self, n: usize) -> usize {
        n.div_ceil(self.per_page).max(1)
    }
    fn take<T>(self, it: impl Iterator<Item = T>) -> Vec<T> {
        it.skip((self.page.saturating_sub(1)).saturating_mul(self.per_page)).take(self.per_page).collect()
    }
}

/// The first line, cut to [`PREVIEW_CHARS`] characters.
fn preview(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(PREVIEW_CHARS).collect()
}

impl State {
    /// Conversations newest first (by their newest message), with the pool's totals.
    pub fn conversations(&self, page: Page) -> serde_json::Value {
        let mut convs: Vec<Conversation> = Vec::new();
        // newest first: the first message seen of a peer is its newest
        for m in self.messages().rev() {
            let unread = usize::from(m.dir == Dir::In && m.status == Status::Unread);
            if let Some(c) = convs.iter_mut().find(|c| c.peer == m.peer) {
                c.messages += 1;
                c.unread += unread;
                continue;
            }
            convs.push(Conversation {
                peer: m.peer.clone(),
                messages: 1,
                unread,
                last_id: m.id,
                last_dir: m.dir,
                last_status: m.status,
                last_time: m.time.clone(),
                preview: preview(&m.text),
            });
        }
        let (total, unread) = self.counts();
        let n = convs.len();
        serde_json::json!({
            "total": total,
            "unread": unread,
            "conversations_total": n,
            "page": page.page,
            "pages": page.pages(n),
            "per_page": page.per_page,
            "conversations": page.take(convs.into_iter()),
        })
    }

    /// The messages to and from `peer`, newest first, whole (nothing is marked read).
    pub fn conversation(&self, peer: &str, page: Page) -> serde_json::Value {
        let all: Vec<&Message> = self.messages().rev().filter(|m| m.peer == peer).collect();
        let unread = all.iter().filter(|m| m.dir == Dir::In && m.status == Status::Unread).count();
        let n = all.len();
        let msgs: Vec<MessageView> = page.take(all.into_iter()).into_iter().map(MessageView::from).collect();
        serde_json::json!({
            "peer": peer,
            "total": n,
            "unread": unread,
            "page": page.page,
            "pages": page.pages(n),
            "per_page": page.per_page,
            "messages": msgs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{NewMessage, Pool, fingerprint, test_dir};
    use super::*;

    fn msg(dir: Dir, status: Status, peer: &str, text: &str) -> NewMessage {
        NewMessage {
            dir,
            status,
            peer: peer.into(),
            time: "2026-10-01 09:00:00".into(),
            utc_offset: None,
            coding: Coding::Gsm,
            parts: 1,
            incomplete: None,
            partial: None,
            part_ids: vec![],
            text: text.into(),
            fingerprint: fingerprint(peer, "2026-10-01 09:00:00", text),
        }
    }

    #[test]
    fn conversations_group_by_peer_newest_first() {
        let dir = test_dir("view");
        let pool = Pool::open(dir.clone(), &dir.join("none"), 3600).expect("open");
        let mut st = pool.state.lock().expect("lock");
        st.store(
            vec![
                msg(Dir::In, Status::Read, "+905551112233", "first"),
                msg(Dir::In, Status::Unread, "ADANA BLD", "second line one\nline two"),
                msg(Dir::Out, Status::Sent, "+905551112233", &"x".repeat(60)),
                msg(Dir::In, Status::Unread, "+905551112233", "again"),
            ],
            1,
        )
        .expect("store");
        let v = st.conversations(Page { page: 1, per_page: 10 });
        assert_eq!((v["total"].as_u64(), v["unread"].as_u64(), v["pages"].as_u64()), (Some(4), Some(2), Some(1)));
        let c = &v["conversations"];
        assert_eq!(c[0]["peer"], "+905551112233");
        assert_eq!(
            (c[0]["messages"].as_u64(), c[0]["unread"].as_u64(), c[0]["last_id"].as_u64()),
            (Some(3), Some(1), Some(4))
        );
        assert_eq!(c[1]["preview"], "second line one");
        let v = st.conversations(Page { page: 2, per_page: 1 });
        assert_eq!((v["pages"].as_u64(), v["conversations"][0]["peer"].as_str()), (Some(2), Some("ADANA BLD")));
        let v = st.conversation("+905551112233", Page { page: 1, per_page: 2 });
        assert_eq!((v["total"].as_u64(), v["pages"].as_u64()), (Some(3), Some(2)));
        assert_eq!(v["messages"][0]["text"], "again");
        assert_eq!(v["messages"][1]["dir"], "out");
        assert_eq!(preview(&"y".repeat(60)).len(), 44);
        drop(st);
        let _ = std::fs::remove_dir_all(dir);
    }
}
