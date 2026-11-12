//! The method tables: `f50.wifi`, `f50.wificlient`, `f50.lan`, `f50.usb`. Each handler is a closure over the
//! service; the fields are checked before it runs, the rules that depend on more than one field inside it.

use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::json;

use crate::{Net, adbd, client, lan, rules, usb, wifi};

const NONE: &[Field] = &[];

const WIFI_SET: &[Field] = &[
    Field::opt("ssid", rules::SSID),
    Field::secret("password", rules::PASSWORD, false),
    Field::opt("band", rules::BAND),
    Field::opt("channel", rules::CHANNEL),
    Field::opt("width", rules::WIDTH),
    Field::opt("hidden", Rule::Bool),
    Field::opt("country", rules::COUNTRY),
];
const POWER: &[Field] = &[Field::req("state", rules::ON_OFF)];

const JOIN: &[Field] = &[
    Field::req("ssid", rules::SSID),
    Field::secret("password", rules::PASSWORD, false),
    Field::opt("open", Rule::Bool),
];
const LEAVE: &[Field] = &[Field::opt("forget", Rule::Bool), Field::opt("keep", Rule::Bool)];

const LAN_SET: &[Field] = &[
    Field::opt("address", lan::ADDRESS),
    Field::opt("netmask", Rule::Word { re: "[0-9.]{7,15}", max: 15 }),
    Field::opt("dhcp", Rule::Bool),
    Field::opt("dhcp_start", Rule::Int { min: 2, max: 254 }),
    Field::opt("dhcp_limit", Rule::Int { min: 1, max: 253 }),
    Field::opt("leasetime", Rule::Word { re: "[0-9a-z]{1,9}", max: 9 }),
];

const USB_SET: &[Field] = &[
    Field::opt("role", Rule::Enum(&["host", "device"])),
    Field::opt("net", Rule::Enum(&["ncm", "ecm", "rndis", "multi", "default"])),
    Field::opt("scope", Rule::Enum(&["once", "permanent"])),
    Field::opt("name", Rule::Enum(&["usb", "eth"])),
];
const ADAPTER: &[Field] = &[Field::req("iface", rules::IFNAME)];
const ADB: &[Field] = &[Field::opt("state", rules::ON_OFF), Field::opt("auth", Rule::Enum(&["open", "keys"]))];
const ADB_KEY: &[Field] = &[Field::req("key", Rule::Text { max_bytes: 800, max_chars: 800 })];

/// Clones the service into a handler.
macro_rules! with {
    ($net:expr, |$n:ident, $p:ident| $body:expr) => {{
        let $n = $net.clone();
        move |$p: Params| {
            let $n = $n.clone();
            async move {
                let _ = &$p;
                $body
            }
        }
    }};
}

pub(crate) fn wifi(net: &Net) -> Object {
    Object::builder("f50.wifi")
        .method(
            "get",
            Access::Read,
            NONE,
            "The hotspot: name, band, channel, country, security, clients (never the password).",
            with!(net, |n, p| wifi::ap::get(&n).await),
        )
        .method(
            "set",
            Access::Write,
            WIFI_SET,
            "Changes the hotspot's name, password, band, channel, width, hidden flag or country.",
            with!(net, |n, p| wifi::settings::apply(&n, &p).await),
        )
        .method(
            "power",
            Access::Write,
            POWER,
            "Turns the hotspot on or off.",
            with!(net, |n, p| {
                let s = wifi::ap::ap_set(&n, p.str("state") == Some("on")).await?;
                Ok(json!({ "result": s }))
            }),
        )
        .build()
}

pub(crate) fn wificlient(net: &Net) -> Object {
    Object::builder("f50.wificlient")
        .method(
            "status",
            Access::Read,
            NONE,
            "The Wi-Fi uplink: mode, network, signal, address, firewall, saved network.",
            with!(net, |n, p| client::status(&n).await),
        )
        .method(
            "scan",
            Access::Write,
            NONE,
            "Lists the networks in range (a job when the radio must scan by itself).",
            with!(net, |n, p| client::scan(&n).await),
        )
        .method(
            "join",
            Access::Write,
            JOIN,
            "Joins a network as the uplink and saves it (a job of kind wifi).",
            with!(net, |n, p| client::join(&n, &p).await),
        )
        .method(
            "leave",
            Access::Write,
            LEAVE,
            "Leaves the network and brings the hotspot back (a job of kind wifi).",
            with!(net, |n, p| client::leave(&n, &p).await),
        )
        .build()
}

pub(crate) fn lan(net: &Net) -> Object {
    Object::builder("f50.lan")
        .method(
            "get",
            Access::Read,
            NONE,
            "The LAN: address, netmask, DHCP range, the USB host's address.",
            with!(net, |n, p| lan::get(&n).await),
        )
        .method(
            "set",
            Access::Write,
            LAN_SET,
            "Changes the LAN address (a private /24's .1), netmask or DHCP range.",
            with!(net, |n, p| lan::set(&n, &p).await),
        )
        .build()
}

pub(crate) fn usb(net: &Net) -> Object {
    Object::builder("f50.usb")
        .method(
            "get",
            Access::Read,
            NONE,
            "The USB-C port: role, network mode, link speed, devices, adb.",
            with!(net, |n, p| usb::get(&n).await),
        )
        .method(
            "set",
            Access::Write,
            USB_SET,
            "Sets the port's role and the USB network mode and name (once or for good).",
            with!(net, |n, p| usb::set(&n, &p).await),
        )
        .method(
            "adapters",
            Access::Read,
            NONE,
            "USB network adapters in the host role, and whether they are in the LAN.",
            with!(net, |n, p| usb::host::adapters(&n).await),
        )
        .method(
            "adapter_add",
            Access::Write,
            ADAPTER,
            "Adds a USB network adapter to the LAN bridge.",
            with!(net, |n, p| usb::host::add(&n, p.str("iface").unwrap_or_default()).await),
        )
        .method(
            "adb",
            Access::Write,
            ADB,
            "Turns adb over USB on or off, or sets whom it lets in (open or keys).",
            with!(net, |n, p| adbd::set(&n, &p).await),
        )
        .method(
            "adb_key_add",
            Access::Write,
            ADB_KEY,
            "Authorises a computer's adb public key (adbkey.pub).",
            with!(net, |n, p| adbd::key_add(&n, &p).await),
        )
        .build()
}
