//! The request rules of the Wi-Fi, LAN and USB methods, kept exactly as `f50-api` and `wifi-client` had them.
//!
//! The field rules (`Rule`) do the type and pattern; what depends on bytes, ends or another field is checked here.

use f50_core::rpc::{Rule, is_ascii_line, is_text_line, matches};

/// A hotspot name: 1-32 bytes, one line, no control character, no space first or last (as hostapd and wifi-client
/// take it). The field rule is `Text { 32, 32 }`; this adds the ends.
pub fn ssid_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && is_text_line(s) && !s.starts_with(' ') && !s.ends_with(' ')
}

/// A network name to join: 1-32 bytes and no control byte (below 0x20, or 0x7f), as wpa_supplicant counts them
/// (`wifi-client`'s `valid_text 1 32`). The API refused a space at either end too (`ssid_ok`).
pub fn join_ssid_ok(s: &str) -> bool {
    ssid_ok(s) && !s.bytes().any(|b| b < 0x20 || b == 0x7f)
}

/// WPA2's passphrase: 8-63 printable ASCII.
pub fn psk_ok(s: &str) -> bool {
    is_ascii_line(s) && (8..=63).contains(&s.len())
}

/// A raw WPA key: 64 hex digits.
pub fn raw_key_ok(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A WPA2 password hostapd takes: a passphrase, or a raw key.
pub fn wpa2_key_ok(s: &str) -> bool {
    psk_ok(s) || raw_key_ok(s)
}

/// A regulatory country as the radio's `country` option has it.
pub fn country_ok(s: &str) -> bool {
    matches("[A-Z0-9]{2}", s)
}

/// An interface name (IFNAMSIZ: 15 characters) that is never an option, "." or "..": it becomes part of a /sys path
/// and an `ip`/UCI argument.
pub fn ifname_ok(s: &str) -> bool {
    s.len() <= 15 && matches("[a-zA-Z0-9][a-zA-Z0-9_.:-]*", s)
}

/// The LAN address: a private /24's .1 (`f50-api lan.set`).
pub const LAN_ADDRESS: &str = concat!(
    r"(10\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])",
    r"|172\.(1[6-9]|2[0-9]|3[01])\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])",
    r"|192\.168\.(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9]))\.1"
);

// field rules shared by the objects
pub const SSID: Rule = Rule::Text { max_bytes: 32, max_chars: 32 };
pub const PASSWORD: Rule = Rule::Word { re: "[ -~]{8,64}", max: 64 };
pub const BAND: Rule = Rule::Enum(&["2.4", "5"]);
pub const CHANNEL: Rule = Rule::WordNum { re: "auto|[0-9]{1,3}", max: 4 };
pub const WIDTH: Rule = Rule::WordNum { re: "auto|20|40|80", max: 4 };
pub const COUNTRY: Rule = Rule::Word { re: "[A-Z]{2}", max: 2 };
pub const ON_OFF: Rule = Rule::Enum(&["on", "off"]);
pub const IFNAME: Rule = Rule::Word { re: "[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,14}", max: 15 };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_passwords() {
        assert!(ssid_ok("Home"));
        assert!(ssid_ok("Café ☕"));
        assert!(!ssid_ok(""));
        assert!(!ssid_ok(" Home"));
        assert!(!ssid_ok("Home "));
        assert!(!ssid_ok(&"x".repeat(33)));
        assert!(!ssid_ok("a\tb"));
        assert!(join_ssid_ok("Home"));
        assert!(!join_ssid_ok("a\u{7f}b"));
        assert!(psk_ok("12345678"));
        assert!(psk_ok("pass with spaces and \"quotes\""));
        assert!(!psk_ok("1234567"));
        assert!(!psk_ok(&"x".repeat(64)));
        assert!(!psk_ok("pässwort1"));
        assert!(raw_key_ok(&"aB".repeat(32)));
        assert!(!raw_key_ok(&"g".repeat(64)));
        assert!(wpa2_key_ok(&"0".repeat(64)));
    }

    #[test]
    fn lan_and_names() {
        for ok in ["192.168.77.1", "10.0.0.1", "172.16.5.1", "172.31.255.1"] {
            assert!(matches(LAN_ADDRESS, ok), "{ok}");
        }
        for bad in ["192.168.77.2", "172.32.0.1", "8.8.8.1", "192.168.256.1", "10.0.0.01"] {
            assert!(!matches(LAN_ADDRESS, bad), "{bad}");
        }
        assert!(ifname_ok("eth1"));
        assert!(ifname_ok("enx00e04c680001"));
        assert!(!ifname_ok("-x"));
        assert!(!ifname_ok(".."));
        assert!(!ifname_ok("a/b"));
        assert!(!ifname_ok("enx00e04c6800011"));
        assert!(country_ok("TR"));
        assert!(!country_ok("tr"));
    }
}
