//! The once-a-minute round of the data count: the device's counters read from sysfs, the ledger updated in memory
//! and in RAM, the cap acted on; the flash copy written every 30 minutes, after 1 GiB more but not within 5 minutes
//! of the last write, after `clear`, and at shutdown. A power cut loses what was counted since the last write, never
//! more; nothing is ever counted twice.
//!
//! The clock is right (bytes go to their days) when `/var/run/f50d/clock-synced` exists (the binary's NTP hook
//! writes it: [`crate::Data::clock_synced`]), with `CLOCK=trust`, or after six hours of uptime without NTP.
//!
//! The cap: at the warning percentage a warning in the log; with `cut` on, data goes off at the cap (`ifdown`) and
//! comes back at the next cycle, a higher cap or cut off ([`super::ledger::cap_step`]).

use std::time::Duration;

use f50_core::time::unix_now;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::cycle::{cycle_next, cycle_start, day_of};
use super::ledger::{Level, Reading, cap_step, level};
use super::tz::Tz;
use crate::Data;
use crate::netifd::IfStatus;

const CHECKPOINT: Duration = Duration::from_secs(1800);
const CHECKPOINT_BYTES: u64 = 1 << 30;
const CHECKPOINT_MIN: Duration = Duration::from_secs(300);
const TRUST_AFTER: u64 = 6 * 3600;

/// The local UTC offset now.
pub(crate) fn offset(data: &Data, now: i64) -> i64 {
    data.platform().sysfs().read("/etc/TZ").and_then(|s| Tz::parse(&s)).map_or(0, |z| z.offset(now))
}

/// A reading of the counted device now.
pub(crate) async fn read(data: &Data) -> Reading {
    let p = data.platform();
    let sys = p.sysfs();
    let up: u64 = sys.read("/proc/uptime").and_then(|u| u.split('.').next()?.parse().ok()).unwrap_or(0);
    let boot =
        sys.read("/proc/sys/kernel/random/boot_id").filter(|b| b.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    let l3 = IfStatus::read(p, &data.config().interface).await.ok().flatten().and_then(|s| s.l3_device);
    let dev = match l3.filter(|d| d.starts_with("sipa_eth")) {
        Some(d) => d,
        None => {
            let c = data.traffic().counts();
            c.ledger.base.as_ref().map_or_else(|| data.device(), |b| b.dev.clone())
        }
    };
    let n = |name: &str| sys.read_num::<u64>(format!("/sys/class/net/{dev}/{name}"));
    let counters = match (n("ifindex"), n("statistics/rx_bytes"), n("statistics/tx_bytes")) {
        (Some(i), Some(rx), Some(tx)) => Some((i, dev.clone(), rx, tx)),
        _ => None,
    };
    let available =
        counters.is_some() && sys.read(format!("/sys/class/net/{dev}/operstate")).as_deref() != Some("down");
    let now = i64::try_from(unix_now()).unwrap_or(0);
    let synced = p.paths().run().join("clock-synced").exists();
    let trusted = synced || up >= TRUST_AFTER || data.traffic().counts().conf.trust_clock;
    {
        let mut c = data.traffic().counts();
        c.device = dev;
        c.available = available;
        c.trusted = trusted;
    }
    Reading { boot: boot.unwrap_or_default(), counters, up, now, off: offset(data, now), trusted }
}

#[derive(Debug)]
struct Saves {
    unsaved: u64,
    last: Instant,
}

/// The sampler task.
pub(crate) async fn run(data: Data, shutdown: CancellationToken) {
    let mut saves = Saves { unsaved: 0, last: Instant::now() };
    round(&data, &mut saves).await;
    save_disk(&data, &mut saves).await;
    let every = data.config().traffic_interval;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(every) => {}
            () = data.traffic().wake.notified() => {}
        }
        round(&data, &mut saves).await;
        let since = saves.last.elapsed();
        if since >= CHECKPOINT || (saves.unsaved >= CHECKPOINT_BYTES && since >= CHECKPOINT_MIN) {
            save_disk(&data, &mut saves).await;
        }
    }
    // a clean stop: one more reading, onto the flash
    round(&data, &mut saves).await;
    save_disk(&data, &mut saves).await;
}

async fn save_disk(data: &Data, saves: &mut Saves) {
    let text = data.traffic().counts().ledger.render();
    let store = data.traffic().store.clone();
    match tokio::task::spawn_blocking(move || store.save_disk(&text)).await {
        Ok(Ok(())) => {
            saves.unsaved = 0;
            saves.last = Instant::now();
        }
        Ok(Err(e)) => tracing::warn!(error = %e, "the data count could not be saved"),
        Err(e) => tracing::warn!(error = %e, "the data count could not be saved"),
    }
}

/// `f50.traffic set clear`: every count forgotten, counting goes on from now; kept on flash at once.
pub(crate) async fn clear(data: &Data) -> f50_core::Result<()> {
    let r = read(data).await;
    let text = {
        let mut c = data.traffic().counts();
        c.ledger.clear(&r);
        c.ledger.render()
    };
    let t = data.traffic();
    t.store.save_live(&text)?;
    t.store.save_disk(&text).map_err(|e| e.context("the data count could not be saved"))
}

async fn round(data: &Data, saves: &mut Saves) {
    let r = read(data).await;
    let interface = data.config().interface.clone();
    let data_up = IfStatus::read(data.platform(), &interface).await.ok().flatten().is_some_and(|s| s.autostart);
    let (text, action, alert, used, cap, warn) = {
        let mut c = data.traffic().counts();
        let counted = c.ledger.sample(&r);
        saves.unsaved += counted;
        let conf = c.conf.clone();
        let today = day_of(r.now, r.off);
        let cs = cycle_start(today, conf.reset_day);
        let (rx, tx) = c.ledger.sum(cs, cycle_next(cs, conf.reset_day));
        let (prx, ptx) = c.ledger.pending_sum();
        let used = rx + tx + prx + ptx;
        let lvl = level(used, conf.cap, conf.warn);
        let mut alert = None;
        let prev = c.ledger.alert.filter(|(k, _)| *k == cs).map_or(Level::Ok, |(_, l)| l);
        if lvl > prev {
            alert = Some(lvl);
        }
        c.ledger.alert = Some((cs, lvl));
        let want = conf.cap > 0 && conf.cut && lvl == Level::Over;
        let action = cap_step(&mut c.ledger, cs, &r.boot, want, data_up);
        (c.ledger.render(), action, alert, used, conf.cap, conf.warn)
    };
    if let Err(e) = data.traffic().store.save_live(&text) {
        tracing::debug!(error = %e, "the live data count");
    }
    match alert {
        Some(Level::Warn) => tracing::warn!("{warn}% of the monthly cap is used ({used} of {cap} bytes this cycle)"),
        Some(Level::Over) => tracing::warn!("the monthly cap is reached ({used} of {cap} bytes this cycle)"),
        _ => {}
    }
    let n = data.platform().netifd();
    if action.cut {
        tracing::warn!("turning mobile data off at the monthly cap (cut off, or a higher cap, keeps it on)");
        if let Err(e) = n.down(&interface).await {
            tracing::warn!(error = %e, "ifdown {interface}");
        }
    }
    if action.restore {
        tracing::info!("turning mobile data back on: the cap no longer holds it off");
        if let Err(e) = n.up(&interface).await {
            tracing::warn!(error = %e, "ifup {interface}");
        }
    }
}
