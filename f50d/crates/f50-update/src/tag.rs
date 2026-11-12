//! Release tags and their order (docs/research/FINDINGS.md 125): `vYYYY.MM.DD` up to v2026.10.27, where the "day"
//! was a counter, and from then on `vYYYY.MM.DD.N` (the UTC day of the build, N = 1..99). Every four-field tag is
//! newer than every three-field one; four-field tags compare by the date, then by N.

/// The pattern a request's `tag` field must match (checked again by [`tag_ok`]).
pub const TAG_RE: &str = r"v[0-9]{4}\.[0-9]{2}\.[0-9]{2}(\.[1-9][0-9]?)?";

/// `tag` is a release tag (month 01-12, day 01-31, N 1-99 without a leading zero).
pub fn tag_ok(tag: &str) -> bool {
    key(tag).is_some()
}

/// A number in the order of the releases: era 1 or 2, then YYYYMMDD, then N (0 for three-field tags).
pub fn key(tag: &str) -> Option<(u8, u32, u8)> {
    let rest = tag.strip_prefix('v')?;
    let parts: Vec<&str> = rest.split('.').collect();
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    if !(3..=4).contains(&parts.len()) || !digits(parts[0], 4) || !digits(parts[1], 2) || !digits(parts[2], 2) {
        return None;
    }
    let (y, m, d): (u32, u32, u32) = (parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let date = y * 10_000 + m * 100 + d;
    match parts.get(3) {
        None => Some((1, date, 0)),
        Some(n) => {
            let ok = (1..=2).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0');
            ok.then(|| n.parse().ok()).flatten().map(|n| (2, date, n))
        }
    }
}

/// Release `a` is newer than release `b`; false when either is no release tag.
pub fn newer(a: &str, b: &str) -> bool {
    matches!((key(a), key(b)), (Some(x), Some(y)) if x > y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_their_order() {
        for t in ["v2026.10.27", "v2026.10.27.1", "v2026.12.31.99", "v2025.01.01"] {
            assert!(tag_ok(t), "{t}");
            assert!(f50_core::rpc::matches(TAG_RE, t), "{t}");
        }
        for t in
            ["2026.10.27", "v2026.13.01", "v2026.10.00", "v2026.10.27.0", "v2026.10.27.01", "v2026.10.27.100", "vx"]
        {
            assert!(!tag_ok(t), "{t}");
        }
        assert!(newer("v2026.10.17.1", "v2026.10.31"));
        assert!(newer("v2026.10.17.10", "v2026.10.17.9"));
        assert!(newer("v2026.11.01.1", "v2026.10.31.99"));
        assert!(!newer("v2026.10.31", "v2026.10.31"));
        assert!(!newer("unknown", "v2026.10.31"));
        assert!(!newer("v2026.10.31", "unknown"));
    }
}
