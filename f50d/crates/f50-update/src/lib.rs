//! Updates for the ZTE F50 on OpenWrt (`f50.update`): the release check, the download, the signature, applying a
//! release and going back. It replaces `mu300-update` (for the F50's OpenWrt systems only) and the LuCI page's
//! adapter `unisoc-modem/update`.
//!
//! Module map:
//! - [`config`]: [`UpdateConfig`] (defaults for the F50 on OpenWrt) and the [`config::Layout`] of every path;
//! - [`release`]: the release's file names (`assets`, the one place that knows them), the list of releases
//!   (`index`), the source (`source`: GitHub or a mirror), the checksums and their signature (`verify`, `sig`);
//! - [`download`]: a release's files, verified, into the staging directory ([`stage`]); [`offline`]: the same from an
//!   uploaded bundle, and [`inspect`]: what an upload is before it is used;
//! - [`apply`]: the update itself - [`apply::rootfs`] (unpack, swap, rollback), [`carry`] (what of the user's an
//!   update keeps), [`apply::kernel`], [`apply::boot`], [`apply::modules`], [`apply::slot`] (the boot image), and
//!   [`apply::journal`] (the commit steps a restart finishes);
//! - [`lock`]: one update at a time across processes; [`notice`]: the periodic check; [`status`]; [`object`]: the
//!   ubus methods.

mod apply;
pub mod carry;
pub mod config;
pub mod download;
mod inspect;
pub mod lock;
mod notice;
mod object;
pub mod offline;
pub mod release;
pub mod stage;
mod status;
mod sys;
pub mod tag;
mod util;

use std::sync::Arc;

use f50_core::Result;
use f50_core::jobs::Jobs;
use f50_core::rpc::Object;
use f50_platform::Platform;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use config::UpdateConfig;

use config::Layout;
use download::Trust;
use release::source::Source;

/// The update service. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Update {
    inner: Arc<Inner>,
}

#[derive(Debug)]
pub(crate) struct Inner {
    pub cfg: UpdateConfig,
    pub layout: Layout,
    pub platform: Platform,
    pub jobs: Jobs,
    pub tracker: TaskTracker,
    pub shutdown: CancellationToken,
}

impl Inner {
    /// The systems on the Linux partition.
    pub fn installed(&self) -> Vec<String> {
        self.cfg.systems.iter().filter(|s| self.layout.system(s).is_dir()).cloned().collect()
    }

    pub fn source(&self) -> Source {
        Source::load(&self.cfg, &self.layout.sys(&self.cfg.source_file))
    }

    pub fn trust(&self, allow_unsigned: bool) -> Trust {
        Trust {
            key_file: self.layout.sys(&self.cfg.release_key),
            usign: self.layout.sys("/usr/bin/usign"),
            allow_unsigned,
        }
    }
}

impl Update {
    /// The service: what a run cut off by a restart left half-way is finished or undone (under the update lock, in
    /// the background), and the periodic check for a newer release starts. Both are tasks of `tracker` and stop at
    /// `shutdown`.
    pub fn start(
        cfg: UpdateConfig,
        platform: Platform,
        jobs: Jobs,
        tracker: &TaskTracker,
        shutdown: CancellationToken,
    ) -> Result<Update> {
        let layout = Layout::new(platform.paths());
        let inner = Arc::new(Inner { cfg, layout, platform, jobs, tracker: tracker.clone(), shutdown });
        let i2 = inner.clone();
        tracker.spawn(async move {
            if i2.shutdown.is_cancelled() {
                return;
            }
            match lock::take(&i2.layout, &i2.cfg.legacy_locks) {
                Ok(_lock) => apply::recover_now(&i2).await,
                Err(e) => tracing::info!(error = %e, "update recovery left to the next run"),
            }
        });
        notice::spawn(inner.clone());
        Ok(Update { inner })
    }

    /// Where an offline update is uploaded through uhttpd's cgi-upload (the one file path the ACL lets a login
    /// write).
    pub fn upload_path(&self) -> std::path::PathBuf {
        self.inner.layout.upload.strip_prefix(self.inner.layout.paths.root()).map_or_else(
            |_| self.inner.layout.upload.clone(),
            |p| std::path::Path::new("/").join(p),
        )
    }

    /// `f50.update`.
    pub fn objects(&self) -> Vec<Object> {
        vec![object::object(self.inner.clone())]
    }
}
