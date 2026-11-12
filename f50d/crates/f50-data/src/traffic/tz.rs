//! The local UTC offset, from OpenWrt's POSIX TZ string (`/etc/TZ`, which `system.@system[0].timezone` becomes:
//! `<+03>-3`, `CET-1CEST,M3.5.0,M10.5.0/3`). Days of the data count are local days; computing the offset here
//! keeps a program (`date +%z`) out of the once-a-minute round. Understood: a standard name and offset, an optional
//! daylight name and offset, and the `Mm.w.d[/time]`, `Jn` and `n` rules; anything else is UTC.

use super::cycle::{days_from_civil, days_in_month};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Month, week 1-5 (5: the last), weekday 0 (Sunday) - 6.
    M(u32, u32, u32),
    /// Day 1-365, February 29 never counted.
    J(u32),
    /// Day 0-365, February 29 counted.
    N(u32),
}

/// A parsed TZ: offsets in seconds east of UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tz {
    std: i64,
    dst: Option<(i64, Rule, i64, Rule, i64)>,
}

fn name(s: &str) -> Option<&str> {
    if let Some(r) = s.strip_prefix('<') {
        return r.find('>').map(|i| &r[i + 1..]);
    }
    let n = s.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(s.len());
    (n >= 3).then(|| &s[n..])
}

/// `[+-]hh[:mm[:ss]]` -> seconds (as written: west positive) and the rest.
fn hms(s: &str) -> Option<(i64, &str)> {
    let (sign, s) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let end = s.find(|c: char| !c.is_ascii_digit() && c != ':').unwrap_or(s.len());
    let mut total = 0i64;
    for (i, p) in s[..end].split(':').enumerate() {
        if i > 2 || p.is_empty() || p.len() > 3 {
            return None;
        }
        total += p.parse::<i64>().ok()? * [3600, 60, 1][i];
    }
    (end > 0).then_some((sign * total, &s[end..]))
}

fn rule(s: &str) -> Option<(Rule, i64)> {
    let (r, time) = match s.split_once('/') {
        Some((r, t)) => (r, hms(t).filter(|(_, rest)| rest.is_empty())?.0),
        None => (s, 7200),
    };
    let rule = if let Some(m) = r.strip_prefix('M') {
        let v: Vec<u32> = m.split('.').filter_map(|x| x.parse().ok()).collect();
        match v.as_slice() {
            [mo @ 1..=12, w @ 1..=5, d @ 0..=6] => Rule::M(*mo, *w, *d),
            _ => return None,
        }
    } else if let Some(j) = r.strip_prefix('J') {
        Rule::J(j.parse().ok().filter(|n| (1..=365).contains(n))?)
    } else {
        Rule::N(r.parse().ok().filter(|n| *n <= 365)?)
    };
    Some((rule, time))
}

impl Tz {
    pub fn parse(s: &str) -> Option<Tz> {
        let s = s.trim();
        let rest = name(s)?;
        let (west, rest) = hms(rest)?;
        let std = -west;
        if rest.is_empty() {
            return Some(Tz { std, dst: None });
        }
        let rest = name(rest)?;
        let (dst, rest) = match hms(rest) {
            Some((w, r)) => (-w, r),
            None => (std + 3600, rest),
        };
        let rules = rest.strip_prefix(',').unwrap_or("M3.2.0,M11.1.0");
        let (a, b) = rules.split_once(',')?;
        let ((r1, t1), (r2, t2)) = (rule(a)?, rule(b)?);
        Some(Tz { std, dst: Some((dst, r1, t1, r2, t2)) })
    }

    /// The offset in force at Unix time `t`.
    pub fn offset(&self, t: i64) -> i64 {
        let Some((dst, r1, t1, r2, t2)) = self.dst else { return self.std };
        let (y, _, _) = super::cycle::civil_from_days((t + self.std).div_euclid(86_400));
        // the start is given in standard time, the end in daylight time
        let start = start_of(y, r1) * 86_400 + t1 - self.std;
        let end = start_of(y, r2) * 86_400 + t2 - dst;
        let in_dst = if start < end { t >= start && t < end } else { t >= start || t < end };
        if in_dst { dst } else { self.std }
    }
}

/// The day number a rule names in year `y`.
fn start_of(y: i64, r: Rule) -> i64 {
    let jan1 = days_from_civil(y, 1, 1);
    match r {
        Rule::N(n) => jan1 + i64::from(n),
        Rule::J(n) => {
            let leap = days_in_month(y, 2) == 29;
            jan1 + i64::from(n) - 1 + i64::from(leap && n >= 60)
        }
        Rule::M(m, w, d) => {
            let first = days_from_civil(y, m, 1);
            // 1970-01-01 was a Thursday (4)
            let wd = (first + 4).rem_euclid(7);
            let mut day = first + (i64::from(d) - wd).rem_euclid(7) + 7 * (i64::from(w) - 1);
            while day >= first + i64::from(days_in_month(y, m)) {
                day -= 7;
            }
            day
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_daylight_zones() {
        assert_eq!(Tz::parse("<+03>-3").map(|z| z.offset(0)), Some(10_800));
        assert_eq!(Tz::parse("UTC0").map(|z| z.offset(0)), Some(0));
        assert_eq!(Tz::parse("IST-5:30").map(|z| z.offset(0)), Some(19_800));
        let cet = Tz::parse("CET-1CEST,M3.5.0,M10.5.0/3").expect("cet");
        let at = |y, m, d, h: i64| days_from_civil(y, m, d) * 86_400 + h * 3600;
        assert_eq!(cet.offset(at(2026, 1, 15, 12)), 3600);
        assert_eq!(cet.offset(at(2026, 7, 1, 12)), 7200);
        // 2026-03-29 01:00 UTC is 03:00 CEST; 00:59 UTC still CET
        assert_eq!(cet.offset(at(2026, 3, 29, 1)), 7200);
        assert_eq!(cet.offset(at(2026, 3, 29, 1) - 60), 3600);
        // 2026-10-25 01:00 UTC is 02:00 CET again
        assert_eq!(cet.offset(at(2026, 10, 25, 1)), 3600);
        let syd = Tz::parse("AEST-10AEDT,M10.1.0,M4.1.0/3").expect("syd");
        assert_eq!(syd.offset(at(2026, 1, 15, 0)), 39_600);
        assert_eq!(syd.offset(at(2026, 6, 15, 0)), 36_000);
        assert_eq!(Tz::parse("x"), None);
    }
}
