//! The client's watcher, for as long as it is joined: the supplicant's events and udhcpc's renewals keep the sharing
//! in step with the link. A lost link (out of range, the access point gone) closes the sharing and drops the lease; a
//! link back asks DHCP again, checks the address as a join does, and shares it. A renewal with another address, or a
//! lost lease, closes first, applies, checks, and only then shares again. Replaces `wpa_cli -a` and the shell's
//! `event`/`lease_changed`; each step holds the client lock, and a watcher whose session ended does nothing.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::share::{self, FwState};
use super::{ctrl::Ctrl, join, lease};
use crate::{Net, tools};

/// Starts the watcher of this session; cancelling the token ends it.
pub(crate) fn spawn(net: &Net, metric: u32) -> CancellationToken {
    let stop = net.inner.shutdown.child_token();
    let token = stop.clone();
    let n = net.clone();
    net.spawn("wifi-client-watch", async move {
        let mut applied = lease::read(&n).filter(|l| l.ip.is_some());
        loop {
            let ctl = match Ctrl::open(&n.path(&n.cfg().wpa_ctrl_dir), &n.cfg().wlan, &n.run_dir()).await {
                Ok(c) if c.attach().await.is_ok() => Some(c),
                _ => None,
            };
            loop {
                let ev = async {
                    match &ctl {
                        Some(c) => c.event().await.ok(),
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    () = stop.cancelled() => return,
                    e = ev => match e {
                        Some(e) if e.starts_with("CTRL-EVENT-DISCONNECTED") => link(&n, &stop, false, metric, &mut applied).await,
                        Some(e) if e.starts_with("CTRL-EVENT-CONNECTED") => link(&n, &stop, true, metric, &mut applied).await,
                        Some(_) => {}
                        // the socket failed: open it again
                        None => break,
                    },
                    () = tokio::time::sleep(Duration::from_secs(5)) => {
                        renewal(&n, &stop, metric, &mut applied).await;
                        if ctl.is_none() {
                            break;
                        }
                    }
                }
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        }
    });
    token
}

async fn link(net: &Net, stop: &CancellationToken, up: bool, metric: u32, applied: &mut Option<lease::Lease>) {
    let _st = net.inner.client.lock().await;
    if stop.is_cancelled() {
        return;
    }
    let ifname = net.cfg().wlan.clone();
    if !up {
        if !join::fail_closed(net).await {
            return;
        }
        lease::drop_all(net).await;
        *applied = None;
        tools::logger("wifi-client", &format!("link lost: {ifname} not shared until it is back")).await;
        return;
    }
    if share::put(FwState::Closed, &ifname, None).await.is_err() {
        tools::logger(
            "wifi-client",
            &format!("could not close the firewall for {ifname} (nft): not asking for an address"),
        )
        .await;
        return;
    }
    match join::uplink_up(net, metric).await {
        Ok(l) => {
            let shown = l.ip.map(|a| format!("{}/{}", crate::lan::network(a, l.prefix), l.prefix)).unwrap_or_default();
            *applied = Some(l);
            tools::logger("wifi-client", &format!("link back: {ifname} shared again ({shown})")).await;
        }
        Err(e) => {
            *applied = None;
            tools::logger("wifi-client", &format!("link back, not shared: {}", e.message())).await;
        }
    }
}

async fn renewal(net: &Net, stop: &CancellationToken, metric: u32, applied: &mut Option<lease::Lease>) {
    let Some(now) = lease::read(net) else { return };
    let same = applied.as_ref().is_some_and(|a| a.ip == now.ip && a.prefix == now.prefix);
    let lost = now.event == "deconfig" || now.ip.is_none();
    if (same && !lost) || (lost && applied.is_none()) {
        return;
    }
    // only when the lock is free: whoever holds it is a join, a leave or a link event, which check the address
    // themselves
    let Ok(_st) = net.inner.client.try_lock() else { return };
    if stop.is_cancelled() {
        return;
    }
    let ifname = net.cfg().wlan.clone();
    if !join::fail_closed(net).await {
        return;
    }
    if lost {
        lease::flush_address(net).await;
        *applied = None;
        tools::logger("wifi-client", &format!("lease lost: {ifname} not shared until it has an address again")).await;
        return;
    }
    if let Err(e) = lease::apply(net, &now, metric).await {
        tools::logger("wifi-client", &format!("new address from the network not applied: {}", e.message())).await;
        return;
    }
    if let Some(why) = join::lan_check(net, now.net()).await {
        lease::flush_address(net).await;
        *applied = None;
        tools::logger("wifi-client", &format!("new address from the network, not shared: {why}")).await;
        return;
    }
    match join::share(net, now.net()).await {
        Ok(()) => tools::logger("wifi-client", &format!("new address from the network: {ifname} shared again")).await,
        Err(e) => tools::logger("wifi-client", e.message()).await,
    }
    *applied = Some(now);
}
