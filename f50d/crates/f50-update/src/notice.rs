//! Checking for releases: on request (`check`, a job) and in the background (every `notice_interval`, the first
//! after `notice_delay`). Each check keeps its answer for `releases` and for the tag `apply` may take, and writes
//! the notice `status` reports when a newer release is out. Nothing is ever installed by itself: an update that went
//! wrong on a device nobody is watching could leave it unreachable.

use std::sync::Arc;

use f50_core::{Result, fsx};
use serde::{Deserialize, Serialize};

use crate::Inner;
use crate::release::index::{self, Checked};
use crate::tag::{newer, tag_ok};
use crate::util::release_of;

/// The note of a newer release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notice {
    pub tag: String,
    pub installed: Option<String>,
    pub at: u64,
}

/// The release the running system is.
pub fn installed(inner: &Inner) -> Option<String> {
    release_of(&inner.layout.sys("/"))
}

/// Asks for the releases, keeps the answer and the notice.
pub async fn check(inner: &Arc<Inner>) -> Result<Checked> {
    let (latest, all) = index::fetch(&inner.platform.fetch(), &inner.source()).await?;
    let choice = inner.layout.kernel_choice();
    let outdated = |tag: &str| -> Vec<String> {
        inner.installed().into_iter().filter(|s| release_of(&inner.layout.system(s)).as_deref() != Some(tag)).collect()
    };
    let beta = index::beta(&latest, &all).map(|b| index::offer(b, &outdated(&b.tag), choice));
    let checked = Checked {
        checked: f50_core::time::unix_now(),
        latest: index::offer(&latest, &outdated(&latest.tag), choice),
        beta,
    };
    fsx::private_dir(&inner.layout.lock_dir)?;
    let run = inner.layout.releases.parent().map(std::path::Path::to_path_buf);
    if let Some(r) = run {
        fsx::private_dir(&r)?;
    }
    fsx::write_atomic(&inner.layout.releases, &serde_json::to_vec(&checked).unwrap_or_default(), 0o600)?;
    let inst = installed(inner);
    let note = match &inst {
        Some(i) => newer(&checked.latest.tag, i),
        None => tag_ok(&checked.latest.tag),
    };
    if note {
        let n = Notice { tag: checked.latest.tag.clone(), installed: inst, at: checked.checked };
        fsx::write_atomic(&inner.layout.notice, &serde_json::to_vec(&n).unwrap_or_default(), 0o600)?;
    } else {
        let _ = std::fs::remove_file(&inner.layout.notice);
    }
    Ok(checked)
}

/// The kept notice.
pub fn kept(inner: &Inner) -> Option<Notice> {
    let n: Notice = serde_json::from_slice(&std::fs::read(&inner.layout.notice).ok()?).ok()?;
    tag_ok(&n.tag).then_some(n)
}

/// The background check, until shutdown.
pub fn spawn(inner: Arc<Inner>) {
    if inner.cfg.notice_interval.is_zero() {
        return;
    }
    let token = inner.shutdown.clone();
    inner.tracker.clone().spawn(async move {
        let mut wait = inner.cfg.notice_delay;
        loop {
            tokio::select! {
                () = token.cancelled() => return,
                () = tokio::time::sleep(wait) => {}
            }
            wait = inner.cfg.notice_interval;
            // offline: the note stays as it was, the next round asks again
            let r = tokio::select! {
                () = token.cancelled() => return,
                r = check(&inner) => r,
            };
            if let Err(e) = r {
                tracing::debug!(error = %e, "release check");
            }
        }
    });
}
