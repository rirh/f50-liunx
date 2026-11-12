//! Which received parts are one long message.
//!
//! The reference number alone does not say it: it is eight bits wide in most headers, and senders reuse it (some send
//! every long message under the same one). So the parts of one message are those with the same sender, reference
//! and total whose service centre timestamps are within ten minutes of the group's first part, and no two of them
//! claim one part number with different text (docs/research/FINDINGS.md 63; the idea of keying by more than the
//! reference is kanoqwq's, clean-tf-7.2 a5bd3e06). A port of sms-pdu.awk's group mode.

use crate::pdu::Concat;

/// Parts whose timestamps are further apart than this are two messages.
pub(crate) const GROUP_WINDOW: i64 = 600;

/// Seconds on one time line for `YYYY-MM-DD HH:MM:SS` of 2000-2099 (every fourth year a leap year, which holds
/// there); `None` for anything else. No zone: two stamps are only compared to say "the same few minutes".
pub(crate) fn ts_secs(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let shape = b.len() == 19
        && b.iter().enumerate().all(|(i, &c)| match i {
            4 | 7 => c == b'-',
            10 => c == b' ',
            13 | 16 => c == b':',
            _ => c.is_ascii_digit(),
        });
    if !shape || !s.starts_with("20") {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|v| v.parse::<i64>().ok());
    let (y, m, d) = (n(2..4)?, n(5..7)?, n(8..10)?);
    if !(1..=12).contains(&m) {
        return None;
    }
    const BEFORE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut days = y * 365 + (y + 3) / 4 + BEFORE[(m - 1) as usize] + d - 1;
    if m > 2 && y % 4 == 0 {
        days += 1;
    }
    Some(days * 86400 + n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?)
}

/// Two timestamps within [`GROUP_WINDOW`]; one that cannot be read is near everything (the other things a group is
/// keyed by still have to match).
pub(crate) fn ts_near(a: &str, b: &str) -> bool {
    match (ts_secs(a), ts_secs(b)) {
        (Some(x), Some(y)) => (x - y).abs() <= GROUP_WINDOW,
        _ => true,
    }
}

/// One decoded part, as the grouping sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rec<'a> {
    pub sender: &'a str,
    pub ts: &'a str,
    pub concat: Option<Concat>,
    pub text: &'a str,
}

/// The records of one message, in the order they came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub members: Vec<usize>,
    /// `multi:SENDER:REF:TOTAL` for a long message (the shell pool's key without its time), `None` for one of its own.
    pub base: Option<String>,
    pub total: u8,
}

/// Every record into the first group of its sender, reference and total that is of its time and has no other text
/// under its part number; the groups in the order their first record came.
pub(crate) fn group(recs: &[Rec<'_>]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, r) in recs.iter().enumerate() {
        let Some(c) = r.concat else {
            groups.push(Group { members: vec![i], base: None, total: 1 });
            continue;
        };
        let base = format!("multi:{}:{}:{}", r.sender, c.reference, c.total);
        let fits = |g: &Group| {
            g.base.as_deref() == Some(base.as_str())
                && g.members.first().is_some_and(|&f| ts_near(r.ts, recs[f].ts))
                && !g
                    .members
                    .iter()
                    .any(|&m| recs[m].concat.is_some_and(|mc| mc.part == c.part) && recs[m].text != r.text)
        };
        match groups.iter_mut().find(|g| fits(g)) {
            Some(g) => g.members.push(i),
            None => groups.push(Group { members: vec![i], base: Some(base), total: c.total }),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_is_ten_minutes_on_the_calendar() {
        assert!(ts_near("2026-10-13 09:41:07", "2026-10-13 09:51:07"));
        assert!(!ts_near("2026-10-13 09:41:07", "2026-10-13 09:51:08"));
        assert!(ts_near("2026-12-31 23:59:59", "2027-01-01 00:00:00"));
        assert!(ts_near("2024-02-28 23:59:00", "2024-02-29 00:09:00"));
        assert!(ts_near("2024-02-29 23:59:00", "2024-03-01 00:09:00"));
        assert!(ts_near("2025-02-28 23:59:00", "2025-03-01 00:09:00"));
        assert!(!ts_near("2026-10-13 09:41:07", "2026-10-14 09:41:07"));
        assert_eq!(ts_secs("2000-01-01 00:00:00"), Some(0));
        // a timestamp that cannot be read is near everything, and decides nothing
        for bad in ["", "2026-13-01 00:00:00", "20:6-10-13 09:41:07", "x"] {
            assert!(ts_near(bad, "2026-10-13 09:41:07"));
        }
    }

    fn rec<'a>(sender: &'a str, ts: &'a str, r: Option<(u16, u8, u8)>, text: &'a str) -> Rec<'a> {
        Rec { sender, ts, concat: r.map(|(reference, total, part)| Concat { reference, total, part }), text }
    }

    #[test]
    fn groups_as_sms_pdu_awk_does() {
        let t = |s: &str| format!("2026-10-13 09:00:0{s}");
        let (t0, t1, t2, t3) = (t("0"), t("1"), t("2"), t("3"));
        let recs = [
            rec("A", &t1, Some((7, 2, 1)), "a1"),
            rec("B", &t1, Some((7, 2, 1)), "b1"), // another sender
            rec("A", &t2, None, "single"),
            rec("A", &t2, Some((7, 3, 1)), "c1"), // another total
            rec("A", &t3, Some((7, 2, 2)), "a2"),
            rec("A", &t3, Some((7, 2, 2)), "a2"), // the second part, twice
            rec("A", "2026-10-13 09:10:02", Some((7, 2, 1)), "d1"), // ten minutes and a second later
            rec("A", &t0, Some((7, 2, 1)), "e1"), // part 1 again, another text
        ];
        let g = group(&recs);
        let members: Vec<Vec<usize>> = g.iter().map(|g| g.members.clone()).collect();
        assert_eq!(members, vec![vec![0, 4, 5], vec![1], vec![2], vec![3], vec![6], vec![7]]);
        assert_eq!(g[0].base.as_deref(), Some("multi:A:7:2"));
        assert_eq!(g[2].base, None);
        // one second, one reference, two messages
        let two = [rec("A", &t1, Some((0, 2, 1)), "x"), rec("A", &t1, Some((0, 2, 1)), "y")];
        assert_eq!(group(&two).len(), 2);
    }
}
