//! A dialled bearer handed to netifd: the kernel state installed by us, then the address-external update, then
//! IPv6 by the mode in use (f50cell.sh's setup).
//!
//! - **relay** (`option ipv6 'relay'`, a dual-stack context): the device takes the carrier's RA before the update;
//!   the relay monitor reports the RA's addresses afterwards ([`crate::relay::monitor`]).
//! - **extend** (otherwise), when the network assigned IPv6: IPv6 back on (a teardown switched it off and netifd does
//!   not turn it on again for a device that never went away: after a WAN restart the bearer stayed IPv4-only, #87),
//!   the link-local address from the assigned identifier, and a DHCPv6 alias (`wan6`, `extendprefix`) that learns the
//!   /64 from the RA and hands it to the LAN (RFC 7278, as uqmi does for QMI modems). The image has no static `wan6`
//!   (90-f50), so the name is free.
//! - an IPv4-only bearer: IPv6 off on the device, after the update (netifd turns it back on while it configures the
//!   interface). Otherwise it sends router solicitations and multicast into a bearer that carries none (FINDINGS 13f).

use f50_core::Result;
use serde_json::json;

use crate::Data;
use crate::dial::Bearer;
use crate::kernel::Kernel;
use crate::netifd::Update;

pub(super) async fn bring_up(data: &Data, config: &str, b: &Bearer, relay: bool) -> Result<()> {
    let k = Kernel::new(data.platform());
    let dev = b.device.as_str();
    k.link_up(dev).await?;
    k.flush_v6(dev).await;
    if relay {
        k.take_ra(dev);
    }
    k.install_v4(dev, &b.ipv4, b.prefix).await?;
    k.fw4_before_ifup(config).await;
    let update = Update {
        ifname: dev.to_owned(),
        ipv4: vec![(b.ipv4.clone(), b.prefix)],
        default4: true,
        dns: b.dns.clone(),
        ..Update::default()
    };
    data.notifier().update(config, &update).await?;
    if relay {
        return Ok(());
    }
    match &b.iid {
        Some(iid) => {
            k.ipv6(dev, true);
            k.link_local(dev, iid).await;
            let mut spec = json!({
                "name": data.config().interface6,
                "ifname": format!("@{config}"),
                "proto": "dhcpv6",
                "reqaddress": "none",
                "reqprefix": "no",
                "extendprefix": true,
            });
            if let Some(zone) = k.fw4_zone(config).await {
                spec["zone"] = json!(zone);
            }
            if let Err(e) = data.notifier().add_dynamic(&spec).await {
                tracing::warn!(error = %e, "the IPv6 alias of the bearer could not be added");
            }
        }
        None => k.ipv6(dev, false),
    }
    Ok(())
}
