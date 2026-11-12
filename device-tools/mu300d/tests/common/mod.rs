//! A scratch device: a fake BIN of shell stubs, a fake PATH for system commands, a system root, a run directory.

#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

static N: AtomicU32 = AtomicU32::new(0);

pub struct Dev {
    pub root: PathBuf,
}

impl Drop for Dev {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub struct Reply {
    pub json: Value,
    pub status: i32,
}

impl Dev {
    pub fn new() -> Dev {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!("mu300d-t-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&root);
        for d in ["bin", "path", "sys/etc/mu300", "sys/proc/sys/kernel", "run", "dash", "unisoc", "log"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        let dev = Dev { root };
        fs::write(dev.p("sys/etc/os-release"), "ID=ubuntu\nVERSION_ID=\"24.04\"\n").unwrap();
        fs::write(dev.p("sys/proc/sys/kernel/hostname"), "mu300\n").unwrap();
        dev.bin("mu300-device", "echo f50");
        // system commands that would touch this Mac: recorded, never run
        for c in [
            "systemctl",
            "hostnamectl",
            "timedatectl",
            "uci",
            "ip",
            "iw",
            "ifstatus",
            "ifup",
            "ifdown",
            "wifi",
            "reboot",
        ] {
            dev.path_cmd(c, "");
        }
        dev
    }

    pub fn p(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn script(&self, at: &Path, name: &str, body: &str) {
        let log = self.p("log");
        let s = format!("#!/bin/sh\nprintf '%s\\n' \"{name} $*\" >> '{log}/calls'\n{body}\n", log = log.display());
        fs::write(at, s).unwrap();
        fs::set_permissions(at, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A device tool in BIN: logs "NAME ARGS" to log/calls, then runs BODY.
    pub fn bin(&self, name: &str, body: &str) {
        self.script(&self.p("bin").join(name), name, body);
    }

    /// A system command on the children's PATH.
    pub fn path_cmd(&self, name: &str, body: &str) {
        self.script(&self.p("path").join(name), name, body);
    }

    /// A control-panel adapter (cell, lock, device-usb).
    pub fn adapter(&self, name: &str, body: &str) {
        self.script(&self.p("unisoc").join(name), name, body);
    }

    /// Shell text that saves the tool's stdin as log/<name>.stdin.
    pub fn save_stdin(&self, name: &str) -> String {
        format!("cat > '{}'", self.p("log").join(format!("{name}.stdin")).display())
    }

    pub fn calls(&self) -> String {
        fs::read_to_string(self.p("log/calls")).unwrap_or_default()
    }

    pub fn stdin_of(&self, name: &str) -> String {
        fs::read_to_string(self.p("log").join(format!("{name}.stdin"))).unwrap_or_default()
    }

    pub fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_mu300d"));
        c.env_clear()
            .env("MU300_SYSROOT", self.p("sys"))
            .env("MU300_BIN", self.p("bin"))
            .env("MU300D_RUN", self.p("run"))
            .env("MU300_DASH_DIR", self.p("dash"))
            .env("MU300_UNISOC_DIR", self.p("unisoc"))
            .env("MU300D_PATH", format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", self.p("path").display()))
            .env("MU300D_NOROOT", "1");
        c
    }

    pub fn call_with(&self, method: &str, body: &str, extra: &[(&str, &str)]) -> Reply {
        let mut c = self.command();
        for (k, v) in extra {
            c.env(k, v);
        }
        let mut child = c
            .arg("call")
            .arg(method)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(body.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let json = serde_json::from_str(text.trim()).unwrap_or_else(|_| panic!("not JSON: {text:?} {:?}", out.stderr));
        Reply { json, status: out.status.code().unwrap_or(-1) }
    }

    pub fn call(&self, method: &str, body: &str) -> Reply {
        self.call_with(method, body, &[])
    }

    /// Poll job.status until the job is no longer running (or the deadline).
    pub fn wait_job(&self, id: &str, secs: u64) -> Value {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            let r = self.call("job.status", &format!("{{\"id\":\"{id}\"}}"));
            assert_eq!(r.json["ok"], true, "{}", r.json);
            if r.json["data"]["state"] != "running" || Instant::now() > end {
                return r.json["data"].clone();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

pub fn wait_until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}
