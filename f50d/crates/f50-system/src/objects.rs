//! The method tables: `f50.system`, `f50.cpu`, `f50.led`, `f50.shell`. Fields are checked by f50-core before a handler runs.

use std::future::Future;

use f50_core::Result;
use f50_core::rpc::{Access, Field, Object, Params, Rule};
use serde_json::Value;

use crate::System;
use crate::admin::{HOSTNAME_RE, ZONE_RE};

const NONE: &[Field] = &[];
const HOSTNAME: &[Field] = &[Field::req("name", Rule::Word { re: HOSTNAME_RE, max: 63 })];
const ZONE: &[Field] = &[Field::req("zone", Rule::Word { re: ZONE_RE, max: 64 })];
const PASSWORD: &[Field] = &[Field::secret("password", Rule::Text { max_bytes: 128, max_chars: 128 }, true)];
const LOGS: &[Field] = &[Field::opt("lines", Rule::Int { min: 10, max: 2000 })];
const PROFILE: &[Field] = &[Field::req("name", Rule::Enum(crate::cpu::PROFILES))];
const SHELL_ID: Field = Field::req("id", Rule::Word { re: "[0-9a-f]{16}", max: 16 });
const SHELL_OPEN: &[Field] =
    &[Field::opt("cols", Rule::Int { min: 2, max: 1000 }), Field::opt("rows", Rule::Int { min: 2, max: 500 })];
const SHELL_SESSION: &[Field] = &[SHELL_ID];
const SHELL_READ: &[Field] = &[
    SHELL_ID,
    // up to JavaScript's largest exact integer: a page keeps the offset as a number
    Field::opt("offset", Rule::Int { min: 0, max: 9_007_199_254_740_991 }),
    Field::opt("wait", Rule::Int { min: 0, max: crate::shell::WAIT_MAX_MS }),
];
// typed input may be a password: a secret, never logged
const SHELL_WRITE: &[Field] =
    &[SHELL_ID, Field::secret("data", Rule::Word { re: "[A-Za-z0-9+/]*={0,2}", max: 65536 }, true)];
const SHELL_RESIZE: &[Field] = &[
    SHELL_ID,
    Field::req("cols", Rule::Int { min: 2, max: 1000 }),
    Field::req("rows", Rule::Int { min: 2, max: 500 }),
];
const LED_SET: &[Field] = &[
    Field::req("key", Rule::Enum(crate::led::settings::KEYS)),
    Field::req("value", Rule::WordNum { re: "[a-z0-9]{1,8}", max: 8 }),
];

/// A handler over a clone of the service.
fn h<F, Fut>(s: &System, f: F) -> impl Fn(Params) -> Fut + Send + Sync + 'static
where
    F: Fn(System, Params) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value>> + Send + 'static,
{
    let s = s.clone();
    move |p| f(s.clone(), p)
}

pub(crate) fn objects(s: &System) -> Vec<Object> {
    let system = Object::builder("f50.system")
        .method(
            "info",
            Access::Read,
            NONE,
            "The device's facts and live figures.",
            h(s, |s, _| async move { s.info().await }),
        )
        .method(
            "status",
            Access::Read,
            NONE,
            "The home page's non-modem figures.",
            h(s, |s, _| async move { s.status().await }),
        )
        .method(
            "clients",
            Access::Write,
            NONE,
            "The hotspot's clients and the DHCP leases.",
            h(s, |s, _| async move { s.clients().await }),
        )
        .method(
            "set_hostname",
            Access::Write,
            HOSTNAME,
            "Sets the hostname.",
            h(s, |s, p| async move { s.set_hostname(p.need_str("name")?).await }),
        )
        .method(
            "set_timezone",
            Access::Write,
            ZONE,
            "Sets the time zone.",
            h(s, |s, p| async move { s.set_timezone(p.need_str("zone")?).await }),
        )
        .method(
            "set_password",
            Access::Write,
            PASSWORD,
            "Sets root's password.",
            h(s, |s, p| async move { s.set_password(p.need_str("password")?).await }),
        )
        .method("reboot", Access::Write, NONE, "Reboots in two seconds.", h(s, |s, _| async move { s.reboot() }))
        .method("poweroff", Access::Write, NONE, "Powers off in two seconds.", h(s, |s, _| async move { s.poweroff() }))
        .method(
            "logs",
            Access::Write,
            LOGS,
            "The kernel's and the system log's last lines.",
            h(s, |s, p| async move {
                let n = p.int("lines").and_then(|n| usize::try_from(n).ok()).unwrap_or(300);
                s.logs(n).await
            }),
        )
        .method(
            "logs_mark",
            Access::Read,
            NONE,
            "A fingerprint of the newest log lines; changes when the logs do, reveals nothing of them.",
            h(s, |s, _| async move { s.logs_mark().await }),
        )
        .build();
    let cpu = Object::builder("f50.cpu")
        .method(
            "get",
            Access::Read,
            NONE,
            "The CPU profile, the policies and the thermal zones.",
            h(s, |s, _| async move { Ok(s.inner.cpu.status(*s.inner.alarm.borrow())) }),
        )
        .method(
            "profile",
            Access::Write,
            PROFILE,
            "Applies and saves a CPU profile.",
            h(s, |s, p| async move {
                s.inner.cpu.set_profile(p.need_str("name")?)?;
                Ok(s.inner.cpu.status(*s.inner.alarm.borrow()))
            }),
        )
        .build();
    let led = Object::builder("f50.led")
        .method("get", Access::Read, NONE, "The LED settings.", h(s, |s, _| async move { Ok(s.inner.leds.get()) }))
        .method(
            "set",
            Access::Write,
            LED_SET,
            "Changes one LED setting.",
            h(s, |s, p| async move { s.inner.leds.set(p.need_str("key")?, p.need_str("value")?) }),
        )
        .build();
    vec![system, cpu, led, shell(s)]
}

/// A terminal dimension the field rules already bounded.
fn dim(p: &Params, name: &str, default: u16) -> u16 {
    p.int(name).and_then(|n| u16::try_from(n).ok()).unwrap_or(default)
}

fn shell(s: &System) -> Object {
    Object::builder("f50.shell")
        .method(
            "open",
            Access::Write,
            SHELL_OPEN,
            "Starts a root login shell on a new terminal.",
            h(s, |s, p| async move { s.inner.shell.open(dim(&p, "cols", 80), dim(&p, "rows", 24)) }),
        )
        .method(
            "read",
            Access::Write,
            SHELL_READ,
            "A shell's output from an offset on (base64), waiting up to `wait` ms for some.",
            h(s, |s, p| async move {
                let offset = p.int("offset").and_then(|n| u64::try_from(n).ok());
                let wait = p.int("wait").and_then(|n| u64::try_from(n).ok()).unwrap_or(0);
                s.inner.shell.read(p.need_str("id")?, offset, std::time::Duration::from_millis(wait)).await
            }),
        )
        .method(
            "write",
            Access::Write,
            SHELL_WRITE,
            "Types input (base64) into a shell.",
            h(s, |s, p| async move { s.inner.shell.write(p.need_str("id")?, p.need_str("data")?).await }),
        )
        .method(
            "resize",
            Access::Write,
            SHELL_RESIZE,
            "Sets a shell's terminal size.",
            h(
                s,
                |s, p| async move { s.inner.shell.resize(p.need_str("id")?, dim(&p, "cols", 80), dim(&p, "rows", 24)) },
            ),
        )
        .method(
            "close",
            Access::Write,
            SHELL_SESSION,
            "Hangs a shell up.",
            h(s, |s, p| async move { s.inner.shell.close(p.need_str("id")?) }),
        )
        .method("list", Access::Write, NONE, "The open shells.", h(s, |s, _| async move { Ok(s.inner.shell.list()) }))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use f50_core::rpc::matches;

    #[test]
    fn patterns_compile_and_match() {
        assert!(matches(HOSTNAME_RE, "OpenWrt"));
        assert!(matches(ZONE_RE, "America/Argentina/Buenos_Aires") && matches(ZONE_RE, "UTC"));
        assert!(matches("[a-z0-9]{1,8}", "default") && matches("[a-z0-9]{1,8}", "120"));
    }
}
