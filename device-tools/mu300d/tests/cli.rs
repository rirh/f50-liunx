//! `mu300d call` against fake tools: the envelope, the refusals, argv and stdin shapes.

mod common;

use std::fs;
use std::process::Stdio;

use common::{Dev, wait_until};
use serde_json::json;

#[test]
fn envelope_and_refusals() {
    let d = Dev::new();
    let r = d.call("nope.nope", "{}");
    assert_eq!((r.status, r.json["code"].as_str()), (1, Some("unknown-method")));
    let r = d.call("system.hostname", "{\"name\":\"box\",\"x\":1}");
    assert_eq!(r.json, json!({"ok":false,"code":"unknown-field","error":"unknown field: x","field":"x"}));
    let r = d.call("system.hostname", "[1]");
    assert_eq!(r.json["code"], "bad-request");
    let r = d.call("system.hostname", "{");
    assert_eq!(r.json["code"], "bad-request");
    let r = d.call("system.hostname", "{}");
    assert_eq!((r.json["code"].as_str(), r.json["field"].as_str()), (Some("missing"), Some("name")));
    for bad in ["\"-rf\"", "\"a b\"", "1", "\"x\\ny\"", "{\"a\":1}"] {
        let r = d.call("system.hostname", &format!("{{\"name\":{bad}}}"));
        assert_eq!((r.json["code"].as_str(), r.json["field"].as_str()), (Some("bad-value"), Some("name")), "{bad}");
    }
    // ubus's session field is never one of ours and never refused
    let r = d.call("system.hostname", "{\"name\":\"box\",\"ubus_rpc_session\":\"x\"}");
    assert_eq!(r.json, json!({"ok":true,"data":{}}));
    assert!(d.calls().contains("hostnamectl set-hostname box"));
    // nothing ran for the refused ones
    assert_eq!(d.calls().matches("hostnamectl").count(), 1);
}

#[test]
fn usage_and_methods() {
    let d = Dev::new();
    let st = d.command().arg("call").stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
    assert_eq!(st.code(), Some(2));
    let out = d.command().arg("methods").output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    for m in ["hello", "status.get", "lock.set", "at.send", "job.cancel", "auth.login"] {
        assert!(text.lines().any(|l| l == m), "{m}");
    }
    for dropped in ["vpn.status", "backup.export", "ussd.send", "zram.get", "adb.auth", "system.bootos"] {
        assert!(!text.lines().any(|l| l == dropped), "{dropped}");
    }
    let r = d.call("hello", "");
    assert_eq!(r.json["data"]["api"], 2);
    assert_eq!(r.json["data"]["device"], "f50");
    assert_eq!(r.json["data"]["os"], "ubuntu 24.04");
    let r = d.call("methods", "{}");
    let all = r.json["data"]["methods"].as_array().unwrap();
    let pw = all.iter().find(|m| m["name"] == "system.password").unwrap();
    assert_eq!(pw["access"], "write");
    assert_eq!(pw["fields"][0]["secret"], true);
}

#[test]
fn write_needs_root() {
    let d = Dev::new();
    let mut c = d.command();
    c.env_remove("MU300D_NOROOT");
    let out = c.args(["call", "cpu.profile"]).stdin(Stdio::null()).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // SAFETY: geteuid(2) has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    assert_eq!(v["code"], "unauthorized");
    // a read is fine
    d.bin("mu300-cpu", "echo '{\"governor\":\"x\"}'");
    let mut c = d.command();
    c.env_remove("MU300D_NOROOT");
    let out = c.args(["call", "cpu.get"]).stdin(Stdio::null()).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["cpu"]["governor"], "x");
}

#[test]
fn tool_arguments_and_failures() {
    let d = Dev::new();
    d.bin("mu300-cpu", "exit 0");
    assert_eq!(d.call("cpu.profile", "{\"name\":\"balanced\"}").json["ok"], true);
    assert!(d.calls().contains("mu300-cpu profile balanced"));
    assert_eq!(d.call("cpu.profile", "{\"name\":\"turbo\"}").json["code"], "bad-value");

    d.bin("mu300-traffic", "exit 0");
    assert_eq!(d.call("traffic.set", "{\"key\":\"cap\",\"value\":0}").json["ok"], true);
    assert_eq!(d.call("traffic.set", "{\"key\":\"used\",\"value\":1073741824}").json["ok"], true);
    assert_eq!(d.call("traffic.set", "{\"key\":\"reset_day\",\"value\":\"32\"}").json["code"], "bad-value");
    assert_eq!(d.call("traffic.set", "{\"key\":\"plan\",\"value\":\"\"}").json["ok"], true);
    assert_eq!(d.call("traffic.set", "{\"key\":\"warn\"}").json["code"], "missing");
    let calls = d.calls();
    assert!(calls.contains("mu300-traffic set cap off"), "{calls}");
    assert!(calls.contains("mu300-traffic used 1073741824"), "{calls}");

    // a failing tool: its last message, sanitised, as the error
    d.bin("mu300-sim", "echo 'mu300-sim: no card in slot 2 \u{1b}[0m see https://x.y/tok?k=1' >&2; exit 3");
    let r = d.call("sim.get", "{}");
    assert_eq!(r.json["code"], "failed");
    assert_eq!(r.json["error"], "mu300-sim: no card in slot 2 [0m see https://...");
    // a tool that is not installed
    assert_eq!(d.call("radio.get", "{}").json["code"], "unsupported");
}

#[test]
fn secrets_go_on_stdin() {
    let d = Dev::new();
    d.path_cmd("chpasswd", &d.save_stdin("chpasswd"));
    let r = d.call("system.password", "{\"password\":\"-s3cr:et pw\"}");
    assert_eq!(r.json, json!({"ok":true,"data":{"user":"ubuntu"}}));
    assert_eq!(d.stdin_of("chpasswd"), "ubuntu:-s3cr:et pw\n");
    assert!(!d.calls().contains("s3cr"));

    // the APN password goes into the 0600 settings file, never to an argument
    fs::write(d.p("sys/etc/mu300/mobile-data.conf"), "MU300_APN=old\n").unwrap();
    let r = d.call("data.set", "{\"apn\":\"internet\",\"password\":\"p@ss w\",\"auth\":\"chap\"}");
    assert_eq!(r.json["data"]["restart"], true, "{}", r.json);
    let conf = fs::read_to_string(d.p("sys/etc/mu300/mobile-data.conf")).unwrap();
    assert!(conf.contains("MU300_APN=internet\n") && conf.contains("MU300_APN_PASSWORD=p@ss w\n"), "{conf}");
    let r = d.call("data.get", "{}");
    assert_eq!(r.json["data"]["password_set"], true);
    assert!(!r.json.to_string().contains("p@ss"));
    assert_eq!(d.call("data.set", "{\"password\":\"a\\\"b\"}").json["code"], "bad-value");
}

#[test]
fn at_guards_and_history() {
    let d = Dev::new();
    d.bin("mu300-at", &format!("{}; echo '+CSQ: 20,99'; echo OK", d.save_stdin("mu300-at")));
    let r = d.call("at.send", "{\"command\":\"AT+SPENGMD=0,1,0\"}");
    assert_eq!((r.json["code"].as_str(), r.json["field"].as_str()), (Some("forbidden"), Some("command")));
    assert_eq!(d.call("at.send", "{\"command\":\"AT+CSQ;+CFUN=0\"}").json["code"], "forbidden");
    assert_eq!(d.call("at.send", "{\"command\":\"AT+SPACTCARD=1\"}").json["code"], "forbidden");
    assert_eq!(d.call("at.send", "{\"command\":\"hello\"}").json["code"], "bad-value");
    let r = d.call("at.send", "{\"command\":\"AT+CFUN=0\"}");
    assert_eq!((r.json["code"].as_str(), r.json["field"].as_str()), (Some("missing"), Some("confirm")));
    assert!(!d.calls().contains("mu300-at"), "nothing was sent");

    let r = d.call("at.send", "{\"command\":\"AT+CSQ\"}");
    assert_eq!(r.json["data"]["text"], "+CSQ: 20,99\nOK\n", "{}", r.json);
    assert!(d.calls().contains("mu300-at -t 8 -"));
    assert_eq!(d.stdin_of("mu300-at"), "AT+CSQ\n");

    let r = d.call("at.send", "{\"command\":\"AT+CPIN=1234\",\"confirm\":true}");
    assert_eq!(r.json["data"]["class"], "sim");
    assert!(!d.calls().contains("1234"), "the PIN is not in any argument");
    let h = d.call("at.history", "{}");
    assert_eq!(h.json["data"]["history"][0]["command"], "AT+CPIN=...");
    assert_eq!(h.json["data"]["history"][1]["command"], "AT+CSQ");
    let file = fs::read_to_string(d.p("dash/at-history")).unwrap();
    assert!(!file.contains("1234"), "{file}");

    // a line some other writer left unredacted is redacted on the way out
    fs::write(d.p("dash/at-history"), "11-12 10:00:00 AT+CLCK=\"SC\",1,\"0000\"\n").unwrap();
    let h = d.call("at.history", "{}");
    assert_eq!(h.json["data"]["history"][0]["command"], "AT+CLCK=...");

    // the modem is restarting: busy, nothing sent
    fs::create_dir_all(d.p("sys/run/unisoc-modem-lock-apply")).unwrap();
    assert_eq!(d.call("at.send", "{\"command\":\"AT\"}").json["code"], "busy");
}

#[test]
fn radio_reads_come_from_the_cache() {
    let d = Dev::new();
    let sig = d.p("dash/sig.json");
    // the collector writes its reading a moment later, in the background
    d.adapter("cell", &format!("sleep 1; echo '{{\"rsrp\":-90}}' > '{}'", sig.display()));
    let r = d.call("signal.get", "{}");
    assert_eq!(r.json["data"]["data"], serde_json::Value::Null, "{}", r.json);
    assert_eq!(r.json["data"]["refreshing"], true);
    assert_eq!(r.json["data"]["collector"], "unisoc-modem/cell");
    assert!(wait_until(10, || sig.exists()), "the collector ran");
    let r = d.call("signal.get", "{}");
    assert_eq!(r.json["data"]["data"]["rsrp"], -90);
    assert!(r.json["data"]["age_ms"].as_u64().unwrap() < 10_000);
    // rate-limited: two reads in a row start one refresh
    let n = d.calls().matches("cell fast").count();
    assert_eq!(n, 1, "{}", d.calls());
}

#[test]
fn at_collector_fallback() {
    let d = Dev::new();
    d.bin(
        "mu300-at",
        "read c; case $c in 'AT+COPS?') echo '+COPS: 0,0,\"Op\",7';; 'AT+CEREG?') echo '+CEREG: 2,5';; \
         *) echo '+CESQ: 99,99,255,255,30,50';; esac; echo OK",
    );
    let r = d.call("signal.get", "{}");
    assert_eq!(r.json["data"]["collector"], "mu300-at");
    let f = d.p("run/radio/sig.json");
    assert!(wait_until(10, || f.exists()));
    let r = d.call("signal.get", "{}");
    assert_eq!(
        r.json["data"]["data"],
        json!({"operator":"Op","rat":"LTE","registration":"roaming","rsrp":-90,"rsrq":-4.5})
    );
    // no cell table without the panel's collector
    assert_eq!(d.call("cell.get", "{}").json["data"]["data"], serde_json::Value::Null);
}
