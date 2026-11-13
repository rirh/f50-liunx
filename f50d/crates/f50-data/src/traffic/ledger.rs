//! The count itself, and the rules that make every byte count once (f50-traffic's ledger, same text format):
//!
//! ```text
//! f50-traffic 1                    the format (`f50-traffic 1` before the rename: the line is not read)
//! since WALL                       counting started (the first reading with a right clock)
//! base BOOT IFINDEX DEV RX TX UP   the last reading of the counters, and its uptime
//! p BOOT UP RX TX                  pending: bytes counted while the clock was not right, per 15 min of uptime
//! d YYYY-MM-DD RX TX               one local day (800 are kept)
//! cut CYCLESTART BOOT OVERRIDE     the cap turned data off in that cycle, in that boot; 1: turned back on by hand
//! alert CYCLESTART warn|over       the last warning logged in that cycle
//! ```
//!
//! - The counters start at 0 when the interface is made: a new boot or ifindex, or a counter below the last
//!   reading, means new counters, all of them new bytes.
//! - The first reading (and one after the counted device changed) is only a base: what the device carried before
//!   cannot be put on a day honestly.
//! - The bytes of one interval go to the local days it covers, split at midnight in proportion to the time; the
//!   interval is measured with the uptime, so a clock jump neither stretches nor squeezes it.
//! - Until the clock is known to be right (no RTC), bytes wait by uptime in "pending" and count towards the cycle
//!   at once; once it is right they go to the days they belong to.
//!
//! The shell's daily quota, count modes and "used" corrections are not carried over; their lines are dropped.

use std::collections::BTreeMap;

use super::cycle::{date, day_of, next_midnight, parse_date};

const KEEP_DAYS: i64 = 800;
const PENDING_MAX: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub boot: String,
    pub idx: u64,
    pub dev: String,
    pub rx: u64,
    pub tx: u64,
    pub up: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub boot: String,
    pub up: u64,
    pub rx: u64,
    pub tx: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    pub cycle: i64,
    pub boot: String,
    pub overridden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Over,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Over => "over",
        }
    }
    fn parse(s: &str) -> Option<Level> {
        Some(match s {
            "ok" => Level::Ok,
            "warn" => Level::Warn,
            "over" => Level::Over,
            _ => return None,
        })
    }
}

/// One reading of the counters, with the clock.
#[derive(Debug, Clone)]
pub struct Reading {
    pub boot: String,
    /// `None` when the device is not there.
    pub counters: Option<(u64, String, u64, u64)>,
    pub up: u64,
    pub now: i64,
    pub off: i64,
    pub trusted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    pub since: Option<i64>,
    pub base: Option<Base>,
    pub pending: Vec<Pending>,
    /// day number -> (rx, tx)
    pub days: BTreeMap<i64, (u64, u64)>,
    pub cut: Option<Cut>,
    pub alert: Option<(i64, Level)>,
}

fn num<T: std::str::FromStr>(s: Option<&str>) -> Option<T> {
    s?.parse().ok()
}

impl Ledger {
    pub fn parse(text: &str) -> Ledger {
        let mut l = Ledger::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let g = |i: usize| f.get(i).copied();
            match (g(0), f.len()) {
                (Some("since"), 2) => l.since = num(g(1)),
                (Some("base"), 7) => {
                    if let (Some(idx), Some(rx), Some(tx), Some(up)) = (num(g(2)), num(g(4)), num(g(5)), num(g(6))) {
                        let (boot, dev) = (f[1].to_owned(), f[3].to_owned());
                        l.base = Some(Base { boot, idx, dev, rx, tx, up });
                    }
                }
                (Some("p"), 5) => {
                    if let (Some(up), Some(rx), Some(tx)) = (num(g(2)), num(g(3)), num(g(4))) {
                        l.pending.push(Pending { boot: f[1].to_owned(), up, rx, tx });
                    }
                }
                (Some("d"), 4) => {
                    if let (Some(d), Some(rx), Some(tx)) = (parse_date(f[1]), num(g(2)), num(g(3))) {
                        l.add_day(d, rx, tx);
                    }
                }
                (Some("cut"), 4) => {
                    if let Some(c) = parse_date(f[1]) {
                        l.cut = Some(Cut { cycle: c, boot: f[2].to_owned(), overridden: f[3] == "1" });
                    }
                }
                (Some("alert"), 3) => l.alert = parse_date(f[1]).zip(Level::parse(f[2])),
                _ => {}
            }
        }
        l
    }

    pub fn render(&self) -> String {
        let mut s = String::from("f50-traffic 1\n");
        if let Some(t) = self.since {
            s += &format!("since {t}\n");
        }
        if let Some(b) = &self.base {
            s += &format!("base {} {} {} {} {} {}\n", b.boot, b.idx, b.dev, b.rx, b.tx, b.up);
        }
        for p in &self.pending {
            s += &format!("p {} {} {} {}\n", p.boot, p.up, p.rx, p.tx);
        }
        for (d, (rx, tx)) in &self.days {
            s += &format!("d {} {rx} {tx}\n", date(*d));
        }
        if let Some(c) = &self.cut {
            s += &format!("cut {} {} {}\n", date(c.cycle), c.boot, u8::from(c.overridden));
        }
        if let Some((c, l)) = &self.alert {
            s += &format!("alert {} {}\n", date(*c), l.as_str());
        }
        s
    }

    fn add_day(&mut self, day: i64, rx: u64, tx: u64) {
        let e = self.days.entry(day).or_default();
        e.0 += rx;
        e.1 += tx;
    }

    /// Bytes counted over [t0, t1], shared out over the local days it covers by time; the rounding goes to the last
    /// day, so the sum is exact.
    fn add_span(&mut self, t0: i64, t1: i64, mut rx: u64, mut tx: u64, off: i64) {
        let mut a = t0.max(t1 - KEEP_DAYS * 86_400);
        loop {
            let b = next_midnight(a, off);
            if t1 <= a || b >= t1 {
                break;
            }
            let share = |v: u64| u64::try_from(i128::from(v) * i128::from(b - a) / i128::from(t1 - a)).unwrap_or(0);
            let (prx, ptx) = (share(rx), share(tx));
            self.add_day(day_of(a, off), prx, ptx);
            rx -= prx;
            tx -= ptx;
            a = b;
        }
        self.add_day(day_of(a, off), rx, tx);
    }

    fn add_pending(&mut self, boot: &str, up: u64, rx: u64, tx: u64) {
        let k = up / 900 * 900;
        let full = self.pending.len() >= PENDING_MAX;
        if let Some(last) = self.pending.last_mut()
            && (full || (last.boot == boot && last.up == k))
        {
            last.rx += rx;
            last.tx += tx;
            return;
        }
        self.pending.push(Pending { boot: boot.to_owned(), up: k, rx, tx });
    }

    /// Pending bytes dated once the clock is right: those of this boot by their uptime; older ones (a boot that
    /// never had a right clock) cannot be placed and go to today.
    fn date_pending(&mut self, r: &Reading) {
        for p in std::mem::take(&mut self.pending) {
            if p.boot == r.boot && p.up <= r.up {
                let t0 = r.now - i64::try_from(r.up - p.up).unwrap_or(0);
                self.add_span(t0, (t0 + 900).min(r.now), p.rx, p.tx, r.off);
            } else {
                self.add_day(day_of(r.now, r.off), p.rx, p.tx);
            }
        }
    }

    /// A reading taken now; returns the bytes it counted.
    pub fn sample(&mut self, r: &Reading) -> u64 {
        let mut counted = 0;
        if let (false, Some((idx, dev, rx, tx))) = (r.boot.is_empty(), &r.counters) {
            let base = Base { boot: r.boot.clone(), idx: *idx, dev: dev.clone(), rx: *rx, tx: *tx, up: r.up };
            match self.base.replace(base) {
                Some(b) if b.dev == *dev => {
                    let (drx, dtx, span) = if b.boot == r.boot && b.idx == *idx {
                        let d = |c: u64, o: u64| if c >= o { c - o } else { c };
                        (d(*rx, b.rx), d(*tx, b.tx), r.up.saturating_sub(b.up))
                    } else {
                        (*rx, *tx, if b.boot == r.boot { r.up.saturating_sub(b.up) } else { r.up })
                    };
                    counted = drx + dtx;
                    if counted > 0 {
                        if r.trusted {
                            self.add_span(r.now - i64::try_from(span).unwrap_or(0), r.now, drx, dtx, r.off);
                        } else {
                            self.add_pending(&r.boot, r.up, drx, dtx);
                        }
                    }
                }
                _ => {}
            }
        }
        if r.trusted && !self.pending.is_empty() {
            self.date_pending(r);
        }
        if r.trusted && self.since.is_none() {
            self.since = Some(r.now);
        }
        let oldest = day_of(r.now, r.off) - KEEP_DAYS;
        self.days.retain(|d, _| *d >= oldest);
        counted
    }

    /// Forget every count; counting goes on from now (the base stays).
    pub fn clear(&mut self, r: &Reading) {
        self.sample(r);
        *self = Ledger { since: r.trusted.then_some(r.now), base: self.base.take(), ..Ledger::default() };
    }

    pub fn pending_sum(&self) -> (u64, u64) {
        self.pending.iter().fold((0, 0), |(a, b), p| (a + p.rx, b + p.tx))
    }

    /// The bytes of the days in [start, end).
    pub fn sum(&self, start: i64, end: i64) -> (u64, u64) {
        self.days.range(start..end).fold((0, 0), |(a, b), (_, (rx, tx))| (a + rx, b + tx))
    }
}

/// What the cap asks for after a reading (cutstep of the shell, the cap only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapAction {
    pub cut: bool,
    pub restore: bool,
}

/// The cap's decision for this cycle. Data is cut once per cycle and again after a reboot; whoever turns it back
/// on in the same boot keeps it on. It comes back by itself at the next cycle, a higher cap or cut off - if the cap
/// was what turned it off. `data_up`: the interface is not down on purpose.
pub fn cap_step(l: &mut Ledger, cycle: i64, boot: &str, want: bool, data_up: bool) -> CapAction {
    let mut a = CapAction::default();
    let mut rest = false;
    match l.cut.as_mut() {
        Some(c) if c.cycle != cycle || !want => {
            rest = !c.overridden && !data_up;
            l.cut = None;
        }
        Some(c) if !c.overridden => {
            if c.boot == boot {
                if data_up {
                    c.overridden = true;
                }
            } else {
                a.cut = data_up;
                c.boot = boot.to_owned();
            }
        }
        _ => {}
    }
    if want && l.cut.is_none() {
        l.cut = Some(Cut { cycle, boot: boot.to_owned(), overridden: false });
        a.cut = data_up;
    }
    a.restore = rest && !a.cut && l.cut.as_ref().is_none_or(|c| c.overridden);
    a
}

/// The level of `used` against `cap` (0: no cap) with the warning at `warn` percent.
pub fn level(used: u64, cap: u64, warn: u64) -> Level {
    if cap == 0 {
        Level::Ok
    } else if used >= cap {
        Level::Over
    } else if u128::from(used) * 100 >= u128::from(cap) * u128::from(warn) {
        Level::Warn
    } else {
        Level::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::super::cycle::days_from_civil;
    use super::*;

    fn reading(rx: u64, tx: u64, up: u64, now: i64, trusted: bool) -> Reading {
        Reading { boot: "b1".into(), counters: Some((5, "sipa_eth0".into(), rx, tx)), up, now, off: 0, trusted }
    }

    #[test]
    fn every_byte_counts_once() {
        let day = days_from_civil(2026, 5, 1);
        let t = day * 86_400 + 86_340; // 23:59
        let mut l = Ledger::default();
        assert_eq!(l.sample(&reading(1000, 1000, 100, t, true)), 0, "the first reading is a base");
        // 120 s later, across midnight: half and half
        assert_eq!(l.sample(&reading(2000, 1000, 220, t + 120, true)), 1000);
        assert_eq!(l.days.get(&day), Some(&(500, 0)));
        assert_eq!(l.days.get(&(day + 1)), Some(&(500, 0)));
        // new counters (lower than the last reading): all new bytes
        l.sample(&reading(10, 0, 280, t + 180, true));
        assert_eq!(l.sum(day, day + 2), (1010, 0));
        let back = Ledger::parse(&l.render());
        assert_eq!(back, l);
    }

    #[test]
    fn pending_until_the_clock_is_right() {
        let now = days_from_civil(2026, 5, 2) * 86_400 + 3600;
        let mut l = Ledger::default();
        l.sample(&reading(0, 0, 100, 0, false));
        l.sample(&reading(100, 50, 160, 60, false));
        assert_eq!(l.pending_sum(), (100, 50));
        assert!(l.days.is_empty() && l.since.is_none());
        l.sample(&reading(100, 50, 200, now, true));
        assert!(l.pending.is_empty());
        assert_eq!(l.sum(0, i64::MAX), (100, 50));
        assert_eq!(l.since, Some(now));
    }

    #[test]
    fn the_cap_cuts_once_and_respects_a_hand() {
        let mut l = Ledger::default();
        assert_eq!(cap_step(&mut l, 10, "b1", true, true), CapAction { cut: true, restore: false });
        // turned back on in the same boot: stays on
        assert_eq!(cap_step(&mut l, 10, "b1", true, true), CapAction::default());
        assert!(l.cut.as_ref().is_some_and(|c| c.overridden));
        let mut l = Ledger::default();
        cap_step(&mut l, 10, "b1", true, true);
        // still off (data down), then a reboot: cut again only if somebody turned it on
        assert_eq!(cap_step(&mut l, 10, "b1", true, false), CapAction::default());
        assert_eq!(cap_step(&mut l, 10, "b2", true, true), CapAction { cut: true, restore: false });
        // the next cycle, data still down by us: back on
        assert_eq!(cap_step(&mut l, 11, "b2", false, false), CapAction { cut: false, restore: true });
        assert_eq!(level(90, 100, 90), Level::Warn);
        assert_eq!(level(100, 100, 90), Level::Over);
        assert_eq!(level(5, 0, 90), Level::Ok);
    }
}
