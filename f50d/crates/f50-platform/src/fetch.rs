//! HTTP(S) downloads through the image's `uclient-fetch`. URLs are checked here: `http://` or `https://`, no blank
//! or control character, so the program gets one argument that never starts with "-".
//!
//! The retry and resume rules are `f50-update`'s `fetch_stdout`/`fetch_file`:
//!
//! - a small text is tried up to [`TEXT_TRIES`] times, [`TEXT_DELAY`] apart, but only while the failure is the
//!   network's (`uclient-fetch` exit 4, or no answer in time); an HTTP error (exit 8) is the server's answer and
//!   asking again changes nothing;
//! - a file is resumed (`-c`) from what is already there. An attempt that failed after bytes arrived is a source
//!   that answered: it is resumed again rather than given up. A failure that brought nothing and was not the
//!   network's means the partial file cannot be resumed (a release published again, a stale part): it is removed
//!   and the download starts over once;
//! - everything stays inside the caller's `timeout`; dropping the future kills `uclient-fetch` (the partial file
//!   stays, to be resumed by the next call). The caller verifies what arrived (SHA256SUMS).
//!
//! `uclient-fetch` flags used (OpenWrt `uclient`): `-O <file>` (`-` for stdout), `-T <seconds>` (network timeout of
//! each step), `-c` (continue). It follows redirects itself (GitHub's release assets are redirects to its CDN).

use std::path::Path;
use std::time::Duration;

use f50_core::exec::{Cmd, Output};
use f50_core::{Code, Error, Result};
use tokio::time::Instant;

use crate::Platform;
use crate::check::refused;

/// Tries of a small text (the first and three more, as `fetch_stdout`).
pub const TEXT_TRIES: u32 = 4;
/// The pause between them.
pub const TEXT_DELAY: Duration = Duration::from_secs(4);
/// Attempts at a file within its timeout.
pub const FILE_TRIES: u32 = 6;
/// The pause between them.
pub const FILE_DELAY: Duration = Duration::from_secs(5);
/// The longest URL accepted.
pub const URL_MAX: usize = 2048;
/// `uclient-fetch -T`: how long one network step (connect, a read) may stall.
const STEP_TIMEOUT: Duration = Duration::from_secs(60);
/// `uclient-fetch`'s exit status for a network failure (connect, resolve, a read timed out).
const EXIT_NETWORK: i32 = 4;

#[derive(Debug, Clone)]
pub struct Fetch {
    #[allow(dead_code)] // the handle every capability gets; downloads need nothing of it yet
    platform: Platform,
}

impl Fetch {
    pub(crate) fn new(platform: Platform) -> Self {
        Fetch { platform }
    }

    /// A small text resource (a release index, SHA256SUMS), at most `max` bytes.
    pub async fn text(&self, url: &str, timeout: Duration, max: usize) -> Result<String> {
        check_url(url)?;
        let deadline = Instant::now() + timeout;
        let mut try_no = 1;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::timeout("download"));
            }
            let out = fetch_cmd(left).arg("-O").arg("-").arg(url).max_output(max.saturating_add(1)).output().await?;
            if out.success() {
                if out.stdout.len() > max {
                    return Err(Error::failed(format!("download larger than {max} bytes")));
                }
                return Ok(out.stdout);
            }
            if !network_failure(&out) || try_no >= TEXT_TRIES || !pause(deadline, TEXT_DELAY).await {
                return Err(error(&out));
            }
            try_no += 1;
        }
    }

    /// A file to `dest` (resumed when a partial one is there). Cancel by dropping the future.
    pub async fn file(&self, url: &str, dest: &Path, timeout: Duration) -> Result<()> {
        check_url(url)?;
        check_dest(dest)?;
        let deadline = Instant::now() + timeout;
        let mut started_over = false;
        let mut last = Error::timeout("download");
        for _ in 0..FILE_TRIES {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let had = size(dest);
            let out = fetch_cmd(left).arg("-c").arg("-O").arg(dest).arg(url).output().await?;
            if out.success() {
                return Ok(());
            }
            last = error(&out);
            let progressed = size(dest) > had;
            if !progressed && !network_failure(&out) {
                // the server's answer, with nothing new: a part that cannot be resumed goes, once
                if had > 0 && !started_over {
                    started_over = true;
                    remove(dest);
                } else {
                    remove(dest);
                    return Err(last);
                }
            }
            if !pause(deadline, FILE_DELAY).await {
                break;
            }
        }
        Err(last)
    }
}

/// `uclient-fetch -T <step>` with the run bounded by `left`.
fn fetch_cmd(left: Duration) -> Cmd {
    let step = STEP_TIMEOUT.min(left).as_secs().max(1);
    Cmd::new("uclient-fetch").arg("-T").arg(step.to_string()).timeout(left)
}

/// A failure worth asking again: the network's, or no answer in time.
fn network_failure(out: &Output) -> bool {
    out.timed_out || out.status == Some(EXIT_NETWORK)
}

fn error(out: &Output) -> Error {
    if out.timed_out {
        return Error::timeout("download");
    }
    let m = out.message();
    let e = Error::failed(if m.is_empty() { format!("download failed (uclient-fetch {:?})", out.status) } else { m });
    if network_failure(out) { e.context("source unreachable") } else { e }
}

/// Sleeps `d` unless that would pass `deadline`; `false` when there is no time left for another try.
async fn pause(deadline: Instant, d: Duration) -> bool {
    if Instant::now() + d >= deadline {
        return false;
    }
    tokio::time::sleep(d).await;
    true
}

fn size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn remove(p: &Path) {
    if let Err(e) = std::fs::remove_file(p)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %p.display(), error = %e, "could not remove a partial download");
    }
}

/// `http://` or `https://`, a host, no blank or control character, at most [`URL_MAX`] bytes.
pub fn check_url(url: &str) -> Result<()> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    let ok = url.len() <= URL_MAX
        && rest.is_some_and(|r| !r.is_empty() && !r.starts_with('/'))
        && url.bytes().all(|b| b.is_ascii_graphic());
    if ok { Ok(()) } else { Err(Error::field(Code::BadValue, "url", "url: not an http(s) address")) }
}

/// The destination: an absolute path, not a link (uclient-fetch would follow one), in a directory that exists.
fn check_dest(dest: &Path) -> Result<()> {
    if !dest.is_absolute() || dest.file_name().is_none() {
        return Err(refused("download destination"));
    }
    if std::fs::symlink_metadata(dest).is_ok_and(|m| !m.is_file()) {
        return Err(Error::failed(format!("{} is not a regular file", dest.display())));
    }
    match dest.parent() {
        Some(d) if d.is_dir() => Ok(()),
        _ => Err(Error::failed(format!("{}: no such directory", dest.display()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert!(check_url("https://github.com/x/y/releases/download/v1/SHA256SUMS").is_ok());
        assert!(check_url("http://192.168.0.2:8080/a").is_ok());
        assert!(check_url("-O/etc/shadow").is_err());
        assert!(check_url("ftp://x").is_err());
        assert!(check_url("https://").is_err());
        assert!(check_url("https:///etc").is_err());
        assert!(check_url("https://x/a b").is_err());
        assert!(check_url("https://x/\n").is_err());
        assert!(check_url(&format!("https://x/{}", "a".repeat(URL_MAX))).is_err());
    }

    #[test]
    fn failures() {
        let o = |status, timed_out| Output { status, stdout: String::new(), stderr: String::new(), timed_out };
        assert!(network_failure(&o(Some(4), false)));
        assert!(network_failure(&o(None, true)));
        assert!(!network_failure(&o(Some(8), false)));
        assert_eq!(error(&o(None, true)).code(), Code::Timeout);
        assert_eq!(error(&o(Some(8), false)).code(), Code::Failed);
    }

    #[test]
    fn destinations() {
        assert!(check_dest(Path::new("relative")).is_err());
        assert!(check_dest(Path::new("/nonexistent-dir/file")).is_err());
        assert!(check_dest(&std::env::temp_dir().join("f50-fetch-test")).is_ok());
        assert!(check_dest(&std::env::temp_dir()).is_err());
    }
}
