//! procd services (`/etc/init.d/<name>`): actions through the init script, boot links in `/etc/rc.d`, running
//! instances through `ubus call service list`.

use std::time::Duration;

use f50_core::Result;
use f50_core::exec::{Cmd, executable};
use serde_json::{Value, json};

use crate::Platform;
use crate::check::{is_object_name, require};

/// How long an init script may take (a restart of the network or Wi-Fi waits for its daemons).
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct Services {
    platform: Platform,
}

/// What can be done to a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    Reload,
    Enable,
    Disable,
}

impl Action {
    /// The init script's verb.
    pub const fn as_str(self) -> &'static str {
        match self {
            Action::Start => "start",
            Action::Stop => "stop",
            Action::Restart => "restart",
            Action::Reload => "reload",
            Action::Enable => "enable",
            Action::Disable => "disable",
        }
    }
}

impl Services {
    pub(crate) fn new(platform: Platform) -> Self {
        Services { platform }
    }

    /// `/etc/init.d/<name> <action>`; `unsupported` when the service is not installed.
    pub async fn run(&self, name: &str, action: Action) -> Result<()> {
        require(is_object_name(name), "service")?;
        let script = self.platform.paths().sys(format!("/etc/init.d/{name}"));
        if !executable(&script) {
            return Err(f50_core::Error::unsupported(format!("service {name}")));
        }
        Cmd::new(script)
            .arg(action.as_str())
            .timeout(SCRIPT_TIMEOUT)
            .ok()
            .await
            .map(drop)
            .map_err(|e| e.context(format!("{name} {}", action.as_str())))
    }

    /// The init script is there.
    pub fn installed(&self, name: &str) -> bool {
        is_object_name(name) && executable(&self.platform.paths().sys(format!("/etc/init.d/{name}")))
    }

    /// Enabled at boot (an `/etc/rc.d/S??<name>` link).
    pub fn enabled(&self, name: &str) -> bool {
        if !is_object_name(name) {
            return false;
        }
        let Ok(dir) = std::fs::read_dir(self.platform.paths().sys("/etc/rc.d")) else { return false };
        dir.filter_map(std::result::Result::ok).any(|e| is_start_link(&e.file_name().to_string_lossy(), name))
    }

    /// procd has a running instance of it (`ubus call service list`).
    pub async fn running(&self, name: &str) -> Result<bool> {
        require(is_object_name(name), "service")?;
        let reply = self.platform.call("service", "list", &json!({ "name": name })).await?;
        Ok(any_running(&reply, name))
    }

    /// Runs `action` and logs a failure instead of returning it: for the follow-ups of a change that is already
    /// saved (`/etc/init.d/system reload` after a hostname).
    pub async fn run_logged(&self, name: &str, action: Action) {
        if let Err(e) = self.run(name, action).await {
            tracing::warn!(service = name, action = action.as_str(), error = %e, "service action failed");
        }
    }
}

/// `S` + two digits + `name`.
fn is_start_link(file: &str, name: &str) -> bool {
    let b = file.as_bytes();
    b.len() == 3 + name.len() && b[0] == b'S' && b[1].is_ascii_digit() && b[2].is_ascii_digit() && &file[3..] == name
}

/// A `service list` reply has an instance of `name` with `"running": true`.
pub fn any_running(reply: &Value, name: &str) -> bool {
    reply
        .get(name)
        .and_then(|s| s.get("instances"))
        .and_then(Value::as_object)
        .is_some_and(|i| i.values().any(|inst| inst.get("running").and_then(Value::as_bool) == Some(true)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_instances() {
        let r = json!({"dropbear": {"instances": {"instance1": {"running": true, "pid": 1234}}}});
        assert!(any_running(&r, "dropbear"));
        assert!(!any_running(&json!({"dropbear": {"instances": {"i": {"running": false}}}}), "dropbear"));
        assert!(!any_running(&json!({}), "dropbear"));
        assert!(!any_running(&json!({"dropbear": {}}), "dropbear"));
    }

    #[test]
    fn start_links() {
        assert!(is_start_link("S50dropbear", "dropbear"));
        assert!(!is_start_link("K50dropbear", "dropbear"));
        assert!(!is_start_link("S50dropbear2", "dropbear"));
        assert!(!is_start_link("S5dropbear", "dropbear"));
    }

    #[test]
    fn enabled_reads_rc_d() {
        let root = std::env::temp_dir().join(format!("f50-platform-rcd-{}", std::process::id()));
        std::fs::create_dir_all(root.join("etc/rc.d")).expect("mkdir");
        std::fs::write(root.join("etc/rc.d/S19f50d"), "").expect("write");
        let p = Platform::new(f50_core::paths::Paths::new(&root), None);
        assert!(p.services().enabled("f50d"));
        assert!(!p.services().enabled("dropbear"));
        assert!(!p.services().enabled("../x"));
        let _ = std::fs::remove_dir_all(root);
    }
}
