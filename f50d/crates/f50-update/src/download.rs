//! The files of one release, verified, in the staging directory: from the network (GitHub or a mirror) or from an
//! offline bundle. The shell's `fetch_verified`, as the shell reasoned it:
//!
//! - `SHA256SUMS` is fetched once per run and judged once ([`verify`](crate::release::verify)); every later file is
//!   checked against the pinned bytes, or the run stops.
//! - A file already staged that matches is not downloaded again; a partial one is resumed (the platform's fetch
//!   resumes). A transfer that stalls is cut by the fetch's timeout and tried again: three rounds, the checksums
//!   fetched again after a file that does not match (a release published again under the same tag).
//! - The network's own failures are retried by the platform's fetch (`uclient-fetch`, the shell's rules), within
//!   each call's timeout; an HTTP error is the source's answer.
//! - A mirror that answers nothing is left for GitHub once, where the source allows it ([`Source::fall_back`]).

use std::path::{Path, PathBuf};
use std::time::Duration;

use f50_core::jobs::JobCtx;
use f50_core::{Code, Error, Result, fsx};
use f50_platform::fetch::Fetch;

use crate::offline::Bundle;
use crate::release::assets;
use crate::release::sig::{self, Key, SigState};
use crate::release::source::Source;
use crate::release::verify::{self, Origin, Sums};
use crate::util::{blocking, file_len, sha256_file, sha256_hex};

const TEXT_MAX: usize = 256 * 1024;
const TEXT_TIMEOUT: Duration = Duration::from_secs(60);
const FILE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const FILE_ROUNDS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(4);

/// Where the release's files come from.
#[derive(Debug)]
pub enum Provider {
    Online { fetch: Fetch, source: Source },
    Offline(Bundle),
}

/// What the run is told about the key and the tools.
#[derive(Debug, Clone)]
pub struct Trust {
    /// The key file (device path under the root).
    pub key_file: PathBuf,
    /// The image's usign (device path under the root).
    pub usign: PathBuf,
    /// The owner said yes to an unsigned release, for this run.
    pub allow_unsigned: bool,
}

/// The release files of one run.
#[derive(Debug)]
pub struct Fetcher {
    provider: Provider,
    stage: PathBuf,
    trust: Trust,
    sums: Option<(String, Sums)>,
    log: Option<JobCtx>,
}

impl Fetcher {
    pub fn new(provider: Provider, stage: PathBuf, trust: Trust, log: Option<JobCtx>) -> Self {
        Fetcher { provider, stage, trust, sums: None, log }
    }

    fn say(&self, line: &str) {
        tracing::info!("{line}");
        if let Some(l) = &self.log {
            l.log(line);
        }
    }

    /// A small file of the release: `Ok(None)` when the source answered without it (an HTTP error); a source that
    /// did not answer at all is left for GitHub once, where allowed. The platform's fetch retries the network.
    async fn text(&mut self, tag: &str, name: &str) -> Result<Option<Vec<u8>>> {
        match &mut self.provider {
            Provider::Offline(b) => {
                let b = b.clone();
                let name = name.to_owned();
                blocking(move || b.read(&name, TEXT_MAX as u64)).await
            }
            Provider::Online { fetch, source } => loop {
                match fetch.text(&source.file(tag, name), TEXT_TIMEOUT, TEXT_MAX).await {
                    Ok(s) => return Ok(Some(s.into_bytes())),
                    Err(e) if unreachable(&e) => {
                        if !source.fall_back() {
                            return Err(e);
                        }
                    }
                    Err(e) if e.code() == Code::Failed => return Ok(None),
                    Err(e) => return Err(e),
                }
            },
        }
    }

    /// The checksums of `tag`, fetched, judged and pinned (once a run; again after `refresh`).
    pub async fn release(&mut self, tag: &str) -> Result<()> {
        if self.sums.as_ref().is_some_and(|(t, _)| t == tag) {
            return Ok(());
        }
        let origin = if matches!(self.provider, Provider::Offline(_)) { Origin::Offline } else { Origin::Online };
        let text = self
            .text(tag, assets::SUMS)
            .await
            .map_err(|e| e.context(format!("cannot download the checksums of {tag}")))?
            .ok_or_else(|| Error::not_found(format!("release {tag} (it has no {})", assets::SUMS)))?;
        let sums =
            Sums::parse(&text).ok_or_else(|| Error::failed(format!("the checksums of {tag} are not readable")))?;
        let sums_path = self.stage.join(assets::SUMS);
        fsx::write_atomic(&sums_path, &text, 0o600)?;
        let state = self.signature(tag, &sums_path, &text).await;
        let line = verify::verdict(tag, state, &sums, origin, self.trust.allow_unsigned)?;
        // the bytes judged are the ones on the disk
        if sha256_hex(&std::fs::read(&sums_path).map_err(|e| Error::io("read SHA256SUMS", &e))?) != sums.pin {
            return Err(Error::failed("the checksums changed while they were checked"));
        }
        self.say(&line);
        self.sums = Some((tag.to_owned(), sums));
        Ok(())
    }

    async fn signature(&mut self, tag: &str, sums_path: &Path, sums: &[u8]) -> SigState {
        let key_text = match sig::key(&self.trust.key_file) {
            Key::None => return SigState::NoKey,
            Key::Unsafe => return SigState::BadKey,
            Key::Ok(k) => k,
        };
        if let Ok(Some(m)) = self.text(tag, assets::SUMS_MINISIG).await {
            return sig::check_minisign(&key_text, &String::from_utf8_lossy(&m), sums);
        }
        let Ok(Some(s)) = self.text(tag, assets::SUMS_SIG).await else { return SigState::Unsigned };
        let sig_path = self.stage.join(assets::SUMS_SIG);
        if fsx::write_atomic(&sig_path, &s, 0o600).is_err() {
            return SigState::NoTool;
        }
        sig::check_usign(&self.trust.usign, &self.trust.key_file, &key_text, sums_path, &sig_path).await
    }

    /// The pinned checksums, still the bytes that were judged.
    fn pinned(&self, tag: &str) -> Result<&Sums> {
        let (t, s) =
            self.sums.as_ref().filter(|(t, _)| t == tag).ok_or_else(|| Error::failed("checksums not judged"))?;
        let now = std::fs::read(self.stage.join(assets::SUMS)).map(|b| sha256_hex(&b)).unwrap_or_default();
        if now != s.pin {
            return Err(Error::failed(format!(
                "the checksums of {t} changed after they were checked: nothing more is taken from them"
            )));
        }
        Ok(s)
    }

    /// Release `tag`'s file `name`, verified, in the staging directory; `None` when the release has no such file.
    pub async fn file(&mut self, tag: &str, name: &str) -> Result<Option<PathBuf>> {
        self.release(tag).await?;
        let Some(want) = self.pinned(tag)?.want(name).map(str::to_owned) else { return Ok(None) };
        let dest = self.stage.join(name);
        if matches(&dest, &want).await {
            return Ok(Some(dest));
        }
        self.say(&format!("downloading {name}"));
        let mut last = None;
        for _round in 0..FILE_ROUNDS {
            let r = match &mut self.provider {
                Provider::Offline(b) => {
                    let (b, n, d) = (b.clone(), name.to_owned(), dest.clone());
                    blocking(move || b.extract(&n, &d)).await.and_then(|found| {
                        if found { Ok(()) } else { Err(Error::not_found(format!("{name} in the bundle"))) }
                    })
                }
                Provider::Online { fetch, source } => {
                    let had = file_len(&dest);
                    let r = fetch.file(&source.file(tag, name), &dest, FILE_TIMEOUT).await;
                    // nothing came from a mirror at all: the rest of the run from GitHub, where allowed
                    if r.as_ref().is_err_and(unreachable) && file_len(&dest) <= had && source.fall_back() {
                        continue;
                    }
                    r
                }
            };
            match r {
                Ok(()) if matches(&dest, &want).await => return Ok(Some(dest)),
                Ok(()) => {
                    // a corrupt or stale file cannot be resumed: start over
                    let _ = std::fs::remove_file(&dest);
                    last = Some(Error::failed(format!("{name} does not match the release's checksum")));
                    if matches!(self.provider, Provider::Offline(_)) {
                        break;
                    }
                }
                Err(e) => last = Some(e),
            }
            tokio::time::sleep(RETRY_DELAY).await;
        }
        Err(last.unwrap_or_else(|| Error::failed(format!("downloading {name} failed"))).context(name))
    }
}

/// The platform's fetch says a source did not answer (its "source unreachable", or no answer in time).
fn unreachable(e: &Error) -> bool {
    e.code() == Code::Timeout || e.message().starts_with("source unreachable")
}

async fn matches(path: &Path, want: &str) -> bool {
    if file_len(path) == 0 {
        return false;
    }
    let p = path.to_owned();
    blocking(move || sha256_file(&p).map_err(|e| Error::io("hash", &e))).await.is_ok_and(|h| h == want)
}
