//! The command line: which role this process plays.
//!
//! ```text
//! f50d daemon                              the daemon (procd: /etc/init.d/f50d)
//! f50d at-keeper                           holds the AT channels and drains the modem log (/etc/init.d/f50d-at)
//! f50d boot <early|vendor|hardware|post>   a boot stage (procd: /etc/init.d/f50d-{vendor,hw,post}, preinit)
//! f50d boot vendor-exec <daemon>           one vendor daemon in the chroot
//! f50d boot trial                          arm the Linux slot for a trial boot (the updater); exit 3: default is Android
//! f50d proto <setup|teardown|renew> <config> [iface]    netifd's f50cell handler
//! f50d hotplug <iface|net|ntp|wcn>         hotplug.d (ACTION, INTERFACE, ... in the environment)
//! f50d call <object> <method> [json]       one API call to the running daemon (json, or stdin, or {})
//! f50d methods                             the API's description (f50.meta describe)
//! f50d acl                                 the rpcd ACL the daemon generated
//! ```
//!
//! Started as `modprobe-guard` (a link), it is the kernel's module helper.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};

use crate::{boot, bus, daemon, log};

const USAGE: &str = "usage: f50d daemon | at-keeper | boot <stage> | proto <op> <config> [iface] | \
                     hotplug <iface|net|ntp|wcn> | call <object> <method> [json] | methods | acl";

pub fn run(argv: Vec<OsString>) -> ExitCode {
    let name = argv.first().and_then(|a| Path::new(a).file_name()).map(|n| n.to_string_lossy().into_owned());
    if name.as_deref() == Some("modprobe-guard") {
        return boot::modprobe_guard(&argv[1..]);
    }
    let args: Vec<String> = argv.iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    match args.first().map(String::as_str) {
        Some("daemon") => daemon::run(),
        Some("at-keeper") => at_keeper(),
        Some("boot") => short(boot::run(&args[1..])),
        Some("hotplug") => short(hotplug(&args[1..])),
        Some("proto") => short(proto(&args[1..])),
        Some("call") => short(call(&args[1..])),
        Some("methods") => short(print_call("f50.meta", "describe", json!({}))),
        Some("acl") => short(print_call("f50.daemon", "acl", json!({}))),
        _ => {
            log::init(None);
            tracing::error!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// A short program: logging, a small runtime, the future, done.
fn short<F: std::future::Future<Output = ExitCode>>(f: F) -> ExitCode {
    log::init(None);
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt.block_on(f),
        Err(e) => {
            tracing::error!(error = %e, "no runtime");
            ExitCode::FAILURE
        }
    }
}

fn at_keeper() -> ExitCode {
    log::init(None);
    let mut devices: Vec<PathBuf> = Vec::new();
    for slot in [0, 1] {
        let c = f50_at::AtConfig::for_slot(slot);
        devices.extend([Some(c.control), c.poll, c.urc].into_iter().flatten());
    }
    devices.sort();
    devices.dedup();
    let drain = std::thread::Builder::new().name("slog".into()).spawn(|| {
        f50_at::keeper::slog::drain(f50_at::keeper::slog::SLOG_DEVICE, Path::new(f50_at::keeper::slog::SLOG_NODE));
    });
    if let Err(e) = drain {
        tracing::error!(error = %e, "could not start the modem log drain");
    }
    match f50_at::keeper::serve(Path::new(f50_at::keeper::KEEPER_SOCKET), &devices) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "at-keeper stopped");
            ExitCode::FAILURE
        }
    }
}

fn ok_of(v: &Value) -> ExitCode {
    if v.get("ok") == Some(&Value::Bool(true)) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

async fn print_call(object: &str, method: &str, args: Value) -> ExitCode {
    let v = bus::call(object, method, &args, Duration::from_secs(120)).await;
    let mut out = std::io::stdout().lock();
    let _ = serde_json::to_writer_pretty(&mut out, &v);
    let _ = std::io::Write::write_all(&mut out, b"\n");
    ok_of(&v)
}

async fn call(args: &[String]) -> ExitCode {
    let (object, method, json) = match args {
        [o, m] => (o, m, None),
        [o, m, j] => (o, m, Some(j.clone())),
        _ => return ExitCode::from(2),
    };
    let text = match json {
        Some(j) if j != "-" => j,
        _ => {
            let mut s = String::new();
            let _ = std::io::stdin().take(f50_core::rpc::REQUEST_MAX as u64 + 1).read_to_string(&mut s);
            s
        }
    };
    let req = if text.trim().is_empty() { json!({}) } else { serde_json::from_str(&text).unwrap_or(Value::Null) };
    if !req.is_object() {
        let v = json!({"ok": false, "code": "bad-request", "error": "the request is not one JSON object"});
        println_json(&v);
        return ExitCode::FAILURE;
    }
    print_call(object, method, req).await
}

fn println_json(v: &Value) {
    let mut out = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, v);
    let _ = std::io::Write::write_all(&mut out, b"\n");
}

async fn proto(args: &[String]) -> ExitCode {
    let (op, config, iface) = match args {
        [op, c] => (op.as_str(), c, ""),
        [op, c, i] => (op.as_str(), c, i.as_str()),
        _ => return ExitCode::from(2),
    };
    let req = match op {
        "setup" | "teardown" => json!({"config": config, "iface": iface}),
        "renew" => json!({"config": config}),
        _ => return ExitCode::from(2),
    };
    ok_of(&bus::call("f50.proto", op, &req, Duration::from_secs(30)).await)
}

async fn hotplug(args: &[String]) -> ExitCode {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let (method, req) = match args.first().map(String::as_str) {
        Some("wcn") => return boot::hotplug_wcn().await,
        Some("iface") => ("hotplug_iface", json!({"action": var("ACTION"), "interface": var("INTERFACE")})),
        Some("net") => ("hotplug_net", json!({"action": var("ACTION"), "interface": var("INTERFACE")})),
        Some("ntp") if var("ACTION") == "stratum" => ("clock_synced", json!({})),
        Some("ntp") => return ExitCode::SUCCESS,
        _ => return ExitCode::from(2),
    };
    // a hotplug event for an interface f50d does not care about, or f50d not running, is no failure of the hook
    let _ = bus::call("f50.daemon", method, &req, Duration::from_secs(30)).await;
    ExitCode::SUCCESS
}
