//! New received messages, found by reading `f50.sms list_unread` every few seconds: each one newer than the last
//! seen (ids only grow) goes to the balance reader and the remote control. The last seen id is kept on flash, so a
//! restart neither repeats a command nor, on the first start ever, runs the old ones.

use std::time::Duration;

use f50_core::time::unix_now;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{Inner, balance, remote, store};

const EVERY: Duration = Duration::from_secs(4);
const FILE: &str = "inbox.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    last_id: Option<u64>,
}

/// A received message as `list_unread` shows it.
#[derive(Debug, Clone)]
pub(crate) struct Incoming {
    pub id: u64,
    pub peer: String,
    pub text: String,
    /// When the pool took it (Unix seconds).
    pub pooled: u64,
}

/// A number as carriers and people write it: digits only, without China's `+86`/`86`.
pub(crate) fn normalize(number: &str) -> String {
    let d: String = number.chars().filter(char::is_ascii_digit).collect();
    match d.strip_prefix("86") {
        Some(rest) if d.len() == 13 => rest.to_owned(),
        _ => d,
    }
}

fn parse(v: &Value) -> Vec<Incoming> {
    v["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["dir"] == "in")
        .filter_map(|m| {
            Some(Incoming {
                id: m["id"].as_u64()?,
                peer: m["peer"].as_str()?.to_owned(),
                text: m["text"].as_str().unwrap_or_default().to_owned(),
                pooled: m["pooled"].as_u64().unwrap_or_else(unix_now),
            })
        })
        .collect()
}

pub(crate) async fn run(inner: std::sync::Arc<Inner>, shutdown: CancellationToken) {
    let mut seen: Seen = store::load(&inner.dir, FILE);
    let mut tick = tokio::time::interval(EVERY);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return,
            _ = tick.tick() => {}
        }
        let Ok(v) = inner.call("f50.sms", "list_unread", json!({ "page": 1, "per_page": 30 })).await else { continue };
        let mut new: Vec<Incoming> = parse(&v);
        let max = new.iter().map(|m| m.id).max();
        let Some(last) = seen.last_id else {
            // the first start: what is there already is not ours to act on
            seen.last_id = Some(max.unwrap_or(0));
            let _ = store::save(&inner.dir, FILE, &seen).await;
            continue;
        };
        new.retain(|m| m.id > last);
        if new.is_empty() {
            continue;
        }
        new.sort_by_key(|m| m.id);
        // remembered before anything runs: a reboot command is not run again after the reboot
        seen.last_id = new.last().map(|m| m.id);
        if let Err(e) = store::save(&inner.dir, FILE, &seen).await {
            tracing::warn!(error = %e, "the inbox position was not saved");
        }
        for m in new {
            let mut consumed = balance::offer(&inner, &m).await;
            consumed |= remote::offer(&inner, &m).await;
            if consumed {
                let _ = inner.call("f50.sms", "read", json!({ "id": m.id.to_string() })).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_lose_the_country_code() {
        assert_eq!(normalize("+8613581995712"), "13581995712");
        assert_eq!(normalize("8613581995712"), "13581995712");
        assert_eq!(normalize("10086"), "10086");
        assert_eq!(normalize("135 8199 5712"), "13581995712");
    }

    #[test]
    fn only_received_messages() {
        let v = json!({"messages": [
            {"dir": "in", "id": 15, "peer": "10086", "text": "x", "pooled": 9},
            {"dir": "out", "id": 16, "peer": "10086", "text": "y"}
        ]});
        let got = parse(&v);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].id, got[0].pooled), (15, 9));
    }
}
