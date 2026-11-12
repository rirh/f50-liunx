//! Local days and billing cycles, as day numbers (1970-01-01 is 0). The UTC offset is given, never looked up here
//! (see [`super::tz`]). A billing cycle starts on its reset day, or on the month's last day when the month is
//! shorter (reset day 31 in February is the 28th or 29th).

/// The day number of a civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a day number.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `YYYY-MM-DD` of a day number.
pub fn date(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The day number of `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s[..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..].parse().ok()?;
    ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then(|| days_from_civil(y, m, d))
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The local day of a Unix time at UTC offset `off` (seconds).
pub fn day_of(t: i64, off: i64) -> i64 {
    (t + off).div_euclid(86_400)
}

/// The next local midnight after `t`.
pub fn next_midnight(t: i64, off: i64) -> i64 {
    t - (t + off).rem_euclid(86_400) + 86_400
}

fn reset_in(reset: u32, y: i64, m: u32) -> u32 {
    reset.min(days_in_month(y, m))
}

/// The first day of the billing cycle `day` is in.
pub fn cycle_start(day: i64, reset: u32) -> i64 {
    let (y, m, d) = civil_from_days(day);
    if d >= reset_in(reset, y, m) {
        return days_from_civil(y, m, reset_in(reset, y, m));
    }
    let (y, m) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    days_from_civil(y, m, reset_in(reset, y, m))
}

/// The first day of the cycle after the one starting on `start`.
pub fn cycle_next(start: i64, reset: u32) -> i64 {
    let (y, m, _) = civil_from_days(start);
    let (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(y, m, reset_in(reset, y, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(date(days_from_civil(2024, 2, 29)), "2024-02-29");
        assert_eq!(parse_date("2026-11-12").map(date).as_deref(), Some("2026-11-12"));
        assert_eq!(parse_date("2025-02-29"), None);
        for z in [-1000, 0, 19_000, 20_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
    }

    #[test]
    fn cycles_and_midnights() {
        let d = |s| parse_date(s).expect("date");
        assert_eq!(cycle_start(d("2026-03-10"), 15), d("2026-02-15"));
        assert_eq!(cycle_start(d("2026-03-15"), 15), d("2026-03-15"));
        assert_eq!(cycle_start(d("2026-01-05"), 15), d("2025-12-15"));
        assert_eq!(cycle_start(d("2026-02-28"), 31), d("2026-02-28"));
        assert_eq!(cycle_next(d("2026-01-31"), 31), d("2026-02-28"));
        assert_eq!(cycle_next(d("2026-12-01"), 1), d("2027-01-01"));
        // 23:30 at +03:00 is the next local day's 02:30
        let t = days_from_civil(2026, 1, 1) * 86_400 + 23 * 3600 + 1800;
        assert_eq!(date(day_of(t, 10_800)), "2026-01-02");
        assert_eq!(next_midnight(t, 0), days_from_civil(2026, 1, 2) * 86_400);
    }
}
