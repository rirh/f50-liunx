//! What a line from the modem is: a final result code, the echo of a command, or an unsolicited result code.
//!
//! The final codes are exactly the ones mu300-atd's `collect` ended a reply on (`OK`, `ERROR`, `+CME ERROR`,
//! `+CMS ERROR`, `CONNECT...`, `NO CARRIER`). Anything else that is not unsolicited is an information line of the
//! command in flight. Commands like AT+CGACT and AT+CGDATA answer `^ORIG: <cid>,<n>` first and their final code ten
//! to twenty seconds later (docs/research/FINDINGS.md 13f): `^ORIG` is part of the reply, never a final code.

use std::fmt;

use f50_core::{Error, Result};

use crate::{Final, Reply};

/// Unsolicited codes this modem (or 27.007/27.005) sends. A line with one of these names is a URC unless the command
/// in flight names it too (AT+CREG? answers `+CREG: ...` as its reply).
const UNSOLICITED: &[&str] = &[
    "+CMTI",
    "+CMT",
    "+CDSI",
    "+CDS",
    "+CBM",
    "+CREG",
    "+CGREG",
    "+CEREG",
    "+C5GREG",
    "+CGEV",
    "+CUSD",
    "+CRING",
    "RING",
    "+CLIP",
    "+CCWA",
    "+CIEV",
    "+CTZV",
    "+CTZE",
    "+CTZEU",
    "+NITZ",
    "+SIND",
    "+ECIND",
    "+SPERRLOG",
    "+SPIMSREASON",
    "^SIMST",
    "+SIMST",
    "+CSSI",
    "+CSSU",
    "^CEND",
    "^CONN",
    "+SPNWNAME",
    "+CIREPI",
    "+CIREPH",
];

/// The final result code a line is, if it is one.
pub(crate) fn final_code(line: &str) -> Option<Final> {
    let l = line.trim();
    match l {
        "OK" => return Some(Final::Ok),
        "ERROR" => return Some(Final::Error),
        "NO CARRIER" => return Some(Final::NoCarrier),
        _ => {}
    }
    if l.starts_with("CONNECT") {
        return Some(Final::Other(l.to_owned()));
    }
    for (prefix, make) in [("+CME ERROR", Final::CmeError as fn(u32) -> Final), ("+CMS ERROR", Final::CmsError)] {
        if let Some(rest) = l.strip_prefix(prefix) {
            let n = rest.strip_prefix(':').unwrap_or(rest).trim();
            // the verbose form (+CMEE=2) has text instead of a number
            return Some(n.parse().map_or_else(|_| Final::Other(l.to_owned()), make));
        }
    }
    None
}

/// The modem's echo of a command (a line that starts with "AT"). Thrown away: a command can carry a secret (a PIN,
/// an APN password), and an echo is no answer (docs/research/FINDINGS.md 65; whether this modem echoes at all is not
/// measured).
pub(crate) fn is_echo(line: &str) -> bool {
    line.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("AT"))
}

/// The name of a line's result code: `+CSQ` of `+CSQ: 20,99`, `RING` of `RING`.
fn name(line: &str) -> &str {
    let l = line.trim_start();
    let end = l.find([':', ' ']).unwrap_or(l.len());
    &l[..end]
}

/// An unsolicited line that arrived while `cmd` was in flight: a known URC that the command does not ask for.
pub(crate) fn is_urc_during(line: &str, cmd: &str) -> bool {
    let n = name(line);
    if !UNSOLICITED.contains(&n) {
        return false;
    }
    let bare = n.trim_start_matches(['+', '^']).to_ascii_uppercase();
    !cmd.to_ascii_uppercase().contains(&bare)
}

/// A URC whose next line belongs to it: `+CMT: <alpha>,<length>` and `+CDS: <length>` carry their PDU on the line
/// after (docs/research/FINDINGS.md 51). Neither line may be logged: the PDU is the message.
pub(crate) fn takes_body(line: &str) -> bool {
    matches!(name(line), "+CMT" | "+CDS")
}

impl fmt::Display for Final {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Final::Ok => f.write_str("OK"),
            Final::Error => f.write_str("ERROR"),
            Final::CmeError(n) => write!(f, "+CME ERROR: {n}"),
            Final::CmsError(n) => write!(f, "+CMS ERROR: {n}"),
            Final::NoCarrier => f.write_str("NO CARRIER"),
            Final::Other(s) => f.write_str(s),
        }
    }
}

/// `Reply::check`: success is `OK`, and `CONNECT` (the answer of ATD and AT+CGDATA).
pub(crate) fn check(reply: Reply) -> Result<Reply> {
    match &reply.status {
        Final::Ok => Ok(reply),
        Final::Other(s) if s.starts_with("CONNECT") => Ok(reply),
        st => Err(Error::failed(format!("the modem answered {st}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::Code;

    #[test]
    fn finals() {
        assert_eq!(final_code("OK"), Some(Final::Ok));
        assert_eq!(final_code("ERROR"), Some(Final::Error));
        assert_eq!(final_code("+CME ERROR: 10"), Some(Final::CmeError(10)));
        assert_eq!(final_code("+CME ERROR:14"), Some(Final::CmeError(14)));
        assert_eq!(final_code("+CMS ERROR: 500"), Some(Final::CmsError(500)));
        assert_eq!(final_code("+CME ERROR: SIM busy"), Some(Final::Other("+CME ERROR: SIM busy".into())));
        assert_eq!(final_code("CONNECT"), Some(Final::Other("CONNECT".into())));
        assert_eq!(final_code("CONNECT 150000000"), Some(Final::Other("CONNECT 150000000".into())));
        assert_eq!(final_code("NO CARRIER"), Some(Final::NoCarrier));
        assert_eq!(final_code("^ORIG: 1,2"), None);
        assert_eq!(final_code("+CSQ: 20,99"), None);
        assert_eq!(final_code("OKAY"), None);
    }

    #[test]
    fn urcs_and_echo() {
        assert!(is_echo("AT+CSQ") && is_echo("at+cpin=1234") && !is_echo("+CSQ: 1"));
        assert!(is_urc_during("+CMTI: \"SM\",3", "AT+CSQ") && is_urc_during("RING", "AT"));
        assert!(is_urc_during("+CEREG: 1", "AT+CSQ"));
        assert!(!is_urc_during("+CEREG: 2,1,\"2F4B\"", "AT+CEREG?"));
        assert!(!is_urc_during("+CSQ: 20,99", "AT+CSQ"));
        assert!(!is_urc_during("+CUSD: 0,\"x\",15", "at+cusd=1,\"2A31303123\",15"));
        assert!(takes_body("+CMT: ,157") && takes_body("+CDS: 25") && !takes_body("+CMTI: \"SM\",1"));
    }

    #[test]
    fn check_and_display() {
        let r = |status| Reply { lines: vec![], status };
        assert!(check(r(Final::Ok)).is_ok());
        assert!(check(r(Final::Other("CONNECT".into()))).is_ok());
        let e = check(r(Final::CmeError(10))).expect_err("failed");
        assert_eq!(e.code(), Code::Failed);
        assert_eq!(e.message(), "the modem answered +CME ERROR: 10");
        assert_eq!(Final::NoCarrier.to_string(), "NO CARRIER");
    }
}
