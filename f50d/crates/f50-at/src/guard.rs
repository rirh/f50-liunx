//! The AT terminal's guards, ported from f50dash (`at_wedges`, `at_moves_sim`, `at_class`, the ";" refusal) as
//! pure functions, and the redaction of the AT history.
//!
//! Every check looks at the command with spaces and double quotes dropped and in upper case, anywhere on the line,
//! so no spelling (case, spaces, quotes, leading zeros, more parameters) gets past it.

use std::sync::LazyLock;

use regex::Regex;

/// The longest AT command accepted.
pub const AT_MAX: usize = 512;

/// What sending a command would do beyond answering; each needs `confirm: true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Takes minutes and holds the channel (AT+COPS=?, any *SCAN*): runs as a job.
    Scan,
    /// A setting the modem keeps in NV (network mode, NR mode, EN-DC, band and cell locks).
    Nv,
    /// The radio, the data connection or the modem itself (CFUN, SFUN, CGATT, CGACT, CGDATA, COPS=n, ATD, assert).
    Radio,
    /// The stored data profiles (CGDCONT, CGAUTH).
    Apn,
    /// The SIM's PIN (CLCK other than the query, CPWD, CPIN): wrong PINs block the SIM.
    Sim,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Scan => "scan",
            Class::Nv => "nv",
            Class::Radio => "radio",
            Class::Apn => "apn",
            Class::Sim => "sim",
        }
    }
}

/// Why a command is refused before anything is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not one line of printable ASCII starting with AT, or longer than `AT_MAX`.
    Malformed,
    /// ";" would chain a second command past the checks.
    Chained,
    /// AT+SPENGMD=0,1,...: switches the port into streaming mode, the channel is dead until a reboot.
    Wedges,
    /// Moves the SIM selection or the data route (AT+SPACTCARD=, AT+SMMSWAP=, AT+SPSWDATA).
    MovesSim,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Refusal::Malformed => "an AT command is one line of printable ASCII starting with AT, at most 512 bytes",
            Refusal::Chained => "no \";\" command chaining",
            Refusal::Wedges => "denied: this command wedges the AT port until a reboot",
            Refusal::MovesSim => "denied: this command moves the SIM selection or the data route; use sim.select",
        }
    }
}

/// A guard pattern. A pattern that does not compile is a bug the tests catch; at run time it is `None`, and every
/// check that uses it fails closed (see [`hit`]).
fn rx(p: &str) -> Option<Regex> {
    Regex::new(p).ok()
}

/// `re` matches `s`; `fallback` when the pattern did not compile: `true` for the patterns that refuse or ask for a
/// confirmation, `false` for the ones that make an exception, so a broken pattern never lets a command through.
fn hit(re: &Option<Regex>, s: &str, fallback: bool) -> bool {
    re.as_ref().map_or(fallback, |r| r.is_match(s))
}

type Rx = LazyLock<Option<Regex>>;

static MOVES_SIM: LazyLock<[Option<Regex>; 3]> =
    LazyLock::new(|| [rx(r"\+(SPACTCARD|SMMSWAP)=([^?]|$)"), rx(r"\+SPSWDATA($|[^?=])"), rx(r"\+SPSWDATA=([^?]|$)")]);
static SCAN: Rx = LazyLock::new(|| rx(r"\+COPS=\?|SCAN"));
static NV: LazyLock<[Option<Regex>; 2]> =
    LazyLock::new(|| [rx(r"\+(SPTESTMODEM?|SP5GRAN|SPENDC)=([^?]|$)"), rx(r"\+SPLBAND=0*[12]([^0-9]|$)")]);
static FORCEFRQ: Rx = LazyLock::new(|| rx(r"\+SPFORCEFRQ=[0-9]*,"));
static FORCEFRQ_READ: Rx = LazyLock::new(|| rx(r"\+SPFORCEFRQ=[0-9]*,0*3([^0-9]|$)"));
static RADIO: LazyLock<[Option<Regex>; 4]> = LazyLock::new(|| {
    [
        rx(r"\+(CFUN|SFUN|CGATT|CGACT|SPATASSERT)=([^?]|$)"),
        rx(r"\+CGDATA"),
        rx(r"\+COPS=0*[0124]([^0-9]|$)"),
        rx(r"^ATD"),
    ]
});
static APN: Rx = LazyLock::new(|| rx(r"\+(CGDCONT|CGAUTH)=([^?]|$)"));
static CLCK: Rx = LazyLock::new(|| rx(r"\+CLCK=([^?]|$)"));
static CLCK_QUERY: Rx = LazyLock::new(|| rx(r"\+CLCK=[A-Z0-9]*,0*2([^0-9]|$)"));
static PIN: Rx = LazyLock::new(|| rx(r"\+(CPWD|CPIN)=([^?]|$)"));
/// Matched against the command in upper case (ASCII only, so the offsets are the original's).
static SECRET_ARGS: Rx = LazyLock::new(|| rx(r#"(\+\s*"?\s*(CPIN|CPWD|CLCK|CGAUTH)|\$\s*QCPDPP)\s*"?\s*="#));

/// The command as the checks see it: spaces and double quotes dropped, upper case.
pub fn normalize(cmd: &str) -> String {
    cmd.chars().filter(|c| *c != ' ' && *c != '"').map(|c| c.to_ascii_uppercase()).collect()
}

/// AT+SPENGMD with the parameters 0,1 first (each compared by its number: 00 = 0, "1" = 1), anywhere on the line.
pub fn wedges(cmd: &str) -> bool {
    let n = normalize(cmd);
    let mut s = n.as_str();
    while let Some(i) = s.find("+SPENGMD=") {
        s = &s[i + "+SPENGMD=".len()..];
        let mut p = s.split(',');
        let p1 = p.next().unwrap_or("");
        let p2 = p.next().unwrap_or("");
        let p1_zero = !p1.is_empty() && p1.bytes().all(|b| b == b'0');
        let rest = p2.trim_start_matches('0');
        let p2_one = rest.starts_with('1') && !rest[1..].starts_with(|c: char| c.is_ascii_digit());
        if p1_zero && p2_one {
            return true;
        }
    }
    false
}

/// One of the commands that say which card the modem's channels and its packet data belong to.
pub fn moves_sim(cmd: &str) -> bool {
    let n = normalize(cmd);
    MOVES_SIM.iter().any(|r| hit(r, &n, true))
}

/// What the command would do beyond answering, or None for one that only reads.
pub fn class(cmd: &str) -> Option<Class> {
    let s = normalize(cmd);
    if hit(&SCAN, &s, true) {
        Some(Class::Scan)
    } else if NV.iter().any(|r| hit(r, &s, true)) || (hit(&FORCEFRQ, &s, true) && !hit(&FORCEFRQ_READ, &s, false)) {
        Some(Class::Nv)
    } else if RADIO.iter().any(|r| hit(r, &s, true)) {
        Some(Class::Radio)
    } else if hit(&APN, &s, true) {
        Some(Class::Apn)
    } else if (hit(&CLCK, &s, true) && !hit(&CLCK_QUERY, &s, false)) || hit(&PIN, &s, true) {
        Some(Class::Sim)
    } else {
        None
    }
}

/// The command trimmed, checked, and classified.
pub fn check(cmd: &str) -> Result<(String, Option<Class>), Refusal> {
    let c = cmd.trim_matches(|c: char| c == ' ' || c == '\t');
    let ascii = c.bytes().all(|b| (0x20..=0x7e).contains(&b));
    if c.is_empty() || c.len() > AT_MAX || !ascii || !c.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("AT")) {
        return Err(Refusal::Malformed);
    }
    if c.contains(';') {
        return Err(Refusal::Chained);
    }
    if wedges(c) {
        return Err(Refusal::Wedges);
    }
    if moves_sim(c) {
        return Err(Refusal::MovesSim);
    }
    Ok((c.to_owned(), class(c)))
}

/// The command as the history keeps it: one that carries a PIN or a password (CPIN, CPWD, CLCK, CGAUTH, $QCPDPP)
/// without its arguments.
pub fn redact(cmd: &str) -> String {
    let Some(re) = SECRET_ARGS.as_ref() else {
        // no pattern: keep nothing but the start, which names no argument
        return format!("{}...", cmd.get(..2).unwrap_or(""));
    };
    match re.find(&cmd.to_ascii_uppercase()) {
        Some(m) => format!("{}...", cmd.get(..m.end()).unwrap_or("")),
        None => cmd.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_compiles() {
        let singles: [&Rx; 8] = [&SCAN, &FORCEFRQ, &FORCEFRQ_READ, &APN, &CLCK, &CLCK_QUERY, &PIN, &SECRET_ARGS];
        assert!(singles.iter().all(|r| r.is_some()));
        assert!(MOVES_SIM.iter().chain(NV.iter()).chain(RADIO.iter()).all(Option::is_some));
    }

    #[test]
    fn wedge_in_any_spelling() {
        for c in [
            "AT+SPENGMD=0,1,0",
            "at+spengmd=0,1",
            "AT + SPENGMD = 00 , 01 , 5",
            "AT+SPENGMD=\"0\",\"1\",\"0\"",
            "AT+CSQ+SPENGMD=0,1,0",
            "AT+SPENGMD=0,6,0+SPENGMD=0,1,0",
            "AT+SPENGMD=0,1+X",
        ] {
            assert!(wedges(c), "{c}");
            assert_eq!(check(c), Err(Refusal::Wedges), "{c}");
        }
        for c in ["AT+SPENGMD=0,6,0", "AT+SPENGMD=0,14,1", "AT+SPENGMD=1,1,0", "AT+SPENGMD=0,10", "AT+SPENGMD=0"] {
            assert!(!wedges(c), "{c}");
        }
    }

    #[test]
    fn sim_movers_denied_queries_allowed() {
        for c in ["AT+SPACTCARD=1", "at+smmswap=0", "AT+SPSWDATA", "AT+SPSWDATA=1", "AT+SPACTCARD="] {
            assert!(moves_sim(c), "{c}");
        }
        for c in ["AT+SPACTCARD?", "AT+SPSWDATA?", "AT+SPSWDATA=?", "AT+SMMSWAP=?"] {
            assert!(!moves_sim(c), "{c}");
        }
    }

    #[test]
    fn chaining_and_shape() {
        assert_eq!(check("AT+CSQ;+CFUN=0"), Err(Refusal::Chained));
        assert_eq!(check("+CSQ"), Err(Refusal::Malformed));
        assert_eq!(check("AT+CSQ\r\nAT+CFUN=0"), Err(Refusal::Malformed));
        assert_eq!(check(""), Err(Refusal::Malformed));
        assert_eq!(check(&format!("AT{}", "Q".repeat(511))), Err(Refusal::Malformed));
        assert_eq!(check("  at+csq  "), Ok(("at+csq".into(), None)));
        assert_eq!(check("AT+CSQ ü"), Err(Refusal::Malformed));
    }

    #[test]
    fn classes() {
        let cases = [
            ("AT+COPS=?", Some(Class::Scan)),
            ("AT+SPFREQSCAN=1", Some(Class::Scan)),
            ("AT+SPTESTMODEM=21,254", Some(Class::Nv)),
            ("AT+SPTESTMODE=21", Some(Class::Nv)),
            ("AT+SPTESTMODE?", None),
            ("AT+SP5GRAN=1", Some(Class::Nv)),
            ("AT+SPENDC=0", Some(Class::Nv)),
            ("AT+SPLBAND=1,3", Some(Class::Nv)),
            ("AT+SPLBAND=0", None),
            ("AT+SPFORCEFRQ=12,1,100", Some(Class::Nv)),
            ("AT+SPFORCEFRQ=12,3", None),
            ("AT+SPFORCEFRQ=12,03", None),
            ("AT+CFUN=0", Some(Class::Radio)),
            ("AT+CFUN?", None),
            ("AT+SFUN=4", Some(Class::Radio)),
            ("AT+CGDATA=\"M-ETHER\",1", Some(Class::Radio)),
            ("AT+COPS=2", Some(Class::Radio)),
            ("AT+COPS=3,0", None),
            ("AT+COPS?", None),
            ("ATD*99#", Some(Class::Radio)),
            ("AT+SPATASSERT=1", Some(Class::Radio)),
            ("AT+CGDCONT=1,\"IP\",\"internet\"", Some(Class::Apn)),
            ("AT+CGDCONT?", None),
            ("AT+CGAUTH=1,1,\"u\",\"p\"", Some(Class::Apn)),
            ("AT+CLCK=\"SC\",1,\"1234\"", Some(Class::Sim)),
            ("AT+CLCK=\"SC\",2", None),
            ("AT+CPIN=1234", Some(Class::Sim)),
            ("AT+CPIN?", None),
            ("AT+CPWD=\"SC\",\"1\",\"2\"", Some(Class::Sim)),
            ("AT+CSQ", None),
            ("ATI", None),
        ];
        for (c, want) in cases {
            assert_eq!(class(c), want, "{c}");
        }
    }

    #[test]
    fn redaction() {
        assert_eq!(redact("AT+CPIN=1234"), "AT+CPIN=...");
        assert_eq!(redact("at+cpin=\"1234\""), "at+cpin=...");
        assert_eq!(redact("AT+CLCK=\"SC\",1,\"0000\""), "AT+CLCK=...");
        assert_eq!(redact("AT+CPWD=\"SC\",\"1\",\"2\""), "AT+CPWD=...");
        assert_eq!(redact("AT+CGAUTH=1,1,\"user\",\"pw\""), "AT+CGAUTH=...");
        assert_eq!(redact("AT + CPIN = 1234"), "AT + CPIN =...");
        assert_eq!(redact("AT$QCPDPP=1,1,\"pw\""), "AT$QCPDPP=...");
        assert_eq!(redact("ATE0+CPIN=1234"), "ATE0+CPIN=...");
        assert_eq!(redact("AT+CPIN?"), "AT+CPIN?");
        assert_eq!(redact("AT+CSQ"), "AT+CSQ");
    }
}
