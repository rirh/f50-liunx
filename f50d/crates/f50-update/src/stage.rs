//! The staging directory of a run (`.f50-update` on the Linux partition): root's own and closed to everyone else
//! before anything is put into it or checked in it, since what is verified there is what is unpacked minutes later.
//! One found writable by others is emptied first: a name in it could be a link someone left (FINDINGS 86).

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use f50_core::{Result, fsx};

use crate::config::Layout;
use crate::util::remove_tree;

/// The staging directory, made safe; its path.
pub fn prepare(layout: &Layout) -> Result<PathBuf> {
    if let Ok(m) = fs::symlink_metadata(&layout.stage)
        && m.is_dir()
        && m.mode() & 0o022 != 0
    {
        let _ = remove_tree(&layout.stage);
    }
    fsx::private_dir(&layout.stage).map_err(|e| e.context("the staging directory"))?;
    Ok(layout.stage.clone())
}

/// What a run staged goes (when it ended well, or by `clean`).
pub fn clear(layout: &Layout) {
    if let Err(e) = remove_tree(&layout.stage) {
        tracing::warn!(error = %e, "could not remove the staging directory");
    }
}

/// The path of a release file in the staging directory.
pub fn file(layout: &Layout, name: &str) -> PathBuf {
    layout.stage.join(name)
}
