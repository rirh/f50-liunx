//! Data usage: how much the bearer carried per local day and per billing cycle, with an optional monthly cap
//! (mu300-traffic). Counted are the bytes of the cellular device (netifd's l3 device of the WAN: `sipa_eth0` with the
//! tray card, `sipa_eth8` with the internal one), both ways - what the operator sees. The modem is never asked.
//!
//! - [`ledger`]: the count and its every-byte-once rules; [`cycle`]: days and cycles; [`tz`]: the local offset;
//! - [`store`]: RAM and flash copies, the settings file; [`sampler`]: the once-a-minute round, the cap's cut.

pub mod cycle;
pub mod ledger;
pub mod sampler;
pub mod store;
pub mod tz;

use std::sync::{Mutex, MutexGuard, PoisonError};

use f50_core::{Error, Result};
use f50_platform::Platform;
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::DataConfig;
use cycle::{cycle_next, cycle_start, date, day_of};
use ledger::{Ledger, level};
use store::{CAP_RE, Conf, RESET_DAY_RE, Store, WARN_RE, plan_ok};

/// How many days and cycles `get` reports.
const DAYS: usize = 62;
const CYCLES: usize = 12;

#[derive(Debug, Default)]
pub(crate) struct Counts {
    pub ledger: Ledger,
    pub conf: Conf,
    /// The device counted in the last round, and whether it was there.
    pub device: String,
    pub available: bool,
    pub trusted: bool,
}

/// The count, shared by the sampler and the methods.
#[derive(Debug)]
pub struct Traffic {
    pub(crate) store: Store,
    counts: Mutex<Counts>,
    /// Wakes the sampler for a round now (a setting changed).
    pub(crate) wake: Notify,
}

impl Traffic {
    pub(crate) fn new(platform: &Platform, cfg: &DataConfig) -> Result<Traffic> {
        let store = Store::new(platform.paths());
        let ledger = store.load_ledger().map(|t| Ledger::parse(&t)).unwrap_or_default();
        let conf = store.load_conf();
        let counts = Counts { ledger, conf, device: cfg.device.clone(), ..Counts::default() };
        Ok(Traffic { store, counts: Mutex::new(counts), wake: Notify::new() })
    }

    pub(crate) fn counts(&self) -> MutexGuard<'_, Counts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The count as `f50.traffic get` reports it (as of the last round).
    pub fn view(&self, now: i64, off: i64) -> Value {
        let c = self.counts();
        let (l, conf) = (&c.ledger, &c.conf);
        let today = day_of(now, off);
        let cs = cycle_start(today, conf.reset_day);
        let ce = cycle_next(cs, conf.reset_day);
        let (prx, ptx) = l.pending_sum();
        let (crx, ctx) = l.sum(cs, ce);
        let (crx, ctx) = (crx + prx, ctx + ptx);
        let used = crx + ctx;
        let lvl = level(used, conf.cap, conf.warn);
        let (trx, ttx) = l.days.get(&today).copied().unwrap_or_default();
        let days: Vec<Value> = l
            .days
            .range(..=today)
            .rev()
            .take(DAYS)
            .map(|(d, (rx, tx))| json!({"date": date(*d), "rx": rx, "tx": tx}))
            .collect();
        let first = l.days.keys().next().copied();
        let mut cycles = Vec::new();
        let mut s = cs;
        for i in 0..CYCLES {
            let next = cycle_next(s, conf.reset_day);
            if i > 0 && first.is_none_or(|f| next <= f) {
                break;
            }
            let (mut rx, mut tx) = l.sum(s, next);
            if s == cs {
                rx += prx;
                tx += ptx;
            }
            cycles.push(json!({"start": date(s), "last": date(next - 1), "rx": rx, "tx": tx, "used": rx + tx}));
            s = cycle_start(s - 1, conf.reset_day);
        }
        json!({
            "device": c.device,
            "available": c.available,
            "clock": c.trusted,
            "since": l.since,
            "now": now,
            "reset_day": conf.reset_day,
            "plan": conf.plan,
            "today": {"date": date(today), "rx": trx, "tx": ttx},
            "pending": {"rx": prx, "tx": ptx},
            "cycle": {"start": date(cs), "last": date(ce - 1), "days_left": ce - today, "rx": crx, "tx": ctx,
                "used": used},
            "cap": {"bytes": conf.cap, "warn": conf.warn, "cut": conf.cut,
                "level": if conf.cap > 0 { lvl.as_str() } else { "none" },
                "cut_active": l.cut.as_ref().is_some_and(|k| k.cycle == cs && !k.overridden)},
            "days": days,
            "cycles": cycles,
        })
    }

    /// A setting changed with the checks of the panel's adapter (`traffic set`); `clear` is the sampler's.
    pub(crate) fn set_conf(&self, key: &str, value: &str) -> Result<()> {
        let bad = || Error::field(f50_core::Code::BadValue, "value", "Invalid value for this setting");
        let one = |re: &'static str| f50_core::rpc::matches(re, value).then_some(()).ok_or_else(bad);
        let mut conf = self.counts().conf.clone();
        match key {
            "reset_day" => {
                one(RESET_DAY_RE)?;
                conf.reset_day = value.parse().map_err(|_| bad())?;
            }
            "cap" => {
                one(CAP_RE)?;
                conf.cap = value.parse().map_err(|_| bad())?;
            }
            "warn" => {
                one(WARN_RE)?;
                conf.warn = value.parse().map_err(|_| bad())?;
            }
            "cut" => {
                one("on|off")?;
                conf.cut = value == "on";
            }
            "plan" => {
                if !plan_ok(value) {
                    return Err(bad());
                }
                value.clone_into(&mut conf.plan);
            }
            _ => return Err(Error::field(f50_core::Code::BadValue, "key", "Unknown data usage setting")),
        }
        self.store.save_conf(&conf).map_err(|e| e.context("The setting could not be saved"))?;
        self.counts().conf = conf;
        self.wake.notify_one();
        Ok(())
    }
}
