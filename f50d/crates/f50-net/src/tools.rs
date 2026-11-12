//! The programs that *are* the platform for the networks: `ip`, `iw`, `nft`, `mount`. Every run goes through
//! [`Cmd`] (process group, timeout, bounded output); nothing else in the crate builds a command line.

use std::net::Ipv4Addr;
use std::time::Duration;

use f50_core::Result;
use f50_core::exec::Cmd;

const SHORT: Duration = Duration::from_secs(10);

/// `ip ARGS`, its stdout; a failure is an error.
pub(crate) async fn ip(args: &[&str]) -> Result<String> {
    Cmd::new("ip").args(args).timeout(SHORT).ok().await
}

/// `ip ARGS` where a failure changes nothing for the caller (a flush of what is not there).
pub(crate) async fn ip_quiet(args: &[&str]) -> bool {
    Cmd::new("ip").args(args).timeout(SHORT).output().await.is_ok_and(|o| o.success())
}

/// `iw ARGS` with a timeout, its stdout.
pub(crate) async fn iw(args: &[&str], timeout: Duration) -> Result<String> {
    Cmd::new("iw").args(args).timeout(timeout).ok().await
}

/// The interface's type as `iw dev IF info` says it (`managed`, `AP`), `None` when it cannot say.
pub(crate) async fn iface_type(ifname: &str) -> Option<String> {
    let out = iw(&["dev", ifname, "info"], SHORT).await.ok()?;
    parse_iw_type(&out)
}

pub(crate) fn parse_iw_type(info: &str) -> Option<String> {
    info.lines().find_map(|l| l.trim_start().strip_prefix("type ")).map(|t| t.trim().to_owned())
}

/// The channel `iw dev IF info` reports (the live one).
pub(crate) fn parse_iw_channel(info: &str) -> Option<u32> {
    info.lines().find_map(|l| l.trim_start().strip_prefix("channel ")?.split_whitespace().next()?.parse().ok())
}

/// One nft transaction from its text (`nft -f -`, the text on stdin).
pub(crate) async fn nft_apply(text: &str) -> Result<()> {
    Cmd::new("nft").args(["-f", "-"]).stdin(text.as_bytes().to_vec()).timeout(SHORT).ok().await.map(drop)
}

/// What nft says about a table: its listing, gone, or nft could not say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Listing {
    Table(String),
    Absent,
    Unknown,
}

/// `nft list table FAMILY NAME`; when that fails, only a listing of the tables that lacks it is [`Listing::Absent`].
pub(crate) async fn nft_list(family: &str, name: &str) -> Listing {
    if let Ok(o) = Cmd::new("nft").args(["list", "table", family, name]).timeout(SHORT).output().await
        && o.success()
    {
        return Listing::Table(o.stdout);
    }
    match Cmd::new("nft").args(["list", "tables"]).timeout(SHORT).output().await {
        Ok(o) if o.success() => {
            let want = format!("table {family} {name}");
            if o.stdout.lines().any(|l| l.trim() == want) { Listing::Unknown } else { Listing::Absent }
        }
        _ => Listing::Unknown,
    }
}

/// The first IPv4 address and prefix of an interface (`ip -4 -o addr show dev IF`).
pub(crate) async fn ipv4_of(ifname: &str) -> Option<(Ipv4Addr, u8)> {
    let out = ip(&["-4", "-o", "addr", "show", "dev", ifname]).await.ok()?;
    parse_ipv4(&out)
}

pub(crate) fn parse_ipv4(out: &str) -> Option<(Ipv4Addr, u8)> {
    out.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        w.find(|t| *t == "inet")?;
        crate::lan::parse_cidr(w.next()?)
    })
}

/// Every address of an interface with its prefix, as `ip -4 -o addr show` lists them.
pub(crate) async fn ipv4_all(ifname: &str) -> Vec<(Ipv4Addr, u8)> {
    let Ok(out) = ip(&["-4", "-o", "addr", "show", "dev", ifname]).await else { return Vec::new() };
    out.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            w.find(|t| *t == "inet")?;
            crate::lan::parse_cidr(w.next()?)
        })
        .collect()
}

/// `object.method(args)` over ubus (the platform's client); `unsupported` before ubusd is there.
pub(crate) async fn ubus(
    net: &crate::Net,
    object: &str,
    method: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    net.platform().call(object, method, args).await
}

/// `mount -t TYPE SOURCE TARGET`.
pub(crate) async fn mount(fstype: &str, source: &str, target: &str) -> Result<()> {
    Cmd::new("mount").args(["-t", fstype, source, target]).timeout(SHORT).ok().await.map(drop)
}

/// `umount TARGET` (a failure is left as it is).
pub(crate) async fn umount(target: &str) {
    let _ = Cmd::new("umount").arg(target).timeout(SHORT).output().await;
}

/// A line for the system log under `tag` (`logger`), for what the user may look for there later.
pub(crate) async fn logger(tag: &str, line: &str) {
    tracing::info!(target: "f50_net", "{tag}: {line}");
    let _ = Cmd::new("logger").args(["-t", tag, "--", line]).timeout(SHORT).output().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iw_info() {
        let info =
            "Interface wlan0\n\tifindex 5\n\ttype AP\n\tchannel 36 (5180 MHz), width: 80 MHz, center1: 5210 MHz\n";
        assert_eq!(parse_iw_type(info).as_deref(), Some("AP"));
        assert_eq!(parse_iw_channel(info), Some(36));
        assert_eq!(parse_iw_type("Interface wlan0\n\ttype managed\n").as_deref(), Some("managed"));
    }

    #[test]
    fn addresses() {
        let out =
            "7: br-lan    inet 192.168.77.1/24 brd 192.168.77.255 scope global br-lan\\       valid_lft forever\n";
        assert_eq!(parse_ipv4(out), Some((Ipv4Addr::new(192, 168, 77, 1), 24)));
        assert_eq!(parse_ipv4(""), None);
    }
}
