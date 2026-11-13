//! The list of releases: GitHub's API (`releases/latest`, `releases?per_page=10`) read into [`Release`]s, and what
//! this device would download of one ([`Offer`]). The newest release is offered, and the newest pre-release only when
//! it is newer than that (the page asks for it behind a tick: FINDINGS 86).

use std::time::Duration;

use f50_core::sanitize::{sanitize_message, truncate_bytes};
use f50_core::{Error, Result};
use f50_platform::fetch::Fetch;
use serde::{Deserialize, Serialize};

use super::assets;
use super::source::Source;
use crate::tag::{newer, tag_ok};

const INDEX_MAX: usize = 2 << 20;
const INDEX_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
}

/// One file of a release and its size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
}

/// A published release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub title: Option<String>,
    /// The day it was published (`YYYY-MM-DD`).
    pub published: Option<String>,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

impl Release {
    fn from_gh(r: GhRelease) -> Option<Release> {
        if r.draft || !tag_ok(&r.tag_name) {
            return None;
        }
        let assets =
            r.assets.into_iter().filter(|a| assets::name_ok(&a.name)).map(|a| Asset { name: a.name, size: a.size });
        Some(Release {
            tag: r.tag_name,
            title: r.name.map(|t| sanitize_message(truncate_bytes(&t, 120))).filter(|t| !t.is_empty()),
            published: r.published_at.map(|p| truncate_bytes(&p, 10).to_owned()).filter(|p| p.len() == 10),
            prerelease: r.prerelease,
            assets: assets.collect(),
        })
    }

    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }

    /// Signed: it has a signature of its checksums.
    pub fn signed(&self) -> bool {
        self.asset(assets::SUMS_MINISIG).is_some() || self.asset(assets::SUMS_SIG).is_some()
    }
}

/// What applying a release would take on this device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub tag: String,
    pub title: Option<String>,
    pub published: Option<String>,
    pub prerelease: bool,
    pub signed: bool,
    /// The systems that are not that release yet.
    pub systems: Vec<String>,
    /// The files that would be downloaded, with their sizes.
    pub files: Vec<Asset>,
    pub download: u64,
}

/// The answer of a check, kept for `releases` and for the tag `apply` may take.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checked {
    /// When the check ran (Unix seconds).
    pub checked: u64,
    pub latest: Offer,
    pub beta: Option<Offer>,
}

impl Checked {
    /// The offer of `tag`, when the check offered it.
    pub fn offered(&self, tag: &str) -> Option<&Offer> {
        std::iter::once(&self.latest).chain(self.beta.as_ref()).find(|o| o.tag == tag)
    }
}

/// Parses `releases/latest`.
pub fn parse_latest(json: &str) -> Result<Release> {
    let r: GhRelease = serde_json::from_str(json).map_err(|_| Error::failed("the release list is not readable"))?;
    Release::from_gh(r).ok_or_else(|| Error::failed("the newest release has no release tag"))
}

/// Parses `releases` (drafts and what has no release tag left out).
pub fn parse_list(json: &str) -> Result<Vec<Release>> {
    let v: Vec<GhRelease> =
        serde_json::from_str(json).map_err(|_| Error::failed("the release list is not readable"))?;
    Ok(v.into_iter().filter_map(Release::from_gh).collect())
}

/// The newest pre-release of `all` when it is newer than `latest`.
pub fn beta<'a>(latest: &Release, all: &'a [Release]) -> Option<&'a Release> {
    let best = all.iter().filter(|r| r.prerelease).fold(None::<&Release>, |b, r| match b {
        Some(b) if !newer(&r.tag, &b.tag) => Some(b),
        _ => Some(r),
    })?;
    newer(&best.tag, &latest.tag).then_some(best)
}

/// What this device would download of `r`: the root filesystem of every system in `outdated`, the kernel bundle of
/// `kernel`, and the checksums with their signature.
pub fn offer(r: &Release, outdated: &[String], kernel: &str) -> Offer {
    let mut want: Vec<String> = vec![assets::SUMS.into(), assets::SUMS_MINISIG.into(), assets::SUMS_SIG.into()];
    want.extend(outdated.iter().map(|s| assets::rootfs(s)));
    want.push(assets::kernel(kernel));
    let files: Vec<Asset> = want.iter().filter_map(|n| r.asset(n).cloned()).collect();
    Offer {
        tag: r.tag.clone(),
        title: r.title.clone(),
        published: r.published.clone(),
        prerelease: r.prerelease,
        signed: r.signed(),
        systems: outdated.to_vec(),
        download: files.iter().map(|f| f.size).sum(),
        files,
    }
}

/// Asks the source for the newest release and the recent ones.
pub async fn fetch(fetch: &Fetch, src: &Source) -> Result<(Release, Vec<Release>)> {
    let latest = fetch.text(&src.api("releases/latest"), INDEX_TIMEOUT, INDEX_MAX).await?;
    let latest = parse_latest(&latest)?;
    // the list only adds a pre-release: without it the newest release still stands
    let all = match fetch.text(&src.api("releases?per_page=10"), INDEX_TIMEOUT, INDEX_MAX).await {
        Ok(s) => parse_list(&s).unwrap_or_default(),
        Err(e) => {
            tracing::info!(error = %e, "the release list could not be read; only the newest release is offered");
            Vec::new()
        }
    };
    Ok((latest, all))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"[
      {"tag_name":"v2026.11.02.1","name":"F50 Linux v2026.11.02.1","prerelease":true,"draft":false,
       "published_at":"2026-11-02T10:00:00Z","body":"notes with \"tag_name\": \"v2099.01.01\"",
       "assets":[{"name":"SHA256SUMS","size":900},{"name":"SHA256SUMS.sig","size":140},
                 {"name":"f50-openwrt-rootfs.tar.gz","size":40000000},{"name":"f50-kernel-6.18.tar.gz","size":52000000},
                 {"name":"../evil","size":1}]},
      {"tag_name":"v2026.11.01.1","name":"stable","prerelease":false,"draft":false,"assets":[]},
      {"tag_name":"v2026.11.03.1","prerelease":true,"draft":true,"assets":[]},
      {"tag_name":"nightly","prerelease":true,"draft":false,"assets":[]}
    ]"#;

    #[test]
    fn list_beta_and_offer() {
        let all = parse_list(LIST).expect("list");
        assert_eq!(all.len(), 2, "the draft and the non-tag are left out");
        assert!(all[0].assets.iter().all(|a| a.name != "../evil"));
        let latest = parse_latest(r#"{"tag_name":"v2026.11.01.1","assets":[]}"#).expect("latest");
        let b = beta(&latest, &all).expect("beta");
        assert_eq!(b.tag, "v2026.11.02.1");
        assert!(b.signed());
        let o = offer(b, &["openwrt".into()], "6.18");
        assert_eq!(o.files.len(), 4);
        assert_eq!(o.download, 900 + 140 + 40_000_000 + 52_000_000);
        assert_eq!(o.published.as_deref(), Some("2026-11-02"));
        // a pre-release older than the newest release is not offered
        let newest = parse_latest(r#"{"tag_name":"v2026.11.05.1","assets":[]}"#).expect("latest");
        assert!(beta(&newest, &all).is_none());
        assert!(parse_latest(r#"{"tag_name":"x"}"#).is_err());
    }
}
