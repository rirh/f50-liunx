//! The SMS side of the modem conversation: message storage, listing, reading and deleting slots, the notification
//! modes, the acknowledgement of a direct delivery, the SMSC refresh before a send. The answers' parsing is pure.
//!
//! Everything here runs inside an [`AtSession`] where several commands must reach the modem with nothing from
//! another client in between: PDU mode (`AT+CMGF=0`) is the modem's global state, and on one network sends failed
//! until the SMSC refresh, PDU mode and the submission went out as one transaction (docs/research/FINDINGS.md 46).

use std::time::Duration;

use f50_at::{At, AtSession, Final, Reply, SendOpts};
use f50_core::time::unix_now;
use f50_core::{Error, Result};
use serde::Serialize;

use crate::pdu::is_pdu_line;

/// New messages stored (on the SIM or in the modem) and announced with `+CMTI`.
pub(crate) const STORED_CNMI: &str = "AT+CNMI=2,1,0,0,0";
/// Direct delivery (FINDINGS 51): mode 3 (URCs straight to us), mt=2 (every message as a `+CMT` frame but class 2,
/// which stays stored with its `+CMTI`), bm=2 (cell broadcasts likewise; they need no acknowledgement), ds=0 (no
/// status reports: none is asked for, and one routed here would need an acknowledgement of its own, or the modem
/// drops the routing), bfr=1 (what was buffered while the link was down is discarded, not replayed).
pub(crate) const DIRECT_CNMI: &str = "AT+CNMI=3,2,2,0,1";

const T_SHORT: Duration = Duration::from_secs(8);
/// AT+CMGL walks the whole store.
const T_LIST: Duration = Duration::from_secs(30);

/// The message store as the last listing found it (the page's capacity line, read without asking the modem).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Storage {
    pub store: String,
    pub used: u32,
    pub total: u32,
    /// When it was read (Unix seconds).
    pub at: u64,
}

/// One stored message (a slot of the store).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Slot {
    pub index: u32,
    /// Stat 0: received unread.
    pub unread: bool,
    pub pdu: String,
}

/// The modem's final answer in words, for a message ("+CMS ERROR: 28").
pub(crate) fn final_text(f: &Final) -> String {
    match f {
        Final::Ok => "OK".into(),
        Final::Error => "ERROR".into(),
        Final::CmeError(n) => format!("+CME ERROR: {n}"),
        Final::CmsError(n) => format!("+CMS ERROR: {n}"),
        Final::NoCarrier => "NO CARRIER".into(),
        Final::Other(s) => s.clone(),
    }
}

fn need_ok(r: Reply, what: &str) -> Result<Reply> {
    if r.is_ok() { Ok(r) } else { Err(Error::failed(format!("{what}: the modem answered {}", final_text(&r.status)))) }
}

/// `AT+CMGF=0`: PDU mode. Text mode mangles names and cannot tell the parts of a long message (FINDINGS 26c).
pub(crate) async fn pdu_mode(s: &mut AtSession) -> Result<()> {
    need_ok(s.send("AT+CMGF=0", T_SHORT).await?, "PDU mode").map(|_| ())
}

/// Selects `mem` for reading, writing and receiving: the three-argument `AT+CPMS`, else the one-argument form some
/// firmwares want. The `+CPMS:` answer's used and total, when it gave them.
async fn select_store(s: &mut AtSession, mem: &str) -> Result<Option<(u32, u32)>> {
    for cmd in [format!("AT+CPMS=\"{mem}\",\"{mem}\",\"{mem}\""), format!("AT+CPMS=\"{mem}\"")] {
        let r = s.send(&cmd, Duration::from_secs(10)).await?;
        if r.is_ok() {
            return Ok(r.value("+CPMS").and_then(parse_cpms));
        }
    }
    Err(Error::failed(format!("the modem refused the {mem} message store")))
}

/// The store that holds anything, SIM (SM) first, then the modem's own (ME), as the shell tools picked it.
async fn pick_store(s: &mut AtSession) -> Result<Storage> {
    let mut chosen = None;
    for mem in ["SM", "ME"] {
        let Ok(cap) = select_store(s, mem).await else { continue };
        let (used, total) = cap.unwrap_or((0, 0));
        chosen = Some(Storage { store: mem.into(), used, total, at: unix_now() });
        if used != 0 {
            break;
        }
    }
    chosen.ok_or_else(|| Error::failed("the modem accepted neither SM nor ME storage"))
}

/// Every stored message (`AT+CMGL=4`, "all" in PDU mode) of the store that holds any.
pub(crate) async fn list(s: &mut AtSession) -> Result<(Storage, Vec<Slot>)> {
    let storage = pick_store(s).await?;
    pdu_mode(s).await?;
    let r = need_ok(s.send("AT+CMGL=4", T_LIST).await?, "listing the stored messages")?;
    Ok((storage, parse_listing(&r.lines)))
}

/// One slot (`AT+CMGR`), after selecting `mem` (the store a `+CMTI` named). `None` for an empty slot.
pub(crate) async fn read_slot(s: &mut AtSession, mem: Option<&str>, index: u32) -> Result<Option<Slot>> {
    if let Some(mem) = mem {
        select_store(s, mem).await?;
    }
    pdu_mode(s).await?;
    let r = s.send(&format!("AT+CMGR={index}"), Duration::from_secs(10)).await?;
    Ok(if r.is_ok() { parse_cmgr(&r.lines, index) } else { None })
}

/// `AT+CMGD`; true when the modem deleted it.
pub(crate) async fn delete_slot(s: &mut AtSession, index: u32) -> bool {
    matches!(s.send(&format!("AT+CMGD={index}"), Duration::from_secs(10)).await, Ok(r) if r.is_ok())
}

/// Stored mode, then direct delivery when `direct` (and the modem takes all three commands). Whether direct delivery
/// is on. A refusal of any of them leaves the stored mode as it was (FINDINGS 51).
pub(crate) async fn notification_mode(at: &At, direct: bool) -> Result<bool> {
    let mut s = at.session("sms", Duration::from_secs(8)).await?;
    let stored = s.send(STORED_CNMI, T_SHORT).await.map(|r| r.is_ok()).unwrap_or(false);
    if !stored {
        tracing::warn!("the modem refused {STORED_CNMI}");
    }
    if !direct {
        return Ok(false);
    }
    for cmd in ["AT+CMGF=0", "AT+CSMS=1", DIRECT_CNMI] {
        if !s.send(cmd, T_SHORT).await.map(|r| r.is_ok()).unwrap_or(false) {
            tracing::warn!(command = cmd, "the modem did not take direct SMS delivery; new messages are stored");
            let _ = s.send(STORED_CNMI, T_SHORT).await;
            return Ok(false);
        }
    }
    Ok(true)
}

/// Back to stored mode (after acknowledgements that kept failing).
pub(crate) async fn stored_mode(at: &At) {
    let _ = at.send(STORED_CNMI, SendOpts::control(T_SHORT)).await;
}

/// Whether the modem still routes messages to us: it drops mt to 0 by itself after an acknowledgement that never
/// came. Only mt is looked at: the modem says OK and keeps a bm of its own ("3,2,1,0,1" for "3,2,2,0,1", FINDINGS 51).
pub(crate) async fn direct_still_on(at: &At) -> Option<bool> {
    let r = at.send("AT+CNMI?", SendOpts::control(T_SHORT)).await.ok()?;
    r.value("+CNMI").and_then(cnmi_mt).map(|mt| mt == 2)
}

/// `AT+CNMA=1`: the message delivered directly is ours. Only once the pool holds it on the disk; until then the
/// modem keeps it, and forwards no other (CSMS=1). The modem held an unacknowledged message for two minutes
/// without dropping the routing (FINDINGS 51); the queue wait here is short so a busy lane cannot eat that.
pub(crate) async fn acknowledge(at: &At) -> Result<()> {
    let opts =
        SendOpts { lane: f50_at::Lane::Control, timeout: Duration::from_secs(5), queue_wait: Duration::from_secs(8) };
    need_ok(at.send("AT+CNMA=1", opts).await?, "the acknowledgement").map(|_| ())
}

/// The modem's SMSC written back to it unchanged before a send (`AT+CSCA?`, then `AT+CSCA="<same>",<type>`). On
/// one network every submission failed with the IMS stack's "Busy Everywhere" until exactly this had been done;
/// what it puts right inside the modem is not known (FINDINGS 46). The address stays byte for byte as the modem
/// showed it, in its character set (CSCS is never touched); nothing here ever picks another SMSC. What happened, in
/// words for the job's log (never the address).
pub(crate) async fn smsc_refresh(s: &mut AtSession) -> &'static str {
    let Ok(r) = s.send("AT+CSCA?", T_SHORT).await else {
        return "the SMSC could not be read; sending without the refresh";
    };
    let Some((addr, ty)) = r.value("+CSCA").and_then(parse_csca) else {
        return "the modem's SMSC answer was not understood; sending without the refresh";
    };
    match s.send(&format!("AT+CSCA=\"{addr}\",{ty}"), T_SHORT).await {
        Ok(w) if w.is_ok() => "SMSC refreshed",
        _ => "the modem refused to refresh its SMSC; sending anyway",
    }
}

/// The slots of an `AT+CMGL` answer: each `+CMGL: index,stat,[alpha],length` line followed by its PDU.
pub(crate) fn parse_listing(lines: &[String]) -> Vec<Slot> {
    let mut out = Vec::new();
    let mut head: Option<(u32, bool)> = None;
    for l in lines.iter().map(|l| l.trim()) {
        if let Some(rest) = l.strip_prefix("+CMGL:") {
            let mut f = rest.split(',').map(str::trim);
            head = match (f.next().and_then(|v| v.parse().ok()), f.next()) {
                (Some(i), Some(stat)) => Some((i, stat == "0")),
                _ => None,
            };
        } else if let Some((index, unread)) = head.take()
            && is_pdu_line(l)
        {
            out.push(Slot { index, unread, pdu: l.to_ascii_uppercase() });
        }
    }
    out
}

/// The slot of an `AT+CMGR` answer: `+CMGR: stat,[alpha],length` and the PDU.
pub(crate) fn parse_cmgr(lines: &[String], index: u32) -> Option<Slot> {
    let mut it = lines.iter().map(|l| l.trim());
    let head = it.by_ref().find_map(|l| l.strip_prefix("+CMGR:"))?;
    let unread = head.split(',').next().map(str::trim) == Some("0");
    let pdu = it.next().filter(|l| is_pdu_line(l))?;
    Some(Slot { index, unread, pdu: pdu.to_ascii_uppercase() })
}

/// Used and total from a `+CPMS:` value: `4,50,4,50,4,50` (set) or `"SM",4,50,...` (query).
pub(crate) fn parse_cpms(v: &str) -> Option<(u32, u32)> {
    let mut nums = v.split(',').map(str::trim).filter(|f| !f.starts_with('"')).map(|f| f.parse::<u32>());
    Some((nums.next()?.ok()?, nums.next()?.ok()?))
}

/// `"ADDRESS",TYPE`: `+` and digits, or hexadecimal (CSCS "HEX"), at most 64; a type of 1-3 digits.
pub(crate) fn parse_csca(v: &str) -> Option<(String, u16)> {
    let (addr, ty) = v.trim().rsplit_once(',')?;
    let addr = addr.strip_prefix('"')?.strip_suffix('"')?;
    let ok = !addr.is_empty() && addr.len() <= 64 && addr.bytes().all(|b| b == b'+' || b.is_ascii_hexdigit());
    let ty = ty.trim();
    (ok && (1..=3).contains(&ty.len()) && ty.bytes().all(|b| b.is_ascii_digit()))
        .then(|| Some((addr.to_owned(), ty.parse().ok()?)))
        .flatten()
}

/// The store and slot of a `+CMTI: "SM",3` line (the store only when it is two capital letters).
pub(crate) fn parse_cmti(line: &str) -> Option<(Option<String>, u32)> {
    let rest = line.strip_prefix("+CMTI:")?.trim();
    let (mem, idx) = rest.rsplit_once(',')?;
    let mem = mem.trim().trim_matches('"');
    let mem = (mem.len() == 2 && mem.bytes().all(|b| b.is_ascii_uppercase())).then(|| mem.to_owned());
    Some((mem, idx.trim().parse().ok()?))
}

/// The mt field of a `+CNMI:` value (`3,2,1,0,1`).
pub(crate) fn cnmi_mt(v: &str) -> Option<u8> {
    v.split(',').nth(1)?.trim().parse().ok()
}

/// The message reference of a `+CMGS:` value.
pub(crate) fn parse_cmgs(v: &str) -> Option<u8> {
    v.split(',').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::to_owned).collect()
    }

    #[test]
    fn listing_and_reads() {
        let l = lines("+CMGL: 3,0,,40\r\n0791aa\n+CMGL: 7,1,,56\nOK-not-a-pdu\n+CMGL: 9,1,,4\n00440B91");
        assert_eq!(
            parse_listing(&l),
            vec![
                Slot { index: 3, unread: true, pdu: "0791AA".into() },
                Slot { index: 9, unread: false, pdu: "00440B91".into() }
            ]
        );
        assert_eq!(
            parse_cmgr(&lines("+CMGR: 0,,40\n0791AA"), 4),
            Some(Slot { index: 4, unread: true, pdu: "0791AA".into() })
        );
        assert_eq!(parse_cmgr(&lines("+CMGR: 1,,40"), 4), None);
        assert_eq!(parse_cmgr(&[], 4), None);
    }

    #[test]
    fn answers() {
        assert_eq!(parse_cpms("4,50,4,50,4,50"), Some((4, 50)));
        assert_eq!(parse_cpms("\"SM\",4,50,\"SM\",4,50"), Some((4, 50)));
        assert_eq!(parse_cpms("x"), None);
        assert_eq!(parse_csca("\"2B38363133383030\",145"), Some(("2B38363133383030".into(), 145)));
        assert_eq!(parse_csca("\"+8613800\",145"), Some(("+8613800".into(), 145)));
        for bad in ["\"\",145", "\"+86\"", "+86,145", "\"+86;AT\",145", "\"+86\",1450", "\"+86\",x"] {
            assert_eq!(parse_csca(bad), None, "{bad}");
        }
        assert_eq!(parse_cmti("+CMTI: \"SM\",3"), Some((Some("SM".into()), 3)));
        assert_eq!(parse_cmti("+CMTI: \"sm;\",3"), Some((None, 3)));
        assert_eq!(parse_cmti("+CMTI: \"SM\",x"), None);
        assert_eq!(cnmi_mt("3,2,1,0,1"), Some(2));
        assert_eq!(cnmi_mt("2,1,0,0,0"), Some(1));
        assert_eq!(parse_cmgs("12"), Some(12));
        assert_eq!(final_text(&Final::CmsError(28)), "+CMS ERROR: 28");
    }
}
