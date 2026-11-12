//! wifi.* on Ubuntu (hotspot.conf) and OpenWrt (UCI through a fake uci).

mod common;

use std::fs;

use common::Dev;

#[test]
fn ubuntu_hotspot_conf() {
    let d = Dev::new();
    let conf = d.p("sys/etc/mu300/hotspot.conf");
    fs::write(&conf, "SSID=Old\nPSK=oldpassword\nBAND=5\nCOUNTRY=TR\n").unwrap();
    let r = d.call("wifi.get", "{}");
    assert_eq!(r.json["data"]["ssid"], "Old");
    assert_eq!(r.json["data"]["password_set"], true);
    assert_eq!(r.json["data"]["channels"]["5"], serde_json::json!([36, 40, 44, 48]));
    assert!(!r.json.to_string().contains("oldpassword"));
    // 149 is not a TR channel on 5 GHz; 6 is fine on 2.4
    assert_eq!(d.call("wifi.set", "{\"channel\":149}").json["field"], "channel");
    assert_eq!(d.call("wifi.set", "{\"password\":\"short\"}").json["field"], "password");
    assert_eq!(d.call("wifi.set", "{\"ssid\":\" lead\"}").json["field"], "ssid");
    assert_eq!(d.call("wifi.set", "{}").json["code"], "missing");
    let r = d.call("wifi.set", "{\"ssid\":\"Ev Ağı\",\"band\":\"2.4\",\"channel\":\"6\",\"hidden\":true}");
    assert_eq!(r.json["data"]["restart"], true, "{}", r.json);
    let t = fs::read_to_string(&conf).unwrap();
    assert_eq!(t, "PSK=oldpassword\nCOUNTRY=TR\nSSID=Ev Ağı\nBAND=2.4\nCHANNEL=6\nHIDDEN=1\n");
}

#[test]
fn openwrt_uci_batch() {
    let d = Dev::new();
    fs::write(d.p("sys/etc/openwrt_release"), "DISTRIB_ID='OpenWrt'\n").unwrap();
    let show = "wireless.radio0=wifi-device\nwireless.radio0.band='5g'\nwireless.radio0.country='DE'\n\
                wireless.radio0.htmode='VHT80'\nwireless.ap=wifi-iface\nwireless.ap.device='radio0'\n\
                wireless.ap.mode='ap'\nwireless.ap.ssid='Box'\nwireless.ap.encryption='sae'\nwireless.ap.key='k'\n";
    fs::write(d.p("log/show"), show).unwrap();
    d.path_cmd(
        "uci",
        &format!(
            "case \"$*\" in *'show wireless'*) cat '{}';; *batch*) cat >> '{}';; esac",
            d.p("log/show").display(),
            d.p("log/uci.stdin").display()
        ),
    );
    let r = d.call("wifi.get", "{}");
    assert_eq!(r.json["data"]["security"], "wpa3");
    assert_eq!(r.json["data"]["ssid"], "Box");
    // SAE on this chip needs a WPA2 password to fall back to
    assert_eq!(d.call("wifi.set", "{\"hidden\":false}").json["field"], "password");
    let r = d.call("wifi.set", "{\"password\":\"it's-a-pass\",\"channel\":44}");
    assert_eq!(r.json["ok"], true, "{}", r.json);
    let b = d.stdin_of("uci");
    assert!(b.contains("set wireless.ap.key='it'\\''s-a-pass'\n"), "{b}");
    assert!(b.contains("set wireless.ap.encryption=psk2+ccmp\n"), "{b}");
    assert!(b.contains("set wireless.radio0.channel=44\nset wireless.radio0.htmode=VHT80\n"), "{b}");
    assert!(b.ends_with("commit wireless\n"));
    assert!(!d.calls().contains("it's-a-pass"), "the password went only to uci's stdin");
}
