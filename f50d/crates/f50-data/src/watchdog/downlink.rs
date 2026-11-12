//! A context that is up, with the right address, and still no downlink (FINDINGS 176, and 13g before it): the PDP
//! context active, the device on its address, packets leaving and nothing coming back for minutes. Every check of
//! the context passes, so only the device's own counters tell.
//!
//! **Stalled**: `rx_packets` grew by at most 2 (a stray router advertisement) while `tx_packets` grew by 20 or more,
//! over 90 s or longer. Nothing is sent to find out: only what the device sends anyway counts, and an idle link
//! (nothing leaving) is never taken for a dead one. Counters that went backwards (a new interface) are a new
//! reference.
//!
//! **The way back**, one step per stall, each judged again on a new reference:
//! 1. the data context again (netifd restarts the interface: AT+CGACT=0, the dial) - 13g's lost context;
//! 2. the radio off and on, then the dial: the modem builds its packet domain again;
//! 3. a reboot, only when nothing came back on the device in the whole boot (fewer than 200 packets received), at
//!    most 3 times in a row (counted in `/etc/f50d/downlink-reboots`, which goes once a boot has received 1000), and
//!    not with `option dl_reboot '0'`.
//!
//! Then 600 s until the next round of steps, doubled up to an hour: a link whose far end is gone reads the same, and
//! redialling it every few minutes would only churn. The shell's first step, restarting the VPN (what FINDINGS 176's
//! dead boots actually needed), went with the VPN; without it nothing here sends traffic or restarts a tunnel, and
//! the guards above keep the reboot from ever being a loop.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use f50_at::SendOpts;
use f50_core::fsx;
use tokio::time::Instant;

use crate::Data;
use crate::kernel::Kernel;
use crate::radio;

const RX_ALLOW: u64 = 2;
const STALL_TX: u64 = 20;
const STALL: Duration = Duration::from_secs(90);
const REBOOT_RX: u64 = 200;
const REBOOT_MAX: u32 = 3;
const GOOD_RX: u64 = 1000;
const BACKOFF: Duration = Duration::from_secs(600);
const BACKOFF_MAX: Duration = Duration::from_secs(3600);
const LOG_MAX: u64 = 32 * 1024;

#[derive(Debug, Clone, Copy)]
struct Reference {
    rx: u64,
    tx: u64,
    t: Instant,
}

/// The detector's memory across rounds.
#[derive(Debug, Default)]
pub(crate) struct Downlink {
    reference: Option<Reference>,
    level: u8,
    last: u8,
    next: Option<Instant>,
    backoff: Option<Duration>,
}

impl Downlink {
    /// A new reference once a dial is through.
    pub fn reset(&mut self) {
        self.reference = None;
    }

    /// The pure decision, for one reading.
    pub(crate) fn judge(&mut self, rx: u64, tx: u64, now: Instant) -> Judgement {
        let fresh = match self.reference {
            None => true,
            Some(r) => rx < r.rx || tx < r.tx || rx - r.rx > RX_ALLOW,
        };
        if fresh {
            let back = self.last > 0 && self.reference.is_some_and(|r| rx > r.rx);
            self.reference = Some(Reference { rx, tx, t: now });
            if back {
                let step = self.last;
                *self = Downlink { reference: self.reference, ..Downlink::default() };
                return Judgement::Back(step);
            }
            return Judgement::Fine;
        }
        let r = self.reference.unwrap_or(Reference { rx, tx, t: now });
        if now.duration_since(r.t) >= STALL && tx - r.tx >= STALL_TX { Judgement::Stalled } else { Judgement::Fine }
    }

    /// One round on a connected bearer: the counters, and a step when it is stalled.
    pub async fn check(&mut self, data: &Data, interface: &str, dev: &str, dl_reboot: bool) {
        let k = Kernel::new(data.platform());
        let (Some(rx), Some(tx)) = (k.counter(dev, "rx_packets"), k.counter(dev, "tx_packets")) else {
            self.reference = None;
            return;
        };
        match self.judge(rx, tx, Instant::now()) {
            Judgement::Fine => {
                if rx >= GOOD_RX {
                    let _ = std::fs::remove_file(reboots_file(data));
                }
            }
            Judgement::Back(step) => {
                tracing::info!("the downlink is back: {dev} receives again (after step {step})");
            }
            Judgement::Stalled => self.recover(data, interface, dev, rx, tx, dl_reboot).await,
        }
    }

    async fn recover(&mut self, data: &Data, interface: &str, dev: &str, rx: u64, tx: u64, dl_reboot: bool) {
        if self.next.is_some_and(|n| Instant::now() < n) {
            return;
        }
        let r = self.reference.unwrap_or(Reference { rx, tx, t: Instant::now() });
        let msg = format!(
            "downlink stalled: {dev} sent {} packets in {} s and received {}",
            tx - r.tx,
            r.t.elapsed().as_secs(),
            rx.saturating_sub(r.rx)
        );
        self.level += 1;
        evidence(data, dev, self.level, &msg).await;
        self.last = self.level;
        match self.level {
            1 => {
                tracing::warn!("{msg}; step 1: the data context again");
                super::restart(data, interface).await;
            }
            2 => {
                tracing::warn!("{msg}; step 2: the radio off and on, then the dial");
                radio::cycle(data.at(), data.modem()).await;
                super::restart(data, interface).await;
            }
            _ => {
                tracing::warn!("{msg} after every step");
                reboot(data, dev, rx, dl_reboot).await;
                let b = self.backoff.map_or(BACKOFF, |b| (b * 2).min(BACKOFF_MAX));
                self.backoff = Some(b);
                tracing::warn!("the downlink is still stalled; the next round of steps in {} s", b.as_secs());
                self.level = 0;
                self.next = Some(Instant::now() + b);
            }
        }

        self.reference = None;
    }
}

/// What one reading says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Judgement {
    Fine,
    Stalled,
    /// Packets came back after this step.
    Back(u8),
}

fn reboots_file(data: &Data) -> PathBuf {
    data.platform().paths().state().join("downlink-reboots")
}

fn log_file(data: &Data) -> PathBuf {
    data.platform().paths().run().join("downlink.log")
}

/// The counters and what the modem said at the moment of the stall, for the next look (small, in RAM; no
/// identities: the context's state and address, the registration).
async fn evidence(data: &Data, dev: &str, step: u8, msg: &str) {
    let mut text = format!("== step {step}: {msg}\n");
    for cmd in ["AT+CGACT?", &format!("AT+CGPADDR={}", data.config().cid), "AT+CEREG?"] {
        if let Ok(r) = data.at().send(cmd, SendOpts::control(Duration::from_secs(4))).await {
            for l in r.lines.iter().filter(|l| l.starts_with('+')) {
                text.push_str(l);
                text.push('\n');
            }
        }
    }
    let k = Kernel::new(data.platform());
    if let Some(e) = k.counter(dev, "tx_errors") {
        text.push_str(&format!("{dev} tx_errors {e}\n"));
    }
    let path = log_file(data);
    if fsx::private_dir(&data.platform().paths().run()).is_err() {
        return;
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_MAX) {
        let _ = std::fs::remove_file(&path);
    }
    let w = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()));
    if let Err(e) = w {
        tracing::debug!(error = %e, "downlink.log");
    }
}

/// Step 3, under its guards.
async fn reboot(data: &Data, dev: &str, rx: u64, enabled: bool) {
    if !enabled {
        tracing::warn!("step 3 (a reboot) is switched off (dl_reboot)");
        return;
    }
    if rx >= REBOOT_RX {
        tracing::warn!("no reboot: {dev} has had a downlink in this boot ({rx} packets received)");
        return;
    }
    let file = reboots_file(data);
    let n: u32 = fsx::read_opt(&file).ok().flatten().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    if n >= REBOOT_MAX {
        tracing::warn!("no reboot: {n} reboots in a row did not bring the downlink back; waiting for one that has it");
        return;
    }
    let counted = fsx::private_dir(&data.platform().paths().state())
        .and_then(|()| fsx::write_atomic(&file, format!("{}\n", n + 1).as_bytes(), 0o600));
    if let Err(e) = counted {
        tracing::warn!(error = %e, "no reboot: it cannot be counted");
        return;
    }
    if let Ok(Some(log)) = fsx::read_opt(&log_file(data)) {
        let _ = fsx::write_atomic(&file.with_extension("log"), log.as_bytes(), 0o600);
    }
    tracing::error!("downlink stalled, step 3: reboot {} of at most {REBOOT_MAX} in a row", n + 1);
    if let Err(e) = data.platform().system().reboot_after(Duration::from_secs(3)) {
        tracing::warn!(error = %e, "reboot");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_is_not_dead_and_a_stall_needs_time_and_traffic() {
        let t0 = Instant::now();
        let mut d = Downlink::default();
        assert_eq!(d.judge(100, 100, t0), Judgement::Fine);
        // nothing sent: idle
        assert_eq!(d.judge(100, 100, t0 + Duration::from_secs(300)), Judgement::Fine);
        // sent, too soon
        assert_eq!(d.judge(101, 130, t0 + Duration::from_secs(60)), Judgement::Fine);
        assert_eq!(d.judge(102, 130, t0 + Duration::from_secs(91)), Judgement::Stalled);
        // more received: a new reference
        assert_eq!(d.judge(110, 140, t0 + Duration::from_secs(100)), Judgement::Fine);
        assert_eq!(d.judge(110, 200, t0 + Duration::from_secs(150)), Judgement::Fine);
        // counters that went backwards (a new interface)
        assert_eq!(d.judge(0, 0, t0 + Duration::from_secs(400)), Judgement::Fine);
    }

    #[test]
    fn the_downlink_back_after_a_step() {
        let t0 = Instant::now();
        let mut d = Downlink { level: 1, last: 1, ..Downlink::default() };
        d.judge(10, 10, t0);
        assert_eq!(d.judge(50, 60, t0 + Duration::from_secs(5)), Judgement::Back(1));
        assert_eq!(d.level, 0);
    }
}
