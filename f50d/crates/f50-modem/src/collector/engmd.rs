//! The Unisoc engineering readings: `AT+SPENGMD` serving cells (LTE, its PHY, NR), the LTE neighbour slots and the
//! NR neighbour table (`AT+SPQ5GNCELLEX`), and the band of a channel number.
//!
//! Unisoc separates fields with "-" and writes negatives as "+<abs>": `1-300-211--11747--654` is band 1, EARFCN 300,
//! PCI 211, RSRP -117.47, RSRQ -6.54. So ",-" becomes ",+" and "--" becomes "-+" first, then the line is split on
//! "-". A token may carry a comma list (bandwidth codes, CA flags); its scalar is the first field. dB values are
//! centi-units. `AT+SPENGMD=0,1,0` is never sent: it switches the AT port into streaming mode and kills the channel
//! until the next reboot (V50 archive; FINDINGS "45g").

use serde_json::{Map, Value, json};

/// The line split into tokens, the sign convention undone into "+" marks.
fn norm(line: &str) -> Vec<String> {
    let mut s = line.replace('\r', "").replace(",-", ",+").replace("--", "-+");
    if s.starts_with('-') {
        s.replace_range(0..1, "+");
    }
    s.split('-').map(|t| t.replace(' ', "")).collect()
}

/// awk's `x+0`: the leading number of `s`, 0 when there is none.
fn awk_num(s: &str) -> f64 {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut end = 0;
    if end < b.len() && (b[end] == b'-' || b[end] == b'+') {
        end += 1;
    }
    let digits = end;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    if end < b.len() && b[end] == b'.' {
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end == digits {
        return 0.0;
    }
    s[..end].parse().unwrap_or(0.0)
}

/// The scalar of a token with its sign restored ("+11747" -> -11747); `None` for an empty or NA token.
fn f(tok: Option<&String>) -> Option<f64> {
    let t = tok?;
    let v = t.split(',').next().unwrap_or("").replace('+', "-");
    if v.is_empty() || v == "NA" || v == "N/A" { None } else { Some(awk_num(&v)) }
}

/// A whole number or a decimal as JSON.
pub(crate) fn jnum(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9.0e15 {
        // a whole number below 2^53 is exact as an i64
        #[allow(clippy::cast_possible_truncation)]
        let i = x as i64;
        json!(i)
    } else {
        json!(x)
    }
}

pub(crate) fn one_decimal(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// A centi-unit value in dB, one decimal.
fn c1(x: Option<f64>) -> Value {
    x.map_or(Value::Null, |v| json!(one_decimal(v / 100.0)))
}

fn num(x: Option<f64>) -> Value {
    x.map_or(Value::Null, jnum)
}

/// awk's `%d`.
#[allow(clippy::cast_possible_truncation)]
fn int(x: Option<f64>) -> i64 {
    x.unwrap_or(0.0) as i64
}

/// A large id (the NR cell id is 36 bits) kept as a string: a negative one is its 32-bit complement, as the vendor
/// tools read it.
fn fs(tok: Option<&String>) -> Value {
    let Some(t) = tok else { return Value::Null };
    let v = t.split(',').next().unwrap_or("");
    let v = match v.strip_prefix('+') {
        Some(neg) => match neg.parse::<u64>().ok().and_then(|n| 4_294_967_296_u64.checked_sub(n)) {
            Some(n) => n.to_string(),
            None => v.to_owned(),
        },
        None => v.to_owned(),
    };
    if v.is_empty() || v == "NA" || v == "N/A" { Value::Null } else { json!(v) }
}

/// The LTE band of an EARFCN (0 when none of the table's).
pub fn lte_band(earfcn: f64) -> u32 {
    const T: &[(u32, f64, f64)] = &[
        (1, 0.0, 599.0),
        (2, 600.0, 1199.0),
        (3, 1200.0, 1949.0),
        (4, 1950.0, 2399.0),
        (5, 2400.0, 2649.0),
        (7, 2750.0, 3449.0),
        (8, 3450.0, 3799.0),
        (12, 5010.0, 5179.0),
        (13, 5180.0, 5279.0),
        (14, 5280.0, 5379.0),
        (17, 5730.0, 5849.0),
        (18, 5850.0, 5999.0),
        (19, 6000.0, 6149.0),
        (20, 6150.0, 6449.0),
        (25, 8040.0, 8689.0),
        (26, 8690.0, 9039.0),
        (27, 9040.0, 9209.0),
        (28, 9210.0, 9659.0),
        (30, 9770.0, 9869.0),
        (31, 9870.0, 9919.0),
        (33, 36000.0, 36199.0),
        (34, 36200.0, 36349.0),
        (35, 36350.0, 36949.0),
        (36, 36950.0, 37549.0),
        (37, 37550.0, 37749.0),
        (38, 37750.0, 38249.0),
        (39, 38250.0, 38649.0),
        (40, 38650.0, 39649.0),
        (41, 39650.0, 41589.0),
        (42, 41590.0, 43589.0),
        (43, 43590.0, 45589.0),
        (44, 45590.0, 46589.0),
        (46, 46790.0, 54539.0),
        (47, 54540.0, 55239.0),
        (48, 55240.0, 56739.0),
        (49, 56740.0, 58239.0),
        (50, 58240.0, 59089.0),
        (51, 59090.0, 59139.0),
        (65, 65536.0, 66435.0),
        (66, 66436.0, 67335.0),
    ];
    T.iter().find(|(_, lo, hi)| earfcn >= *lo && earfcn <= *hi).map_or(0, |(b, _, _)| *b)
}

/// The NR band of an NR-ARFCN (0 when none of the table's; n78 before n77, as the shell's table).
pub fn nr_band(arfcn: f64) -> u32 {
    const T: &[(u32, f64, f64)] = &[
        (1, 422_000.0, 434_000.0),
        (3, 361_000.0, 376_000.0),
        (5, 173_800.0, 178_800.0),
        (8, 185_000.0, 192_000.0),
        (28, 151_600.0, 160_600.0),
        (40, 460_000.0, 480_000.0),
        (41, 499_200.0, 537_999.0),
        (78, 620_000.0, 653_333.0),
        (77, 620_000.0, 680_000.0),
        (79, 693_334.0, 733_333.0),
    ];
    T.iter().find(|(_, lo, hi)| arfcn >= *lo && arfcn <= *hi).map_or(0, |(b, _, _)| *b)
}

fn no_cell(band: Option<f64>, fr: Option<f64>, pci: Option<f64>) -> bool {
    matches!(band, None | Some(0.0)) && fr == Some(0.0) && pci == Some(0.0)
}

/// The LTE serving cell (`AT+SPENGMD=0,6,0`), with the PHY line (`AT+SPENGMD=0,0,6`) when there is one: token 2 is
/// the SINR (centi-dB), 3 and 4 the UL/DL MCS histograms (the dominant index is the live MCS), 5 and 6 the BLER.
/// Range-guarded: the device answers all zeros while camped on 5G SA (no LTE anchor), and an idle histogram is all
/// zeros too; both mean no reading, not 0 dB. `Null` when there is no cell.
pub fn lte_serving(line: &str, phy: Option<&str>) -> Value {
    let t = norm(line);
    let (band, earfcn, pci) = (f(t.first()), f(t.get(1)), f(t.get(2)));
    if no_cell(band, earfcn, pci) {
        return Value::Null;
    }
    let bw: Vec<String> = t
        .get(7)
        .map(|s| s.split(',').collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| match awk_num(c) {
            0.0 => Some("1.4M".to_owned()),
            1.0 => Some("3M".to_owned()),
            2.0 => Some("5M".to_owned()),
            3.0 => Some("10M".to_owned()),
            4.0 => Some("15M".to_owned()),
            5.0 => Some("20M".to_owned()),
            _ => None,
        })
        .collect();
    let mut tac = f(t.get(28));
    if matches!(tac, None | Some(0.0)) {
        tac = f(t.get(29));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let tac = tac.map_or(String::new(), |v| format!("{:x}", v.max(0.0) as u64));
    let ca = if t.get(13).is_some_and(|c| c.contains("DL_CA")) { "DL_CA" } else { "" };
    let (mut sinr, mut dl_mcs, mut ul_mcs, mut dl_bler, mut ul_bler) =
        (Value::Null, Value::Null, Value::Null, Value::Null, Value::Null);
    if let Some(phy) = phy.filter(|p| !p.is_empty()) {
        let u = norm(phy);
        if let Some(s2) = f(u.get(2))
            && (-2000.0..=4000.0).contains(&s2)
        {
            sinr = json!(one_decimal(s2 / 100.0));
        }
        ul_bler = c1(f(u.get(5)));
        dl_bler = c1(f(u.get(6)));
        for dir in [3, 4] {
            let (mut best, mut best_n, mut total) = (None, 0.0, 0.0);
            for (i, h) in u.get(dir).map(String::as_str).unwrap_or("").split(',').enumerate() {
                let hv = awk_num(h);
                total += hv;
                if hv > best_n {
                    best_n = hv;
                    best = Some(i);
                }
            }
            if let Some(b) = best.filter(|_| total > 0.0) {
                if dir == 4 {
                    dl_mcs = json!(b);
                } else {
                    ul_mcs = json!(b);
                }
            }
        }
    }
    json!({
        "band": int(band), "earfcn": int(earfcn), "pci": int(pci),
        "rsrp": c1(f(t.get(3))), "rsrq": c1(f(t.get(4))), "bw": bw.join("+"),
        "enb": num(f(t.get(11))), "cid": num(f(t.get(12))), "tac": tac, "ca": ca,
        "sinr": sinr, "dl_mcs": dl_mcs, "ul_mcs": ul_mcs, "dl_bler": dl_bler, "ul_bler": ul_bler,
    })
}

/// The NR serving cell (`AT+SPENGMD=0,14,1`); `Null` when there is no cell.
pub fn nr_serving(line: &str) -> Value {
    let t = norm(line);
    let (band, arfcn, pci) = (f(t.first()), f(t.get(1)), f(t.get(2)));
    if no_cell(band, arfcn, pci) {
        return Value::Null;
    }
    json!({
        "band": int(band), "arfcn": int(arfcn), "pci": int(pci),
        "rsrp": c1(f(t.get(3))), "rsrq": c1(f(t.get(4))), "sinr": c1(f(t.get(15))),
        "bw_mhz": num(f(t.get(7))), "gnb": fs(t.get(8)), "cid": fs(t.get(9)),
        "ul_mcs": num(f(t.get(16))), "dl_mcs": num(f(t.get(17))),
        "ul_bler": c1(f(t.get(18))), "dl_bler": c1(f(t.get(19))),
    })
}

/// The LTE neighbours (`AT+SPENGMD=0,6,6`): eight 12-field slots whose first four are EARFCN, PCI, RSRP, RSRQ.
pub fn lte_neighbours(line: &str) -> Vec<Value> {
    let t = norm(line);
    let mut out = Vec::new();
    for s in 0..8 {
        let b = s * 12;
        if b + 3 >= t.len() {
            break;
        }
        let (earfcn, pci) = (f(t.get(b)), f(t.get(b + 1)));
        if earfcn == Some(0.0) && pci == Some(0.0) {
            continue;
        }
        let e = earfcn.unwrap_or(0.0);
        out.push(json!({
            "rat": "lte", "band": lte_band(e), "arfcn": int(earfcn), "pci": int(pci),
            "rsrp": c1(f(t.get(b + 2))), "rsrq": c1(f(t.get(b + 3))),
        }));
    }
    out
}

/// The NR neighbours (`AT+SPQ5GNCELLEX`, CSV): the serving 8-tuple, the count at index 8, then 8-tuples of ARFCN,
/// PCI, RSRP, RSRQ, SINR (centi-dB) and three more. Real per-cell SINR, unlike the LTE table.
pub fn nr_neighbours(line: &str) -> Vec<Value> {
    let a: Vec<&str> = line.split(',').collect();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cnt = a.get(8).map_or(0.0, |c| awk_num(c)).max(0.0) as usize;
    let mut out = Vec::new();
    for i in 0..cnt.min(8) {
        let b = 9 + i * 8;
        if b + 5 > a.len() {
            break;
        }
        let v = |k: usize| a.get(k).map_or(0.0, |s| awk_num(s));
        let (fr, pci) = (v(b), v(b + 1));
        if fr == 0.0 && pci == 0.0 {
            continue;
        }
        out.push(json!({
            "rat": "nr", "band": nr_band(fr), "arfcn": jnum(fr), "pci": jnum(pci),
            "rsrp": one_decimal(v(b + 2) / 100.0), "rsrq": one_decimal(v(b + 3) / 100.0),
            "sinr": one_decimal(v(b + 4) / 100.0),
        }));
    }
    out
}

/// One neighbour list, NR first (the page shows NR cells at the top); `Null` when there is none.
pub fn neighbours(nr: Vec<Value>, lte: Vec<Value>) -> Value {
    let all: Vec<Value> = nr.into_iter().chain(lte).collect();
    if all.is_empty() { Value::Null } else { Value::Array(all) }
}

/// An object's number field.
pub(crate) fn field_f64(v: &Value, k: &str) -> Option<f64> {
    v.as_object().and_then(|o: &Map<String, Value>| o.get(k)).and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lte_serving_decodes_the_sign_convention() {
        let line = "1-300-211--11747--654-0-0-5,5-0-0-0-12345-67-DL_CA";
        let v = lte_serving(line, None);
        assert_eq!(v["band"], 1);
        assert_eq!(v["earfcn"], 300);
        assert_eq!(v["pci"], 211);
        assert_eq!(v["rsrp"], -117.5);
        assert_eq!(v["rsrq"], -6.5);
        assert_eq!(v["bw"], "20M+20M");
        assert_eq!(v["enb"], 12345);
        assert_eq!(v["cid"], 67);
        assert_eq!(v["ca"], "DL_CA");
        assert_eq!(v["sinr"], Value::Null);
    }

    #[test]
    fn lte_without_a_cell_is_null() {
        assert_eq!(lte_serving("0-0-0-0-0", None), Value::Null);
        assert_eq!(nr_serving("0-0-0-0-0"), Value::Null);
    }

    #[test]
    fn lte_phy_sinr_mcs_and_guards() {
        let line = "3-1650-100--9000--1000";
        let phy = "0-0-1250-0,5,1-0,0,9,2-150-300";
        let v = lte_serving(line, Some(phy));
        assert_eq!(v["sinr"], 12.5);
        assert_eq!(v["ul_mcs"], 1);
        assert_eq!(v["dl_mcs"], 2);
        assert_eq!(v["ul_bler"], 1.5);
        assert_eq!(v["dl_bler"], 3.0);
        // camped on SA: all zeros is no reading
        let z = lte_serving(line, Some("0-0-0-0,0-0,0-0-0"));
        assert_eq!((z["ul_mcs"].clone(), z["dl_mcs"].clone()), (Value::Null, Value::Null));
        // out of range SINR
        assert_eq!(lte_serving(line, Some("0-0-9000"))["sinr"], Value::Null);
    }

    #[test]
    fn tac_is_hex() {
        let mut toks = vec!["1", "300", "211"];
        toks.resize(28, "0");
        toks.push("4660");
        let v = lte_serving(&toks.join("-"), None);
        assert_eq!(v["tac"], "1234");
    }

    #[test]
    fn nr_serving_keeps_large_ids() {
        let line = "78-627264-393--8050--1100-0-0-100-+1715503865-2579463431-0-0-0-0-0-1500-10-20-50-100";
        let v = nr_serving(line);
        assert_eq!(v["band"], 78);
        assert_eq!(v["arfcn"], 627_264);
        assert_eq!(v["rsrp"], -80.5);
        assert_eq!(v["bw_mhz"], 100);
        assert_eq!(v["gnb"], "2579463431");
        assert_eq!(v["cid"], "2579463431");
        assert_eq!(v["sinr"], 15.0);
        assert_eq!(v["ul_mcs"], 10);
        assert_eq!(v["dl_mcs"], 20);
    }

    #[test]
    fn neighbour_tables() {
        let mut slots = vec!["1650", "211", "-9500", "-1200"];
        slots.resize(12, "0");
        slots.extend(["0", "0", "0", "0"]);
        let ln = lte_neighbours(&slots.join("-"));
        assert_eq!(ln.len(), 1);
        assert_eq!(ln[0]["band"], 3);
        assert_eq!(ln[0]["rsrp"], -95.0);

        let nr = "627264,393,-8000,-1100,1500,0,0,0,2,627264,394,-9000,-1200,800,0,0,0,0,0,0,0,0,0,0,0";
        let nn = nr_neighbours(nr);
        assert_eq!(nn.len(), 1);
        assert_eq!(nn[0]["band"], 78);
        assert_eq!(nn[0]["pci"], 394);
        assert_eq!(nn[0]["sinr"], 8.0);
        assert_eq!(neighbours(nn, ln).as_array().map(Vec::len), Some(2));
        assert_eq!(neighbours(Vec::new(), Vec::new()), Value::Null);
    }

    #[test]
    fn bands_of_channels() {
        assert_eq!(lte_band(1650.0), 3);
        assert_eq!(lte_band(38_950.0), 40);
        assert_eq!(lte_band(99_999.0), 0);
        assert_eq!(nr_band(504_990.0), 41);
        assert_eq!(nr_band(640_000.0), 78);
    }
}
