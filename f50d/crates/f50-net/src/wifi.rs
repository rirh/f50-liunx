//! The Wi-Fi hotspot: `/etc/config/wireless` read and changed through UCI, netifd reloading it.
//!
//! - [`channels`]: the 5 GHz channels without DFS per country (FINDINGS 23, 42);
//! - [`ap`]: which section is the hotspot, `f50.wifi get`, the on/off switch;
//! - [`settings`]: `f50.wifi set` (name, password, band, channel, width, hidden, country);
//! - [`wpa2`]: WPA2 only, never WPA3/SAE (FINDINGS 80), and U-APSD off (FINDINGS 203);
//! - [`boot`]: the configuration made once, a DFS channel replaced at boot;
//! - [`ready`]: is the Wi-Fi up as configured (`wifi-ready`).

pub(crate) mod ap;
pub(crate) mod boot;
pub(crate) mod channels;
pub(crate) mod ready;
pub(crate) mod settings;
pub(crate) mod wpa2;

use f50_core::Result;
use f50_platform::uci::{Change, Section};

use crate::Net;

/// The wireless configuration as loaded once for one decision.
#[derive(Debug, Clone, Default)]
pub(crate) struct Wireless {
    pub sections: Vec<Section>,
}

impl Wireless {
    pub async fn load(net: &Net) -> Result<Wireless> {
        Ok(Wireless { sections: net.platform().uci().load("wireless").await? })
    }

    pub fn radios(&self) -> impl Iterator<Item = &Section> {
        self.sections.iter().filter(|s| s.stype == "wifi-device")
    }

    pub fn ifaces(&self) -> impl Iterator<Item = &Section> {
        self.sections.iter().filter(|s| s.stype == "wifi-iface")
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The hotspot's radio and access point: the access point on the first radio whose interface is the F50's
    /// (`ifname wlan0`), else the first access point on that radio, else the only access point there is.
    pub fn hotspot(&self, wlan: &str) -> Option<(&Section, &Section)> {
        let aps: Vec<&Section> = self.ifaces().filter(|s| s.get("mode") == Some("ap")).collect();
        let radio_of =
            |ap: &Section| ap.get("device").and_then(|d| self.section(d)).filter(|r| r.stype == "wifi-device");
        let first = self.radios().next();
        let on_first = |ap: &&Section| first.is_some_and(|r| ap.get("device") == Some(r.name.as_str()));
        let pick = aps
            .iter()
            .find(|ap| on_first(ap) && ap.get("ifname") == Some(wlan))
            .or_else(|| aps.iter().find(|ap| on_first(ap)))
            .or_else(|| aps.iter().find(|ap| ap.get("ifname") == Some(wlan)))
            .or_else(|| (aps.len() == 1).then(|| &aps[0]))?;
        Some((radio_of(pick)?, pick))
    }
}

/// `set section.option=value`.
pub(crate) fn set(section: &Section, option: &str, value: impl Into<String>) -> Change {
    Change::Set { section: section.name.clone(), option: option.into(), value: value.into() }
}

/// `delete section.option`, only when it is there (uci refuses to delete what is not, and one refusal fails the
/// whole batch).
pub(crate) fn delete_if(section: &Section, option: &str, out: &mut Vec<Change>) {
    if section.options.contains_key(option) {
        out.push(Change::Delete { section: section.name.clone(), option: Some(option.into()) });
    }
}

/// The radio's band as the API names it.
pub(crate) fn band_of(radio: &Section) -> &'static str {
    if radio.get("band") == Some("2g") { "2.4" } else { "5" }
}

/// The radio's country, or the default when it has none (or something else).
pub(crate) fn country_of(net: &Net, radio: &Section) -> String {
    radio
        .get("country")
        .filter(|c| crate::rules::country_ok(c))
        .map_or_else(|| net.cfg().default_country.clone(), str::to_owned)
}

/// Encryption modes that ask for SAE: never on this chip (FINDINGS 80).
pub(crate) fn asks_sae(encryption: Option<&str>) -> bool {
    let base = encryption.unwrap_or("").split('+').next().unwrap_or("");
    matches!(base, "sae" | "sae-mixed" | "sae-compat" | "psk3" | "psk3-mixed")
}

/// Has a `hostapd_bss_options` entry for the U30 Air's SAE offload (FINDINGS 80).
pub(crate) fn offload_options(iface: &Section) -> Vec<String> {
    iface.list("hostapd_bss_options").into_iter().filter(|o| o.starts_with("u30_sae_offload")).collect()
}

/// Asks netifd to reload the wireless configuration (`ubus call network reload`).
pub(crate) async fn reload(net: &Net) -> Result<()> {
    net.platform().netifd().reload().await
}

/// A full Wi-Fi restart (`wifi down; wifi up` through netifd's `network.wireless`): an access point the wifi-scripts
/// refused is not there to reload.
pub(crate) async fn restart(net: &Net) -> Result<()> {
    let none = serde_json::json!({});
    crate::tools::ubus(net, "network.wireless", "down", &none).await?;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    crate::tools::ubus(net, "network.wireless", "up", &none).await.map(drop)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use f50_platform::uci::Value;

    use super::*;

    pub(crate) fn sec(name: &str, stype: &str, o: &[(&str, &str)]) -> Section {
        Section {
            name: name.into(),
            stype: stype.into(),
            anonymous: false,
            options: o.iter().map(|(k, v)| ((*k).to_owned(), Value::Str((*v).to_owned()))).collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn finds_the_hotspot() {
        let w = Wireless {
            sections: vec![
                sec("radio0", "wifi-device", &[("band", "5g")]),
                sec("sta", "wifi-iface", &[("device", "radio0"), ("mode", "sta")]),
                sec("guest", "wifi-iface", &[("device", "radio0"), ("mode", "ap"), ("ifname", "wlan1")]),
                sec("ap0", "wifi-iface", &[("device", "radio0"), ("mode", "ap"), ("ifname", "wlan0")]),
            ],
        };
        let (r, ap) = w.hotspot("wlan0").expect("found");
        assert_eq!((r.name.as_str(), ap.name.as_str()), ("radio0", "ap0"));
        assert_eq!(band_of(r), "5");
        let none = Wireless { sections: vec![sec("radio0", "wifi-device", &[])] };
        assert!(none.hotspot("wlan0").is_none());
    }

    #[test]
    fn sae_modes() {
        for e in ["sae", "sae-mixed", "sae-compat", "psk3", "psk3-mixed", "sae+ccmp"] {
            assert!(asks_sae(Some(e)), "{e}");
        }
        for e in ["psk2", "psk2+ccmp", "none", ""] {
            assert!(!asks_sae(Some(e)), "{e}");
        }
        assert!(!asks_sae(None));
    }
}
