//! The 5 GHz channels the hotspot may use, per country, and the widest channel width there.
//!
//! The SC2355 firmware runs a 5 GHz access point on 36-48 and 149-165 only (docs/research/FINDINGS.md 23), and the
//! driver has no radar detection (no `start_radar_detection`, FINDINGS 42): hostapd cannot start an access point on a
//! DFS channel (52-64, 100-144 and, in a few countries, also 36-48 or 149-165) - `start_dfs_cac()` fails and the
//! access point stays down. So the hotspot never goes onto a DFS channel: what is offered here is the firmware's
//! channels a country allows without DFS and without NO-IR, from wireless-regdb (db.txt of 2026; the countries whose
//! list differs from the full one are named below). Ported from `lib/wifi-channels.sh`.

/// Every 5 GHz channel the firmware offers an access point.
pub const AP5_ALL: &[u32] = &[36, 40, 44, 48, 149, 153, 157, 161, 165];
const LOW: &[u32] = &[36, 40, 44, 48];
const HIGH: &[u32] = &[149, 153, 157, 161, 165];
const NO_165: &[u32] = &[36, 40, 44, 48, 149, 153, 157, 161];

/// The 2.4 GHz channels the hotspot may use.
pub const AP2_ALL: &[u32] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];

/// No 5 GHz access point without DFS at all (00 is the world domain: NO-IR everywhere).
const NONE: &[&str] = &["00", "AM", "BH", "DZ", "TG", "YE"];
/// 149-165 not allowed, DFS or NO-IR.
const ONLY_LOW: &[&str] = &[
    "AF", "AI", "AN", "AW", "AZ", "BL", "BT", "BW", "BY", "EG", "ET", "FO", "GE", "GF", "GI", "GL", "GP", "IM", "IR",
    "JP", "KE", "KH", "KW", "LK", "LS", "MA", "MF", "MQ", "MR", "MW", "NA", "OM", "PF", "PM", "QA", "RE", "RS", "SA",
    "SM", "SR", "SX", "SY", "TD", "TN", "TR", "TZ", "UZ", "VA", "VC", "WF", "WS", "YT", "ZW",
];
/// 36-48 not allowed, DFS or NO-IR.
const ONLY_HIGH: &[&str] = &["BD", "BO", "BZ", "CU", "EC", "NG", "PK"];
/// No 165.
const WITHOUT_165: &[&str] = &["ID", "KN", "KP", "LC"];

/// The channels for country `cc` (any case); empty: none, the hotspot has to use 2.4 GHz.
pub fn ap5_channels(cc: &str) -> &'static [u32] {
    let cc = cc.to_ascii_uppercase();
    let cc = cc.as_str();
    if NONE.contains(&cc) {
        &[]
    } else if ONLY_LOW.contains(&cc) {
        LOW
    } else if ONLY_HIGH.contains(&cc) {
        HIGH
    } else if WITHOUT_165.contains(&cc) {
        NO_165
    } else {
        AP5_ALL
    }
}

/// `ch` is a DFS channel in every country (52-64, 100-144).
pub fn ap5_dfs(ch: u32) -> bool {
    matches!(ch, 52 | 56 | 60 | 64 | 100 | 104 | 108 | 112 | 116 | 120 | 124 | 128 | 132 | 136 | 140 | 144)
}

/// `ch` is one of `cc`'s channels.
pub fn ap5_allowed(cc: &str, ch: u32) -> bool {
    ap5_channels(cc).contains(&ch)
}

/// 36, else 149, else the first channel of `cc` (`None`: none).
pub fn ap5_default(cc: &str) -> Option<u32> {
    let l = ap5_channels(cc);
    [36, 149].into_iter().find(|c| l.contains(c)).or_else(|| l.first().copied())
}

/// What [`ap5_resolve`] decided for an explicit channel it could not keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The channel to use; `None` when the country has no 5 GHz channel at all.
    pub channel: Option<u32>,
    /// One line saying why the asked-for channel was replaced (`None`: kept, or "auto").
    pub note: Option<String>,
}

/// The channel to use for `asked` (`None`/0 = "auto"). An explicit channel `cc` does not allow (a DFS one, or one
/// outside the firmware's list) is replaced by the default, with the reason in one line.
pub fn ap5_resolve(cc: &str, asked: Option<u32>) -> Resolved {
    let d = ap5_default(cc);
    let ch = match asked {
        None | Some(0) => return Resolved { channel: d, note: None },
        Some(ch) => ch,
    };
    if ap5_allowed(cc, ch) {
        return Resolved { channel: Some(ch), note: None };
    }
    let cc_shown = if cc.is_empty() { "?" } else { cc };
    let why = if ap5_dfs(ch) {
        "a DFS channel (radar detection), which this Wi-Fi driver cannot do".to_owned()
    } else {
        format!("not available to the hotspot in country {cc_shown}")
    };
    let note = match d {
        Some(d) => format!("hotspot: 5 GHz channel {ch} is {why}: using channel {d}"),
        None => {
            format!("hotspot: 5 GHz channel {ch} is {why}, and country {cc_shown} has no 5 GHz channel without DFS")
        }
    };
    Resolved { channel: d, note: Some(note) }
}

/// The widest channel width in MHz (80, 40 or 20) whose channels `cc` all allows: 80 needs the block of four
/// (36-48, 149-161), 40 the pair (36+40, 44+48, 149+153, 157+161); 165 is 20 MHz only.
pub fn ap5_width(cc: &str, ch: u32) -> u32 {
    width_in(ap5_channels(cc), ch)
}

fn width_in(allowed: &[u32], ch: u32) -> u32 {
    let block: &[u32] = match ch {
        36 | 40 | 44 | 48 => &[36, 40, 44, 48],
        149 | 153 | 157 | 161 => &[149, 153, 157, 161],
        _ => return 20,
    };
    let pair: &[u32] = match ch {
        36 | 40 => &[36, 40],
        44 | 48 => &[44, 48],
        149 | 153 => &[149, 153],
        _ => &[157, 161],
    };
    if block.iter().all(|c| allowed.contains(c)) {
        80
    } else if pair.iter().all(|c| allowed.contains(c)) {
        40
    } else {
        20
    }
}

/// The radio's `htmode` for a 5 GHz channel, keeping the family of `current` (`HT40`, `VHT80`, `HE80`): HT goes up to
/// 40, NOHT (or none) stays as it is. `width` caps the country's widest (a narrower one asked for by the user).
pub fn htmode_for(current: Option<&str>, cc: &str, ch: u32, width: Option<u32>) -> Option<String> {
    let mut w = ap5_width(cc, ch);
    if let Some(asked) = width {
        w = w.min(asked);
    }
    match current {
        None | Some("" | "NOHT") => None,
        Some(ht) if ht.starts_with("HT") => Some(format!("HT{}", w.min(40))),
        Some(ht) => {
            let family: String = ht.chars().take_while(|c| !c.is_ascii_digit()).collect();
            Some(format!("{family}{w}"))
        }
    }
}

/// `channels` as UCI takes a list in one option: "36 40 44 48".
pub fn list_text(channels: &[u32]) -> String {
    channels.iter().map(u32::to_string).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_table() {
        assert_eq!(ap5_channels("CN"), AP5_ALL);
        assert_eq!(ap5_channels("de"), AP5_ALL);
        assert_eq!(ap5_channels("TR"), LOW);
        assert_eq!(ap5_channels("JP"), LOW);
        assert_eq!(ap5_channels("BD"), HIGH);
        assert_eq!(ap5_channels("ID"), NO_165);
        assert!(ap5_channels("00").is_empty());
        assert!(ap5_channels("AM").is_empty());
        assert_eq!(ap5_channels("ZZ"), AP5_ALL);
        assert_eq!(ap5_channels(""), AP5_ALL);
        for cc in ["CN", "TR", "BD", "ID", "US"] {
            assert!(ap5_channels(cc).iter().all(|c| !ap5_dfs(*c)), "{cc}");
        }
    }

    #[test]
    fn defaults_and_resolve() {
        assert_eq!(ap5_default("CN"), Some(36));
        assert_eq!(ap5_default("BD"), Some(149));
        assert_eq!(ap5_default("00"), None);
        let r = ap5_resolve("CN", Some(60));
        assert_eq!(r.channel, Some(36));
        assert_eq!(
            r.note.as_deref(),
            Some(
                "hotspot: 5 GHz channel 60 is a DFS channel (radar detection), which this Wi-Fi driver cannot do: \
                 using channel 36"
            )
        );
        assert_eq!(ap5_resolve("CN", Some(165)), Resolved { channel: Some(165), note: None });
        assert_eq!(ap5_resolve("CN", None), Resolved { channel: Some(36), note: None });
        assert_eq!(ap5_resolve("BD", Some(0)), Resolved { channel: Some(149), note: None });
        assert_eq!(
            ap5_resolve("TR", Some(149)).note.as_deref(),
            Some("hotspot: 5 GHz channel 149 is not available to the hotspot in country TR: using channel 36")
        );
        let none = ap5_resolve("00", Some(60));
        assert_eq!(none.channel, None);
        assert!(none.note.unwrap_or_default().contains("country 00 has no 5 GHz channel without DFS"));
        assert_eq!(ap5_resolve("00", None), Resolved { channel: None, note: None });
    }

    #[test]
    fn widths() {
        for (cc, ch, w) in [
            ("CN", 36, 80),
            ("CN", 48, 80),
            ("CN", 149, 80),
            ("CN", 161, 80),
            ("CN", 165, 20),
            ("TR", 40, 80),
            ("ID", 157, 80),
            ("BD", 153, 80),
            ("BD", 165, 20),
            ("CN", 60, 20),
        ] {
            assert_eq!(ap5_width(cc, ch), w, "{cc} {ch}");
        }
        let partial = [36, 40, 44];
        assert_eq!([36, 40, 44].map(|c| width_in(&partial, c)), [40, 40, 20]);
    }

    #[test]
    fn htmodes() {
        assert_eq!(htmode_for(Some("VHT80"), "CN", 165, None).as_deref(), Some("VHT20"));
        assert_eq!(htmode_for(Some("HT20"), "CN", 36, None).as_deref(), Some("HT40"));
        assert_eq!(htmode_for(Some("HE40"), "CN", 36, None).as_deref(), Some("HE80"));
        assert_eq!(htmode_for(Some("VHT80"), "CN", 36, Some(40)).as_deref(), Some("VHT40"));
        assert_eq!(htmode_for(Some("NOHT"), "CN", 36, None), None);
        assert_eq!(list_text(LOW), "36 40 44 48");
    }
}
