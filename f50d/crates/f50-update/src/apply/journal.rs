//! The record of the step that must finish once it began, even if f50d dies in the middle of it (a job does not
//! survive the daemon: `f50_core::jobs` reports it `interrupted`, nothing more). It is on the Linux partition, beside
//! the systems, written atomically and synced before the step starts and removed after it ended; `recover` reads it
//! at the next start (see `apply::recover` for what each step does then).

use std::path::PathBuf;

use f50_core::{Result, fsx};
use serde::{Deserialize, Serialize};

use crate::config::Layout;
use crate::util::sync_dir;

/// The step in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum Step {
    /// `<os>` is being put aside as `<os>.old` and `<os>.new` put in its place.
    Switch { os: String, tag: String },
    /// The new boot image is being written to `dev`; its parts are in the staging directory (`boot/new`,
    /// `boot/vbmeta`, `boot/newfooter`), the previous image in `boot/prev.*`.
    BootWrite {
        tag: String,
        dev: PathBuf,
        psize: u64,
        /// sha256 of body and vbmeta as written, and of the footer.
        image_sha: String,
        footer_sha: String,
        /// sha256 of the new body alone (`written.sha256`), of kernel and generic ramdisk (`installed.id`).
        body_sha: String,
        id: String,
        kernel: String,
    },
}

pub fn write(l: &Layout, step: &Step) -> Result<()> {
    let data = serde_json::to_vec(step).map_err(|e| f50_core::Error::failed(format!("journal: {e}")))?;
    fsx::write_atomic(&l.journal, &data, 0o600)?;
    if let Some(d) = l.journal.parent() {
        sync_dir(d);
    }
    Ok(())
}

pub fn read(l: &Layout) -> Option<Step> {
    let b = std::fs::read(&l.journal).ok()?;
    serde_json::from_slice(&b).ok()
}

pub fn clear(l: &Layout) {
    let _ = std::fs::remove_file(&l.journal);
    if let Some(d) = l.journal.parent() {
        sync_dir(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let root = std::env::temp_dir().join(format!("f50-update-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let l = Layout::new(&f50_core::paths::Paths::new(&root));
        std::fs::create_dir_all(&l.disk).expect("disk");
        let s = Step::Switch { os: "openwrt".into(), tag: "v2026.11.01.1".into() };
        write(&l, &s).expect("write");
        assert_eq!(read(&l), Some(s));
        clear(&l);
        assert_eq!(read(&l), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
