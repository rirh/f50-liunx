//! The hotspot is WPA2-PSK, never WPA3/SAE, and U-APSD is off on every access point.
//!
//! WPA3: the SC2355's firmware SAE offload is disabled (docs/research/U30AIR-SAE.md, FINDINGS 80): the PMK record the
//! firmware hands back came in the same element list as the client's own association request, with no boundary, so a
//! client could forge it and join without the password. Plain hostapd SAE never worked on this chip either. At boot,
//! before the Wi-Fi comes up, every access point that asks for SAE (encryption sae, sae-mixed, sae-compat, psk3,
//! psk3-mixed, or the offload's hostapd option `u30_sae_offload`) becomes WPA2-PSK (CCMP) with the same password.
//! Without a usable WPA2 password it is switched off instead - never left open. The settings as they were are kept
//! once in `/etc/f50/wireless.before-wpa2` (root only), and `/etc/f50/wpa3-disabled` tells LuCI and the panel why.
//! Ported from `openwrt-wifi-wpa2-only migrate`.
//!
//! U-APSD (kanoqwq 2d3f7438, FINDINGS 203): `uapsd 0` on every access point; stations and every other option are
//! left alone. (On his U30 Air the firmware's beacon still announced it: a configuration policy, not proof that the
//! firmware stopped it.) Ported from `uci-defaults/94-mu300-ap-uapsd`.

use f50_core::{Result, fsx};
use f50_platform::uci::{Change, Section};

use super::{Wireless, asks_sae, delete_if, offload_options, set};
use crate::{Net, rules, tools};

/// The access points (mode ap, or none given) that ask for SAE.
pub(crate) fn sae_aps(w: &Wireless) -> Vec<&Section> {
    w.ifaces()
        .filter(|s| matches!(s.get("mode"), Some("ap") | None))
        .filter(|s| asks_sae(s.get("encryption")) || !offload_options(s).is_empty())
        .collect()
}

/// What the migration does to one access point: WPA2 with its password, or off when it has none WPA2 can use.
pub(crate) fn migrate_changes(ap: &Section) -> (Vec<Change>, bool) {
    let mut out = vec![set(ap, "encryption", "psk2+ccmp")];
    let usable = ap.get("key").is_some_and(rules::wpa2_key_ok);
    if !usable {
        out.push(set(ap, "disabled", "1"));
    }
    delete_if(ap, "ieee80211w", &mut out);
    delete_if(ap, "sae_pwe", &mut out);
    for o in offload_options(ap) {
        out.push(Change::DelList { section: ap.name.clone(), option: "hostapd_bss_options".into(), value: o });
    }
    (out, usable)
}

/// The notice's text for LuCI and the panel, and the log line.
pub(crate) fn message(migrated: &[String], down: &[String]) -> (String, &'static str) {
    if down.is_empty() {
        let m = format!(
            "WPA3 (SAE) on the hotspot is disabled for security (FINDINGS 80): {} is now WPA2-PSK (CCMP) with the same \
             password. The settings it had are in /etc/f50/wireless.before-wpa2.",
            migrated.join(" ")
        );
        return (m, "migrated");
    }
    let mut m = format!(
        "WPA3 (SAE) on the hotspot is disabled for security (FINDINGS 80) and the switch to WPA2 failed for {} (no usable \
         WPA2 password, or the change was not saved): that access point is off, not open. Set a WPA2 password (8-63 \
         characters) in Network > Wireless and switch it on.",
        down.join(" ")
    );
    if !migrated.is_empty() {
        m.push_str(&format!(" {} is back on WPA2-PSK with the same password.", migrated.join(" ")));
    }
    (m, "down")
}

/// The migration; whether anything changed. Nothing is done, asked or written when no access point asks for SAE.
/// The password is never logged.
pub(crate) async fn migrate(net: &Net) -> Result<bool> {
    let w = Wireless::load(net).await?;
    let aps = sae_aps(&w);
    if aps.is_empty() {
        return Ok(false);
    }
    let saved = net.legacy("wireless.before-wpa2");
    if !saved.exists()
        && let Ok(Some(old)) = fsx::read_opt(&net.path("/etc/config/wireless"))
        && let Some(dir) = saved.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        if let Err(e) = fsx::write_atomic(&saved, old.as_bytes(), 0o600) {
            tracing::warn!(error = %e, "could not keep the wireless settings before WPA2");
        }
    }
    let (mut changes, mut migrated, mut down) = (Vec::new(), Vec::new(), Vec::new());
    for ap in &aps {
        let (c, usable) = migrate_changes(ap);
        changes.extend(c);
        if usable { migrated.push(ap.name.clone()) } else { down.push(ap.name.clone()) }
    }
    let applied = net.platform().uci().apply("wireless", &changes).await;
    // what uci has now, not what was asked: an access point still on SAE is refused by the wifi-scripts (down, not
    // open)
    let after = Wireless::load(net).await.unwrap_or_default();
    migrated.retain(|s| {
        let ok = applied.is_ok() && after.section(s).is_some_and(|a| a.get("encryption") == Some("psk2+ccmp"));
        if !ok {
            down.push(s.clone());
        }
        ok
    });
    let (msg, state) = message(&migrated, &down);
    tools::logger("f50-wifi", &msg).await;
    let notice = format!(
        "state={state}\ntime={}\nmigrated={}\ndown={}\n",
        f50_core::time::unix_now(),
        migrated.join(" "),
        down.join(" ")
    );
    // the log does not keep it for long: the notice stays until it is dismissed in LuCI or the panel
    if let Err(e) = fsx::write_atomic(&net.legacy("wpa3-disabled"), notice.as_bytes(), 0o644) {
        tracing::warn!(error = %e, "could not write the WPA3 notice");
    }
    applied.map(|()| true)
}

/// `uapsd 0` on every access point that does not have it; whether anything changed.
pub(crate) async fn uapsd_off(net: &Net) -> Result<bool> {
    let w = Wireless::load(net).await?;
    let changes: Vec<Change> = w
        .ifaces()
        .filter(|s| s.get("mode") == Some("ap") && s.get("uapsd") != Some("0"))
        .map(|s| set(s, "uapsd", "0"))
        .collect();
    if changes.is_empty() {
        return Ok(false);
    }
    net.platform().uci().apply("wireless", &changes).await.map(|()| true)
}

#[cfg(test)]
mod tests {
    use super::super::tests::sec;
    use super::*;

    #[test]
    fn finds_every_sae_mode_and_the_option() {
        let mut off = sec("o", "wifi-iface", &[("mode", "ap"), ("encryption", "psk2")]);
        off.options.insert(
            "hostapd_bss_options".into(),
            f50_platform::uci::Value::List(vec!["u30_sae_offload=1".into(), "x=1".into()]),
        );
        let w = Wireless {
            sections: vec![
                sec("a", "wifi-iface", &[("mode", "ap"), ("encryption", "sae-mixed")]),
                sec("b", "wifi-iface", &[("encryption", "psk3")]),
                sec("c", "wifi-iface", &[("mode", "sta"), ("encryption", "sae")]),
                sec("d", "wifi-iface", &[("mode", "ap"), ("encryption", "psk2")]),
                off,
            ],
        };
        let names: Vec<&str> = sae_aps(&w).iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "o"]);
        let (c, usable) = migrate_changes(sae_aps(&w)[2]);
        assert!(!usable);
        assert!(c.contains(&Change::DelList {
            section: "o".into(),
            option: "hostapd_bss_options".into(),
            value: "u30_sae_offload=1".into()
        }));
    }

    #[test]
    fn without_a_password_off_not_open() {
        let ap = sec("ap0", "wifi-iface", &[("mode", "ap"), ("encryption", "sae"), ("key", "short")]);
        let (c, usable) = migrate_changes(&ap);
        assert!(!usable);
        assert!(c.contains(&set(&ap, "disabled", "1")));
        assert!(c.contains(&set(&ap, "encryption", "psk2+ccmp")));
        let good = sec("ap0", "wifi-iface", &[("encryption", "sae"), ("key", "longenough"), ("sae_pwe", "2")]);
        let (c, usable) = migrate_changes(&good);
        assert!(usable);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn messages() {
        let (m, s) = message(&["ap0".into()], &[]);
        assert_eq!(s, "migrated");
        assert!(m.contains("ap0 is now WPA2-PSK"));
        let (m, s) = message(&["a".into()], &["b".into()]);
        assert_eq!(s, "down");
        assert!(m.contains("failed for b") && m.ends_with("a is back on WPA2-PSK with the same password."));
    }
}
