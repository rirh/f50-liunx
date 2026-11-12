//! The subprocess runner: every device tool runs through here.
//!
//! A tool runs in its own process group with a hard timeout (the whole group is killed), its output is read up to
//! a bound and the rest drained, a secret goes on its stdin, and its failure becomes a sanitised one-line message.
//! The group is also killed when the caller goes away (an HTTP client that disconnects drops the future).

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::api::error::{ApiError, ApiResult, Code};
use crate::config::Config;
use crate::sanitize::{last_message, sanitize_message};

/// Output kept from one stream of a tool.
pub const OUTPUT_MAX: usize = 256 * 1024;

/// One run of a tool, built up and then `run`.
#[derive(Debug)]
pub struct Tool {
    program: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    max_out: usize,
    path: OsString,
}

/// What a tool did.
#[derive(Debug, Clone)]
pub struct Output {
    /// The exit status; None when it ended by a signal.
    pub rc: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl Output {
    pub fn success(&self) -> bool {
        self.rc == Some(0) && !self.timed_out
    }

    /// The reason for a failure, sanitised: the last message on stderr, else the last line on stdout.
    pub fn message(&self) -> String {
        if self.timed_out {
            return "timed out".into();
        }
        let m = last_message(&self.stderr);
        let m = if m.is_empty() { last_message(&self.stdout) } else { m };
        sanitize_message(&m)
    }

    /// The error for this run when it did not succeed.
    pub fn error(&self) -> ApiError {
        if self.timed_out {
            ApiError::new(Code::Timeout, "the tool did not finish in its time")
        } else {
            ApiError::failed(self.message())
        }
    }

    /// Ok(self) when it succeeded, else its error.
    pub fn check(self) -> ApiResult<Output> {
        if self.success() { Ok(self) } else { Err(self.error()) }
    }
}

impl Tool {
    pub fn new(cfg: &Config, program: impl Into<PathBuf>) -> Self {
        Tool {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            stdin: None,
            timeout: Duration::from_secs(15),
            max_out: OUTPUT_MAX,
            path: cfg.child_path.clone(),
        }
    }

    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Self {
        self.args.push(a.as_ref().to_owned());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(mut self, it: I) -> Self {
        self.args.extend(it.into_iter().map(|a| a.as_ref().to_owned()));
        self
    }

    pub fn env(mut self, k: &str, v: impl AsRef<OsStr>) -> Self {
        self.env.push((k.into(), v.as_ref().to_owned()));
        self
    }

    /// Bytes for the tool's stdin (a secret goes here, never into the arguments).
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }

    pub fn timeout_secs(mut self, s: u64) -> Self {
        self.timeout = Duration::from_secs(s);
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Run it. Err only when it could not be started at all.
    pub async fn run(self) -> io::Result<Output> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .env("PATH", &self.path)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(if self.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = cmd.spawn()?;
        let pid = child.id().map(|p| p as libc::pid_t);
        let mut guard = GroupGuard(pid);

        if let (Some(data), Some(mut sin)) = (self.stdin, child.stdin.take()) {
            // a tool that never reads its stdin must not block us: the write is bounded by the tool's timeout
            tokio::spawn(async move {
                let _ = sin.write_all(&data).await;
                let _ = sin.shutdown().await;
            });
        }
        let out = tokio::spawn(read_bounded(child.stdout.take(), self.max_out));
        let err = tokio::spawn(read_bounded(child.stderr.take(), self.max_out));

        let (status, timed_out) = match tokio::time::timeout(self.timeout, child.wait()).await {
            Ok(st) => (st.ok(), false),
            Err(_) => {
                guard.kill(libc::SIGKILL);
                (child.wait().await.ok(), true)
            }
        };
        // what is left of the group (a helper it put in the background) goes too: it would hold the pipes open
        guard.kill(libc::SIGKILL);
        guard.0 = None;
        let grace = Duration::from_secs(2);
        let stdout = tokio::time::timeout(grace, out).await.ok().and_then(Result::ok).unwrap_or_default();
        let stderr = tokio::time::timeout(grace, err).await.ok().and_then(Result::ok).unwrap_or_default();
        Ok(Output { rc: status.and_then(|s| s.code()), stdout, stderr, timed_out })
    }

    /// Run it; a tool that cannot be started is `unsupported` when missing, `failed` otherwise.
    pub async fn output(self) -> ApiResult<Output> {
        let name = self.program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.run().await.map_err(|e| match e.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => ApiError::unsupported(&name),
            _ => ApiError::failed(format!("{name}: could not start")),
        })
    }

    /// Run it and require success.
    pub async fn ok(self) -> ApiResult<Output> {
        self.output().await?.check()
    }
}

/// Kills the tool's process group when dropped (the request went away) unless it already ended.
struct GroupGuard(Option<libc::pid_t>);

impl GroupGuard {
    fn kill(&mut self, sig: libc::c_int) {
        if let Some(p) = self.0 {
            // SAFETY: kill(2) with a negative pid signals the process group we created for this child.
            unsafe {
                libc::kill(-p, sig);
            }
        }
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        self.kill(libc::SIGKILL);
    }
}

async fn read_bounded<R: AsyncRead + Unpin>(r: Option<R>, max: usize) -> String {
    let Some(mut r) = r else { return String::new() };
    let mut kept = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = max.saturating_sub(kept.len());
                kept.extend_from_slice(&buf[..n.min(room)]);
            }
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// Start a program detached from us: a new session, stdio on /dev/null, reparented to init (a double fork through
/// sh), so it outlives this process and is never our zombie. Returns once it is started.
pub fn spawn_detached(cfg: &Config, argv: &[OsString], env: &[(&str, OsString)]) -> io::Result<()> {
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(r#""$@" </dev/null >/dev/null 2>&1 &"#)
        .arg("mu300d-detach")
        .args(argv)
        .env("PATH", &cfg.child_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    // SAFETY: setsid(2) is async-signal-safe and touches no memory of the parent.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let st = cmd.status()?;
    if st.success() { Ok(()) } else { Err(io::Error::other("could not start in the background")) }
}

/// mu300-api's `later`: CMD in a new session two seconds from now, after the answer has gone out (a restart of the
/// hotspot, the LAN, a reboot).
pub fn later(cfg: &Config, argv: &[&str]) -> ApiResult<()> {
    let mut v: Vec<OsString> =
        vec!["/bin/sh".into(), "-c".into(), r#"sleep 2; exec "$@""#.into(), "mu300d-later".into()];
    v.extend(argv.iter().map(OsString::from));
    spawn_detached(cfg, &v, &[]).map_err(|_| ApiError::failed("could not schedule the restart"))
}

/// The program is there and executable.
pub fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        let mut c = Config::from_env().expect("config");
        c.child_path = "/usr/bin:/bin".into();
        c
    }

    #[tokio::test]
    async fn timeout_kills_the_whole_group() {
        let dir = std::env::temp_dir().join(format!("mu300d-exec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pidf = dir.join("pid");
        let script = format!("sleep 30 & echo $! > '{}'; sleep 30", pidf.display());
        let t0 = std::time::Instant::now();
        let o =
            Tool::new(&cfg(), "/bin/sh").args(["-c", &script]).timeout(Duration::from_millis(500)).run().await.unwrap();
        assert!(o.timed_out);
        assert!(t0.elapsed() < Duration::from_secs(5));
        assert_eq!(o.error().code, Code::Timeout);
        let pid: i32 = std::fs::read_to_string(&pidf).unwrap().trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        // SAFETY: signal 0 only checks existence
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "the background child was killed too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn output_is_bounded_and_stdin_fed() {
        let o = Tool::new(&cfg(), "/bin/sh").args(["-c", "head -c 3000000 /dev/zero"]).run().await.unwrap();
        assert!(o.success());
        assert_eq!(o.stdout.len(), OUTPUT_MAX);
        let o = Tool::new(&cfg(), "/bin/cat").stdin("secret\n").run().await.unwrap();
        assert_eq!(o.stdout, "secret\n");
        let o = Tool::new(&cfg(), "/bin/sh").args(["-c", "echo 'tool: bad thing' >&2; exit 3"]).run().await.unwrap();
        assert_eq!(o.rc, Some(3));
        assert_eq!(o.check().unwrap_err().error, "tool: bad thing");
        let e = Tool::new(&cfg(), "/nonexistent/tool").output().await.unwrap_err();
        assert_eq!(e.code, Code::Unsupported);
    }
}
