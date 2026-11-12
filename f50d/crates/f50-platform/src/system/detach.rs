//! A delayed reboot or power-off that outlives f50d.
//!
//! `/bin/sh` is started in a new session (`setsid` in the child before exec), puts `sleep N; sync; exec <program>`
//! in the background and exits at once; the background shell is reparented to procd, out of f50d's process group and
//! session, so procd stopping f50d (or f50d exiting) does not take it along. Its stdin, stdout and stderr are
//! `/dev/null`, its environment is only `PATH`; every descriptor f50d holds is close-on-exec (std and tokio open
//! them all with `O_CLOEXEC`), so none is inherited.
//!
//! This is the one place in the crate that starts a program outside `f50_core::exec::Cmd`: `Cmd` puts the program in
//! a process group that is killed when the run ends, which is exactly what must not happen here.
#![allow(unsafe_code)]

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use f50_core::exec::CHILD_PATH;
use f50_core::{Error, Result};

/// The script the session leader runs: `$1` seconds, then `$2`.
const SCRIPT: &str = r#"(sleep "$1"; sync; exec "$2") </dev/null >/dev/null 2>&1 &"#;

/// The longest delay accepted (a reply leaving first needs seconds, not hours).
pub(crate) const MAX_DELAY: Duration = Duration::from_secs(3600);

/// Runs `program` (an absolute path) after `delay`, detached; returns once the background shell is started.
pub(crate) fn after(delay: Duration, program: &str) -> Result<()> {
    if delay > MAX_DELAY {
        return Err(crate::check::refused("delay"));
    }
    let secs = delay.as_secs() + u64::from(delay.subsec_nanos() > 0);
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", SCRIPT, "f50d-later", &secs.to_string(), program])
        .env_clear()
        .env("PATH", CHILD_PATH)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the closure runs in the forked child before exec and only calls setsid(2), which is
    // async-signal-safe, allocates nothing and touches no memory shared with the parent. It cannot fail here
    // (the child is not a process group leader), but a failure is reported as an error rather than ignored.
    unsafe {
        cmd.pre_exec(|| if libc::setsid() == -1 { Err(io::Error::last_os_error()) } else { Ok(()) });
    }
    // the session leader only forks the background shell and exits: waiting for it is short and bounded
    let status = cmd.status().map_err(|e| Error::io(format!("start {program} later"), &e))?;
    if status.success() { Ok(()) } else { Err(Error::failed(format!("could not schedule {program}"))) }
}
