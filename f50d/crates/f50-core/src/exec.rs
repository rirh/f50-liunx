//! The only way f50d starts a program.
//!
//! A program runs in its own process group with a hard timeout (the whole group is killed), its output is read up
//! to a bound and the rest drained, a secret goes on its stdin (never into the arguments, which any user can read in
//! /proc), and its failure becomes a sanitised one-line message. When the caller's future is dropped (a request that
//! went away, a cancelled job) the group is killed too.
#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::error::{Code, Error, Result};
use crate::sanitize::{last_message, sanitize_message};

/// Output kept from one stream of a program.
pub const OUTPUT_MAX: usize = 256 * 1024;

/// The PATH every program gets: OpenWrt's own directories only.
pub const CHILD_PATH: &str = "/usr/sbin:/usr/bin:/sbin:/bin";

/// One run of a program, built up and then `run`.
#[derive(Debug)]
pub struct Cmd {
    program: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    max_out: usize,
}

/// What a program did.
#[derive(Debug, Clone)]
pub struct Output {
    /// The exit status; `None` when it ended by a signal.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status == Some(0) && !self.timed_out
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
    pub fn error(&self, program: &str) -> Error {
        if self.timed_out {
            Error::timeout(program)
        } else {
            let m = self.message();
            Error::failed(if m.is_empty() { format!("{program} failed") } else { m })
        }
    }
}

impl Cmd {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Cmd {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            stdin: None,
            timeout: Duration::from_secs(15),
            max_out: OUTPUT_MAX,
        }
    }

    #[must_use]
    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Self {
        self.args.push(a.as_ref().to_owned());
        self
    }

    #[must_use]
    pub fn args<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(mut self, it: I) -> Self {
        self.args.extend(it.into_iter().map(|a| a.as_ref().to_owned()));
        self
    }

    #[must_use]
    pub fn env(mut self, k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) -> Self {
        self.env.push((k.as_ref().to_owned(), v.as_ref().to_owned()));
        self
    }

    /// Bytes for the program's stdin. A secret goes here, never into the arguments.
    #[must_use]
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }

    #[must_use]
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    #[must_use]
    pub fn max_output(mut self, bytes: usize) -> Self {
        self.max_out = bytes;
        self
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    fn name(&self) -> String {
        self.program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Runs it. `Err` only when it could not be started at all.
    pub async fn run(self) -> io::Result<Output> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .env_clear()
            .env("PATH", CHILD_PATH)
            .env("LC_ALL", "C")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(if self.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = cmd.spawn()?;
        let mut group = GroupGuard(child.id().and_then(|p| libc::pid_t::try_from(p).ok()));

        if let (Some(data), Some(mut sin)) = (self.stdin, child.stdin.take()) {
            // a program that never reads its stdin must not block us: the writer ends with the program
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
                group.kill();
                (child.wait().await.ok(), true)
            }
        };
        // what is left of the group (a helper it put in the background) goes too: it would hold the pipes open
        group.kill();
        group.0 = None;
        let grace = Duration::from_secs(2);
        let stdout = tokio::time::timeout(grace, out).await.ok().and_then(std::result::Result::ok).unwrap_or_default();
        let stderr = tokio::time::timeout(grace, err).await.ok().and_then(std::result::Result::ok).unwrap_or_default();
        Ok(Output { status: status.and_then(|s| s.code()), stdout, stderr, timed_out })
    }

    /// Runs it; a program that cannot be started is `unsupported` when missing, `failed` otherwise.
    pub async fn output(self) -> Result<Output> {
        let name = self.name();
        self.run().await.map_err(|e| match e.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied => Error::unsupported(&name),
            _ => Error::failed(format!("{name}: could not start: {e}")),
        })
    }

    /// Runs it and requires success; the value is its stdout.
    pub async fn ok(self) -> Result<String> {
        let name = self.name();
        let o = self.output().await?;
        if o.success() { Ok(o.stdout) } else { Err(o.error(&name)) }
    }
}

/// Kills the program's process group when dropped, unless it already ended.
struct GroupGuard(Option<libc::pid_t>);

impl GroupGuard {
    fn kill(&self) {
        if let Some(p) = self.0 {
            // SAFETY: kill(2) with a negative pid signals the process group created for this child (process_group(0)
            // made the child its leader); no memory is touched.
            unsafe {
                libc::kill(-p, libc::SIGKILL);
            }
        }
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        self.kill();
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

/// The program is a regular file with an execute bit.
pub fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// A refusal for a run that could not even start, for callers that build their own messages.
pub fn not_started(program: &str) -> Error {
    Error::new(Code::Unsupported, format!("{program} is not installed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_kills_the_whole_group() {
        let dir = std::env::temp_dir().join(format!("f50-exec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp");
        let pidf = dir.join("pid");
        let script = format!("sleep 30 & echo $! > '{}'; sleep 30", pidf.display());
        let t0 = std::time::Instant::now();
        let o = Cmd::new("/bin/sh").args(["-c", &script]).timeout(Duration::from_millis(500)).run().await.expect("run");
        assert!(o.timed_out);
        assert!(t0.elapsed() < Duration::from_secs(5));
        assert_eq!(o.error("sh").code(), Code::Timeout);
        let pid: i32 = std::fs::read_to_string(&pidf).expect("pid").trim().parse().expect("pid");
        std::thread::sleep(Duration::from_millis(200));
        // SAFETY: signal 0 only checks that the process exists
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "the background child was killed too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn output_is_bounded_and_stdin_fed() {
        let o = Cmd::new("/bin/sh").args(["-c", "head -c 3000000 /dev/zero"]).run().await.expect("run");
        assert!(o.success());
        assert_eq!(o.stdout.len(), OUTPUT_MAX);
        assert_eq!(Cmd::new("/bin/cat").stdin("secret\n").ok().await.expect("cat"), "secret\n");
        let e = Cmd::new("/bin/sh").args(["-c", "echo 'tool: bad thing' >&2; exit 3"]).ok().await.expect_err("fails");
        assert_eq!(e.message(), "tool: bad thing");
        assert_eq!(Cmd::new("/nonexistent/tool").output().await.expect_err("missing").code(), Code::Unsupported);
    }
}
