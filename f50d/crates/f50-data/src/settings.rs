//! The data settings, per SIM card, in UCI: options of netifd's interface (`network.wan`).
//!
//! Why UCI and not `/etc/f50d`: netifd hands these options to the protocol, LuCI's interface page edits the same
//! ones, `sysupgrade` keeps `/etc/config/network`, and the file is mode 600 from the first boot (FINDINGS 65), the
//! only place the APN password may live. The daemon reads them again at every dial and every decision, so a change
//! needs no restart of anything.
//!
//! | SIM1, the tray card | SIM2, the internal card | |
//! |---|---|---|
//! | `apn` | `apn_internal` | empty: the context the SIM defines is kept |
//! | `pdptype` (`IP`) | `pdptype_internal` (`IPV4V6`) | `IP`, `IPV6`, `IPV4V6` |
//! | `auth`, `username`, `password` | `auth_internal`, `username_internal`, `password_internal` | FINDINGS 65 |
//! | `iccid` | `iccid_internal` | the settings are for this card only (FINDINGS 231) |
//! | `roaming` | `roaming_internal` | data while roaming, allowed unless 0/off/no/false (FINDINGS 120) |
//!
//! The credentials of one card are never offered to the other card's carrier: each has its own options. Shared:
//! `ipv6` (`relay`: K35-K37), `peerdns`, `dl_reboot` (the watchdog's step 3, FINDINGS 176).

use std::fmt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use f50_core::{Error, Result};
use f50_modem::Slot;
use f50_platform::Platform;
use f50_platform::uci::{Change, Section};
use serde::Serialize;

/// The PDP type of the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pdp {
    Ipv4,
    Ipv6,
    Ipv4v6,
}

impl Pdp {
    /// The word of AT+CGDCONT.
    pub fn at_word(self) -> &'static str {
        match self {
            Pdp::Ipv4 => "IP",
            Pdp::Ipv6 => "IPV6",
            Pdp::Ipv4v6 => "IPV4V6",
        }
    }

    /// The API's word or UCI's (LuCI writes `IP`, `IPV6`, `IPV4V6`).
    pub fn parse(s: &str) -> Option<Pdp> {
        match s.to_ascii_lowercase().as_str() {
            "ip" | "ipv4" => Some(Pdp::Ipv4),
            "ipv6" => Some(Pdp::Ipv6),
            "ipv4v6" => Some(Pdp::Ipv4v6),
            _ => None,
        }
    }
}

/// The APN's authentication, as AT+CGPCO numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    None = 0,
    Pap = 1,
    Chap = 2,
    /// PAP or CHAP, the network picks (what an unset auth with credentials means, as on Android).
    Both = 3,
}

/// One card's settings. The password is never printed by `Debug`.
#[derive(Clone, Default)]
pub struct CardSettings {
    pub apn: String,
    pub pdp: Option<Pdp>,
    /// As stored: `none`, `pap`, `chap`, `both`, or unset.
    pub auth: Option<String>,
    pub username: String,
    pub password: String,
    pub iccid: Option<String>,
    /// The stored roaming value as it is.
    pub roaming_raw: Option<String>,
}

impl fmt::Debug for CardSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CardSettings")
            .field("apn", &self.apn)
            .field("pdp", &self.pdp)
            .field("auth", &self.auth)
            .field("password_set", &!self.password.is_empty())
            .finish_non_exhaustive()
    }
}

/// Checked credentials: what AT+CGPCO gets.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub auth: Auth,
    pub username: String,
    pub password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials").field("auth", &self.auth).finish_non_exhaustive()
    }
}

impl CardSettings {
    /// The PDP type, with the card's default: the tray card's context was always IPv4, the internal card's stock
    /// RIL dials dual stack.
    pub fn pdp_or_default(&self, slot: Slot) -> Pdp {
        self.pdp.unwrap_or(match slot {
            Slot::Two => Pdp::Ipv4v6,
            Slot::One => Pdp::Ipv4,
        })
    }

    /// Data may be used while roaming: 1/on/yes/true allow, 0/off/no/false refuse, anything else is allowed (the
    /// default of every earlier release: an update must not take anybody's data away).
    pub fn roaming_allowed(&self) -> bool {
        self.roaming_raw.as_deref().and_then(roaming_word) != Some(false)
    }

    /// The credentials, checked (`apn_credentials`); a message saying what is wrong, never with what.
    pub fn credentials(&self) -> Result<Credentials> {
        let auth = match self.auth.as_deref().unwrap_or("") {
            "" if self.username.is_empty() && self.password.is_empty() => Auth::None,
            "" | "both" => Auth::Both,
            "none" => Auth::None,
            "pap" => Auth::Pap,
            "chap" => Auth::Chap,
            _ => return Err(Error::failed("the APN authentication is none, pap, chap or both")),
        };
        if auth == Auth::None {
            return Ok(Credentials { auth, username: String::new(), password: String::new() });
        }
        if self.username.chars().count() > 64 || self.password.chars().count() > 64 {
            return Err(Error::failed("the APN user name or password is longer than 64 characters"));
        }
        if !credential_ok(&self.username) || !credential_ok(&self.password) {
            return Err(Error::failed(
                "the APN user name and password may hold printable ASCII only, and no quote or backslash",
            ));
        }
        Ok(Credentials { auth, username: self.username.clone(), password: self.password.clone() })
    }
}

/// Printable ASCII without a quote or a backslash: the value is pasted into a quoted AT argument.
pub fn credential_ok(s: &str) -> bool {
    s.bytes().all(|b| (0x20..=0x7e).contains(&b) && b != b'"' && b != b'\\')
}

/// An APN that cannot end its quoted AT argument early: no quote, backslash or control character.
pub fn apn_ok(s: &str) -> bool {
    !s.chars().any(|c| c == '"' || c == '\\' || c.is_control())
}

/// 1/on/yes/true or 0/off/no/false; `None` for anything else.
pub fn roaming_word(s: &str) -> Option<bool> {
    match s {
        "1" | "on" | "yes" | "true" => Some(true),
        "0" | "off" | "no" | "false" => Some(false),
        _ => None,
    }
}

/// The option of a card's setting: `apn` for the tray card, `apn_internal` for the internal one.
pub fn option(slot: Slot, base: &str) -> String {
    match slot {
        Slot::Two => format!("{base}_internal"),
        Slot::One => base.to_owned(),
    }
}

/// The interface's settings as one read of `/etc/config/network`.
#[derive(Debug, Clone, Default)]
pub struct WanSettings {
    pub card: CardSettings,
    /// `option ipv6 'relay'`: relay mode (applies to a dual-stack context only).
    pub relay: bool,
    pub peerdns: bool,
    /// The watchdog may reboot (`option dl_reboot '0'` turns that off).
    pub dl_reboot: bool,
}

impl WanSettings {
    pub fn from_section(s: Option<&Section>, slot: Slot) -> WanSettings {
        let get = |base: &str| s.and_then(|s| s.get(&option(slot, base))).map(str::to_owned);
        let shared = |name: &str| s.and_then(|s| s.get(name)).map(str::to_owned);
        let card = CardSettings {
            apn: get("apn").unwrap_or_default(),
            pdp: get("pdptype").as_deref().and_then(Pdp::parse),
            auth: get("auth").filter(|a| !a.is_empty()),
            username: get("username").unwrap_or_default(),
            password: get("password").unwrap_or_default(),
            iccid: get("iccid").filter(|i| !i.is_empty()),
            roaming_raw: get("roaming"),
        };
        let off = |v: Option<String>| matches!(v.as_deref(), Some("0" | "off" | "no" | "false"));
        WanSettings {
            card,
            relay: shared("ipv6").as_deref() == Some("relay"),
            peerdns: !off(shared("peerdns")),
            dl_reboot: !off(shared("dl_reboot")),
        }
    }

    /// The settings of `interface` for the card in `slot`.
    pub async fn load(platform: &Platform, interface: &str, slot: Slot) -> Result<WanSettings> {
        let sections = platform.uci().load("network").await?;
        Ok(WanSettings::from_section(sections.iter().find(|s| s.name == interface), slot))
    }
}

/// The changes to one card's settings: `None` leaves an option as it is, an empty value deletes it.
#[derive(Default)]
pub struct Edit {
    pub apn: Option<String>,
    pub pdp: Option<Pdp>,
    pub auth: Option<Auth>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub roaming: Option<bool>,
}

impl fmt::Debug for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edit").field("apn", &self.apn).field("pdp", &self.pdp).finish_non_exhaustive()
    }
}

impl Edit {
    /// Something that changes the context (netifd has to dial again), not only the roaming policy.
    pub fn redials(&self) -> bool {
        self.apn.is_some()
            || self.pdp.is_some()
            || self.auth.is_some()
            || self.username.is_some()
            || self.password.is_some()
    }

    /// The UCI changes for `interface` and the card in `slot`.
    pub fn changes(&self, interface: &str, slot: Slot) -> Vec<Change> {
        let mut out = Vec::new();
        let mut put = |base: &str, v: Option<String>| {
            let Some(v) = v else { return };
            let option = option(slot, base);
            out.push(if v.is_empty() {
                Change::Delete { section: interface.to_owned(), option: Some(option) }
            } else {
                Change::Set { section: interface.to_owned(), option, value: v }
            });
        };
        put("apn", self.apn.clone());
        put("pdptype", self.pdp.map(|p| p.at_word().to_owned()));
        put(
            "auth",
            self.auth.map(|a| {
                match a {
                    Auth::None => "none",
                    Auth::Pap => "pap",
                    Auth::Chap => "chap",
                    Auth::Both => "both",
                }
                .to_owned()
            }),
        );
        put("username", self.username.clone());
        put("password", self.password.clone());
        put("roaming", self.roaming.map(|r| if r { "1" } else { "0" }.to_owned()));
        out
    }

    /// The settings as they will be after this edit (for the checks before it is saved).
    pub fn applied_to(&self, card: &CardSettings) -> CardSettings {
        let mut c = card.clone();
        if let Some(v) = &self.apn {
            c.apn.clone_from(v);
        }
        if let Some(p) = self.pdp {
            c.pdp = Some(p);
        }
        if let Some(a) = self.auth {
            c.auth = Some(format!("{a:?}").to_ascii_lowercase());
        }
        if let Some(v) = &self.username {
            c.username.clone_from(v);
        }
        if let Some(v) = &self.password {
            c.password.clone_from(v);
        }
        c
    }
}

/// The settings file that holds the APN password is its owner's alone (`settings_private`): made so when it is a
/// regular file of ours; a link is not followed for that, and what it leads to must already be private. A file
/// that is not there is fine (nothing to protect).
pub fn settings_private(path: &Path) -> bool {
    let Ok(lm) = std::fs::symlink_metadata(path) else { return true };
    if lm.file_type().is_file() {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    } else if !lm.file_type().is_symlink() {
        return false;
    }
    let (Ok(m), Ok(me)) = (std::fs::metadata(path), std::fs::metadata("/proc/self")) else { return false };
    m.is_file() && m.mode() & 0o077 == 0 && m.uid() == me.uid()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(auth: Option<&str>, user: &str, pass: &str) -> CardSettings {
        CardSettings {
            auth: auth.map(str::to_owned),
            username: user.into(),
            password: pass.into(),
            ..CardSettings::default()
        }
    }

    #[test]
    fn credentials_follow_the_shell() {
        assert_eq!(card(None, "", "").credentials().map(|c| c.auth), Ok(Auth::None));
        assert_eq!(card(None, "u", "p").credentials().map(|c| c.auth), Ok(Auth::Both));
        assert_eq!(card(Some("pap"), "u", "p").credentials().map(|c| c.auth), Ok(Auth::Pap));
        let none = card(Some("none"), "u", "p").credentials().expect("none");
        assert!(none.username.is_empty() && none.password.is_empty());
        assert!(card(Some("md5"), "u", "p").credentials().is_err());
        assert!(card(None, "u", "a\"b").credentials().is_err());
        assert!(card(None, "u", &"x".repeat(65)).credentials().is_err());
        let e = card(None, "u", "sec\\ret").credentials().expect_err("refused");
        assert!(!e.message().contains("sec"));
    }

    #[test]
    fn roaming_values() {
        let mut c = CardSettings::default();
        assert!(c.roaming_allowed());
        for (v, want) in [("0", false), ("off", false), ("1", true), ("maybe", true)] {
            c.roaming_raw = Some(v.into());
            assert_eq!(c.roaming_allowed(), want, "{v}");
        }
    }

    #[test]
    fn per_card_options_and_defaults() {
        assert_eq!(option(Slot::Two, "apn"), "apn_internal");
        assert_eq!(option(Slot::One, "apn"), "apn");
        assert_eq!(CardSettings::default().pdp_or_default(Slot::Two), Pdp::Ipv4v6);
        assert_eq!(CardSettings::default().pdp_or_default(Slot::One), Pdp::Ipv4);
        assert_eq!(Pdp::parse("IPV4V6"), Some(Pdp::Ipv4v6));
        assert!(!apn_ok("a\"b") && !apn_ok("a\nb") && apn_ok("internet.v6"));
        let e = Edit { password: Some(String::new()), roaming: Some(false), ..Edit::default() };
        let ch = e.changes("wan", Slot::Two);
        assert!(ch.contains(&Change::Delete { section: "wan".into(), option: Some("password_internal".into()) }));
        assert!(ch.contains(&Change::Set {
            section: "wan".into(),
            option: "roaming_internal".into(),
            value: "0".into()
        }));
        assert!(e.redials());
        assert!(!format!("{:?}", card(None, "u", "hunter2")).contains("hunter2"));
    }
}
