//! The lock encodings the modem uses (the ufi_tools standalone mode's, cross-checked against the F50's own
//! read-backs) and the validation of the values a caller may send.
//!
//! Field layouts verified on the device:
//! - `+SPTESTMODE: 134,134,0,...`: mode of slot 0, mode of slot 1, primary (FINDINGS 74; extra fields ignored);
//! - `+SP5GRAN: 1`: SA capable; `+ENDC: 1`: EN-DC allowed (off is 2, not 0: measured);
//! - `+SPLBAND: <5 masks>` (read 0): LTE masks over band bases [49,33,17,1,65], bit n = base + n;
//! - `+SPLBAND: <3 masks>` (read 3): NR tables A (n1..n74), B (n34..n79), super (n75..n86);
//! - `+SPFORCEFRQ: 12,3[,fr,pci...]` the LTE cell lock, `16,3` the NR one; no pair = no lock.

use f50_core::rpc::check_word;
use f50_core::{Error, Result};

/// The F50's LTE table: a cleared mask (`AT+SPLBAND=1,0,0,0,0,0`) reads back as `0,482,0,149,0`, these bands. There
/// is no LTE capability query; fixed per board, never learned from a read-back (a lock left in the NV by Android or
/// the stock UI would become "the table"; FINDINGS 223).
pub const LTE_TABLE: &[u32] = &[1, 3, 5, 8, 34, 38, 39, 40, 41];

/// The NR bands when `AT+SP5GCMDS="get nr support_band"` says nothing (measured: 6,41,78,1,8,28,5). Never write a
/// band the module does not support: an n79 lock (from an old, wrong capability decode) left the CP searching until
/// it wedged.
pub const NR_FALLBACK: &[u32] = &[1, 5, 6, 8, 28, 41, 78];

const LTE_BASES: [u32; 5] = [49, 33, 17, 1, 65];
const NR_A: &[u32] = &[1, 2, 3, 5, 7, 8, 12, 20, 25, 28, 66, 70, 71, 74];
const NR_B: &[u32] = &[34, 38, 39, 40, 41, 50, 51, 77, 78, 79];
const NR_S: &[u32] = &[75, 76, 80, 81, 82, 83, 84, 86];

/// A band list: `1,3,5` -> [1, 3, 5] (no validation beyond digits).
pub fn bands(s: &str) -> Vec<u32> {
    s.split(',').filter_map(|b| b.trim().parse().ok()).collect()
}

/// Sorted, without duplicates, as a list.
pub fn canonical(b: &[u32]) -> Vec<u32> {
    let mut v = b.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

pub fn join(b: &[u32]) -> String {
    b.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

/// The bands of `want` the modem supports, in the order given.
pub fn filter(want: &[u32], supported: &[u32]) -> Vec<u32> {
    want.iter().copied().filter(|b| supported.contains(b)).collect()
}

/// LTE bands -> the five masks.
pub fn lte_masks(b: &[u32]) -> [u32; 5] {
    let mut m = [0; 5];
    for &band in &canonical(b) {
        for (j, base) in LTE_BASES.iter().enumerate() {
            if band >= *base && band - base <= 15 {
                m[j] |= 1 << (band - base);
            }
        }
    }
    m
}

/// The five masks of a read-back -> bands; `None` when the read is not five numbers.
pub fn lte_bands(raw: &str) -> Option<Vec<u32>> {
    let m: Vec<u32> = raw.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    if m.len() != 5 {
        return None;
    }
    let mut out = Vec::new();
    for (j, base) in LTE_BASES.iter().enumerate() {
        for n in 0..16 {
            if m[j] >> n & 1 == 1 {
                out.push(base + n);
            }
        }
    }
    Some(out)
}

/// NR bands -> the three table masks.
pub fn nr_masks(b: &[u32]) -> [u32; 3] {
    let mut m = [0; 3];
    for (k, table) in [NR_A, NR_B, NR_S].iter().enumerate() {
        for (j, t) in table.iter().enumerate() {
            if b.contains(t) {
                m[k] |= 1 << j;
            }
        }
    }
    m
}

/// The NR read: the write format is `mA,0,mB,mS` and the read keeps the middle zero: four fields A,0,B,S; three
/// A,0,B (S = 0). An old decoder passed three fields through, put B into the super table and showed n82.
pub fn nr_read_masks(raw: &str) -> Option<[u32; 3]> {
    let f: Vec<u32> = raw.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    match f.len() {
        n if n >= 4 => Some([f[0], f[2], f[3]]),
        3 => Some([f[0], f[2], 0]),
        _ => None,
    }
}

pub fn nr_bands(m: [u32; 3]) -> Vec<u32> {
    let mut out = Vec::new();
    for (k, table) in [NR_A, NR_B, NR_S].iter().enumerate() {
        for (j, t) in table.iter().enumerate() {
            if m[k] >> j & 1 == 1 {
                out.push(*t);
            }
        }
    }
    out
}

/// The NR write command for a band list (empty: automatic).
pub fn nr_write(b: &[u32]) -> String {
    if b.is_empty() {
        return "AT+SPLBAND=2,0,0,0,0".into();
    }
    let m = nr_masks(b);
    format!("AT+SPLBAND=2,{},0,{},{}", m[0], m[1], m[2])
}

/// The LTE write command for a band list (empty: automatic).
pub fn lte_write(b: &[u32]) -> String {
    if b.is_empty() {
        return "AT+SPLBAND=1,0,0,0,0,0".into();
    }
    let m = lte_masks(b);
    format!("AT+SPLBAND=1,{},{},{},{},{}", m[0], m[1], m[2], m[3], m[4])
}

/// `bands` holds every band of the LTE table: what a cleared mask reads back as (a superset counts: a firmware with
/// a larger table is still automatic).
pub fn lte_full(bands: &[u32]) -> bool {
    LTE_TABLE.iter().all(|b| bands.contains(b))
}

/// The network mode of a work mode and the SA flag (ufi_tools toAppMode, slot 0). An unknown work mode reads as
/// `auto`; 152 is `auto152`, the automatic mode without 2G (FINDINGS 198).
pub fn mode_label(work: Option<u32>, gran: Option<u32>) -> &'static str {
    match work {
        Some(3) => "4g",
        Some(152) => "auto152",
        Some(128 | 131) if gran == Some(1) => "sa",
        Some(128 | 131) => "nsa",
        _ => "auto",
    }
}

/// The work mode and SA flag to write for a mode (ufi_tools modeConfigForSlot0).
pub fn mode_pair(label: &str) -> (u32, u32) {
    match label {
        "4g" => (3, 1),
        "sa" => (128, 1),
        "nsa" => (131, 0),
        "auto152" => (152, 1),
        _ => (134, 1),
    }
}

/// A locked cell: `nr:627264,393` or `lte:1650,211`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub nr: bool,
    pub arfcn: u32,
    pub pci: u32,
}

impl Cell {
    /// One pair, the ARFCN up to 7 digits and the PCI up to 4.
    pub fn parse(s: &str) -> Option<Cell> {
        let (rat, pair) = s.split_once(':')?;
        let nr = match rat {
            "nr" => true,
            "lte" => false,
            _ => return None,
        };
        let (fr, pci) = pair.split_once(',')?;
        let digits = |x: &str, max: usize| !x.is_empty() && x.len() <= max && x.bytes().all(|b| b.is_ascii_digit());
        if !digits(fr, 7) || !digits(pci, 4) {
            return None;
        }
        Some(Cell { nr, arfcn: fr.parse().ok()?, pci: pci.parse().ok()? })
    }

    /// The SPFORCEFRQ family: 16 NR, 12 LTE.
    pub const fn family(&self) -> u32 {
        if self.nr { 16 } else { 12 }
    }

    pub fn write(&self) -> String {
        format!("AT+SPFORCEFRQ={},6,{},{}", self.family(), self.arfcn, self.pci)
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{},{}", if self.nr { "nr" } else { "lte" }, self.arfcn, self.pci)
    }
}

/// The cells of a `+SPFORCEFRQ: 12,3,fr,pci[,fr,pci...]` read (several may be locked).
pub fn cells_of(raw: &str, nr: bool) -> Vec<Cell> {
    let f: Vec<&str> = raw.split(',').map(str::trim).collect();
    let mut out = Vec::new();
    let mut i = 2;
    while i + 1 < f.len() {
        if let (Ok(arfcn), Ok(pci)) = (f[i].parse(), f[i + 1].parse()) {
            out.push(Cell { nr, arfcn, pci });
        }
        i += 2;
    }
    out
}

/// What a lock change is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mode,
    Endc,
    Lte,
    Nr,
    Cell,
    AutoApply,
}

impl Kind {
    pub const WORDS: &'static [&'static str] = &["mode", "endc", "lte", "nr", "cell", "auto_apply"];

    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "mode" => Kind::Mode,
            "endc" => Kind::Endc,
            "lte" => Kind::Lte,
            "nr" => Kind::Nr,
            "cell" => Kind::Cell,
            "auto_apply" => Kind::AutoApply,
            _ => return None,
        })
    }

    /// The name of its saved setting.
    pub const fn key(self) -> &'static str {
        match self {
            Kind::Mode => "mode",
            Kind::Endc => "endc",
            Kind::Lte => "lte",
            Kind::Nr => "nr",
            Kind::Cell => "cell",
            Kind::AutoApply => "auto_apply",
        }
    }
}

/// The values each kind takes, exactly as the rpcd backend checked them: bands are up to 32 numbers of up to 3
/// digits (empty = automatic); a cell is one RAT:ARFCN,PCI (NR-ARFCN up to 7 digits, PCI up to 4).
pub fn validate(kind: Kind, value: &str) -> Result<()> {
    let re = match kind {
        Kind::Mode => "auto|auto152|4g|sa|nsa",
        Kind::Endc | Kind::AutoApply => "on|off",
        Kind::Lte | Kind::Nr if value.is_empty() => return Ok(()),
        Kind::Lte | Kind::Nr => "[0-9]{1,3}(,[0-9]{1,3}){0,31}",
        Kind::Cell => "auto|off|off-nr|off-lte|(nr|lte):[0-9]{1,7},[0-9]{1,4}",
    };
    check_word("value", value, re, 127)
        .map_err(|_| Error::field(f50_core::Code::BadValue, "value", "Invalid value for this lock kind"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lte_masks_match_the_device_readback() {
        // bands 1,3,5,8,34,38,39,40,41 read back as 0,482,0,149,0 on the F50
        assert_eq!(lte_masks(LTE_TABLE), [0, 482, 0, 149, 0]);
        assert_eq!(canonical(&lte_bands("0,482,0,149,0").unwrap()), LTE_TABLE);
        assert!(lte_full(&lte_bands("0,482,0,149,0").unwrap()));
        assert_eq!(lte_write(&[1, 3]), "AT+SPLBAND=1,0,0,0,5,0");
        assert_eq!(lte_write(&[]), "AT+SPLBAND=1,0,0,0,0,0");
        assert!(lte_bands("1,2,3").is_none());
    }

    #[test]
    fn nr_masks_round_trip() {
        let m = nr_masks(&[78, 41]);
        assert_eq!(m, [0, 0b1_0001_0000, 0]);
        assert_eq!(nr_write(&[78, 41]), "AT+SPLBAND=2,0,0,272,0");
        assert_eq!(canonical(&nr_bands(nr_read_masks("0,0,272,0").unwrap())), vec![41, 78]);
        // three fields: A,0,B
        assert_eq!(nr_read_masks("553,0,272"), Some([553, 272, 0]));
        assert_eq!(canonical(&nr_bands([553, 272, 0])), vec![1, 5, 8, 28, 41, 78]);
    }

    #[test]
    fn modes() {
        assert_eq!(mode_label(Some(134), Some(1)), "auto");
        assert_eq!(mode_label(Some(152), Some(1)), "auto152");
        assert_eq!(mode_label(Some(128), Some(1)), "sa");
        assert_eq!(mode_label(Some(131), Some(0)), "nsa");
        assert_eq!(mode_label(Some(3), None), "4g");
        assert_eq!(mode_label(Some(999), None), "auto");
        assert_eq!(mode_pair("nsa"), (131, 0));
        assert_eq!(mode_pair("auto"), (134, 1));
    }

    #[test]
    fn cells() {
        let c = Cell::parse("nr:627264,393").unwrap();
        assert_eq!(c.write(), "AT+SPFORCEFRQ=16,6,627264,393");
        assert_eq!(c.to_string(), "nr:627264,393");
        assert!(Cell::parse("lte:12345678,1").is_none());
        assert!(Cell::parse("gsm:1,1").is_none());
        assert!(Cell::parse("lte:1").is_none());
        assert_eq!(cells_of("12,3,100,69", false), vec![Cell { nr: false, arfcn: 100, pci: 69 }]);
        assert!(cells_of("16,3", true).is_empty());
    }

    #[test]
    fn values() {
        assert!(validate(Kind::Mode, "auto152").is_ok());
        assert!(validate(Kind::Mode, "5g").is_err());
        assert!(validate(Kind::Lte, "").is_ok());
        assert!(validate(Kind::Lte, "1,3,5").is_ok());
        assert!(validate(Kind::Nr, "1,,3").is_err());
        assert!(validate(Kind::Nr, "1234").is_err());
        assert!(validate(Kind::Cell, "nr:627264,393").is_ok());
        assert!(validate(Kind::Cell, "off-lte").is_ok());
        assert!(validate(Kind::Cell, "nr:1,12345").is_err());
        assert!(validate(Kind::Endc, "1").is_err());
    }
}
