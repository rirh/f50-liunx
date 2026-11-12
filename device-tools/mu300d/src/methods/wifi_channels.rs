//! The 5 GHz channels the hotspot may use: /opt/mu300/lib/wifi-channels.sh in Rust. The firmware runs a 5 GHz AP on
//! 36-48 and 149-165 only, and the driver has no radar detection, so a country's list is those channels it allows
//! without DFS and without NO-IR (wireless-regdb).

const ALL: &[u32] = &[36, 40, 44, 48, 149, 153, 157, 161, 165];

/// The channels for country CC (empty: none, the hotspot has to use 2.4 GHz).
pub fn ap5_channels(cc: &str) -> Vec<u32> {
    let cc = cc.to_ascii_uppercase();
    match cc.as_str() {
        "00" | "AM" | "BH" | "DZ" | "TG" | "YE" => vec![],
        "AF" | "AI" | "AN" | "AW" | "AZ" | "BL" | "BT" | "BW" | "BY" | "EG" | "ET" | "FO" | "GE" | "GF" | "GI"
        | "GL" | "GP" | "IM" | "IR" | "JP" | "KE" | "KH" | "KW" | "LK" | "LS" | "MA" | "MF" | "MQ" | "MR" | "MW"
        | "NA" | "OM" | "PF" | "PM" | "QA" | "RE" | "RS" | "SA" | "SM" | "SR" | "SX" | "SY" | "TD" | "TN" | "TR"
        | "TZ" | "UZ" | "VA" | "VC" | "WF" | "WS" | "YT" | "ZW" => vec![36, 40, 44, 48],
        "BD" | "BO" | "BZ" | "CU" | "EC" | "NG" | "PK" => vec![149, 153, 157, 161, 165],
        "ID" | "KN" | "KP" | "LC" => vec![36, 40, 44, 48, 149, 153, 157, 161],
        _ => ALL.to_vec(),
    }
}

pub fn ap5_allowed(cc: &str, ch: u32) -> bool {
    ap5_channels(cc).contains(&ch)
}

/// The widest width in MHz (80, 40 or 20) whose channels CC all allows.
pub fn ap5_width(cc: &str, ch: u32) -> u32 {
    let block: &[u32] = match ch {
        36 | 40 | 44 | 48 => &[36, 40, 44, 48],
        149 | 153 | 157 | 161 => &[149, 153, 157, 161],
        _ => return 20,
    };
    if block.iter().all(|c| ap5_allowed(cc, *c)) {
        return 80;
    }
    let pair: [u32; 2] = match ch {
        36 | 40 => [36, 40],
        44 | 48 => [44, 48],
        149 | 153 => [149, 153],
        _ => [157, 161],
    };
    if pair.iter().all(|c| ap5_allowed(cc, *c)) { 40 } else { 20 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countries() {
        assert_eq!(ap5_channels("tr"), vec![36, 40, 44, 48]);
        assert!(ap5_channels("DZ").is_empty());
        assert!(ap5_allowed("DE", 165));
        assert!(!ap5_allowed("ID", 165));
        assert!(!ap5_allowed("DE", 52));
        assert_eq!(ap5_width("DE", 40), 80);
        assert_eq!(ap5_width("TR", 149), 20);
        assert_eq!(ap5_width("DE", 165), 20);
    }
}
