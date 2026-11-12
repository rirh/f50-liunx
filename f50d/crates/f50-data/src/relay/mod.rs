//! IPv6 in relay mode (`option ipv6 'relay'`, a dual-stack context; K35-K37, spec D9, after kanoqwq's
//! clean-tf-7.2): the bearer accepts the carrier's RA, odhcpd relays RA and DHCPv6 to the LAN (`dhcp.wan` master),
//! fw4 masquerades IPv6, and f50d keeps the rest in step. It works on carriers that never fill in the context's IPv6
//! address (measured on China Telecom IoT SIMs: SLAAC only).
//!
//! - [`monitor`]: during a session, the RA's addresses and route and the bearer's IPv4/DNS reported to netifd;
//! - [`ndp`]: for the daemon's life, the bearer's /64s routed to the LAN and withdrawn when they go ([`ra`]).

pub mod monitor;
pub mod ndp;
pub mod ra;
mod sys;
