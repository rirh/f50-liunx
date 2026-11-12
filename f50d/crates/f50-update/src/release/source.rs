//! Where a run takes the releases from: GitHub, or a mirror named in the source file (root's own file, lines
//! `MU300_API_URL=`, `MU300_DOWNLOAD_URL=`, `MU300_MIRROR_FALLBACK=0|1`, as the shell read `/etc/mu300/update-source`).
//!
//! A mirror that cannot be reached is said once and the rest of that run comes from GitHub - but by itself only for
//! an https mirror on a public name. A plain-http mirror or one on a private address is almost always a copy on the
//! LAN, kept so the release does not come over a metered SIM: there the run stops and says why
//! (`MU300_MIRROR_FALLBACK=1` allows it, `0` forbids it for every mirror; FINDINGS 86, "When the mirror is not there").
//! With a release key a mirror cannot change a file; without one it is trusted as much as whoever runs it.

use std::path::Path;

use f50_core::rpc::matches;
use serde::Serialize;

use crate::config::UpdateConfig;
use crate::util::trusted_file;

const URL_RE: &str = "https?://[A-Za-z0-9._:/-]{1,200}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fallback {
    /// Only an https mirror on a public name falls back to GitHub.
    Auto,
    Never,
    Always,
}

/// The release source of one run.
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    api: String,
    download: String,
    #[serde(skip)]
    github_api: String,
    #[serde(skip)]
    github_download: String,
    /// A mirror is named (in the source file).
    pub mirror: bool,
    pub fallback: Fallback,
    /// This run found the mirror unreachable and uses GitHub.
    pub fell_back: bool,
}

impl Source {
    /// GitHub, or the mirror the source file names (`file`: the device path under the root). A file that is not
    /// root's own, or that others may write, is ignored with a line in the log.
    pub fn load(cfg: &UpdateConfig, file: &Path) -> Source {
        let mut s = Source {
            api: cfg.api_url.clone(),
            download: cfg.download_url.clone(),
            github_api: cfg.api_url.clone(),
            github_download: cfg.download_url.clone(),
            mirror: false,
            fallback: Fallback::Auto,
            fell_back: false,
        };
        if !file.exists() {
            return s;
        }
        if !trusted_file(file) {
            tracing::warn!(file = %file.display(), "the update source file is not root's own or others may write it: ignored");
            return s;
        }
        let Ok(text) = std::fs::read_to_string(file) else { return s };
        for line in text.lines().take(32) {
            let Some((k, v)) = line.trim().split_once('=') else { continue };
            match k {
                "MU300_API_URL" | "F50_API_URL" if matches(URL_RE, v) => {
                    s.api = v.trim_end_matches('/').to_owned();
                    s.mirror = true;
                }
                "MU300_DOWNLOAD_URL" | "F50_DOWNLOAD_URL" if matches(URL_RE, v) => {
                    s.download = v.trim_end_matches('/').to_owned();
                    s.mirror = true;
                }
                "MU300_MIRROR_FALLBACK" | "F50_MIRROR_FALLBACK" => {
                    s.fallback = match v {
                        "0" => Fallback::Never,
                        "1" => Fallback::Always,
                        _ => s.fallback,
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// An address of the API (`releases/latest`).
    pub fn api(&self, path: &str) -> String {
        format!("{}/{path}", self.api)
    }

    /// The address of a release's file.
    pub fn file(&self, tag: &str, name: &str) -> String {
        format!("{}/{tag}/{name}", self.download)
    }

    /// The mirror of this run could not be reached: GitHub for the rest of it, when that is allowed (true), else
    /// false and the run stays with the mirror. Said once in the log.
    pub fn fall_back(&mut self) -> bool {
        if !self.mirror || self.fell_back {
            return false;
        }
        let allowed = match self.fallback {
            Fallback::Never => false,
            Fallback::Always => true,
            Fallback::Auto => !mirror_local(&self.download) && !mirror_local(&self.api),
        };
        if allowed {
            tracing::warn!("mirror unreachable: the rest comes from github.com");
            self.api = self.github_api.clone();
            self.download = self.github_download.clone();
            self.fell_back = true;
        } else {
            tracing::warn!(
                "mirror unreachable: the release is not taken from github.com instead (a LAN mirror, or the source file forbids it)"
            );
        }
        allowed
    }
}

/// `url` is plain http, or its host is a private, loopback or link-local address or a name only a LAN resolves.
pub fn mirror_local(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return true };
    let host = rest.split('/').next().unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(h, _)| h).to_ascii_lowercase();
    if !host.contains('.') {
        return true;
    }
    let lan = [".localhost", ".lan", ".local", ".home", ".home.arpa", ".internal", ".localdomain"];
    if host == "localhost" || lan.iter().any(|s| host.ends_with(s)) {
        return true;
    }
    let p: Vec<u32> = host.split('.').map_while(|o| o.parse().ok()).collect();
    if p.len() == 4 {
        return matches!((p[0], p[1]), (10 | 127, _) | (192, 168) | (169, 254))
            || (p[0] == 172 && (16..=31).contains(&p[1]));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mirrors() {
        for u in [
            "http://mirror.example.com/x",
            "https://nas/x",
            "https://box.lan/dl",
            "https://10.0.0.2:8000/d",
            "https://172.20.1.1/d",
            "https://192.168.1.2/d",
        ] {
            assert!(mirror_local(u), "{u}");
        }
        for u in ["https://mirror.example.com/dl", "https://172.32.0.1/d", "https://8.8.8.8/d"] {
            assert!(!mirror_local(u), "{u}");
        }
    }

    #[test]
    fn fallback_policy() {
        let cfg = UpdateConfig::default();
        let mut s = Source::load(&cfg, Path::new("/nonexistent/update-source"));
        assert!(!s.fall_back(), "GitHub has no fallback");
        s.mirror = true;
        s.download = "http://192.168.1.2:8000/download".into();
        assert!(!s.fall_back(), "a LAN mirror stays");
        s.fallback = Fallback::Always;
        assert!(s.fall_back());
        assert_eq!(s.file("v2026.11.01.1", "SHA256SUMS"), format!("{}/v2026.11.01.1/SHA256SUMS", cfg.download_url));
        assert!(!s.fall_back(), "once a run");
    }
}
