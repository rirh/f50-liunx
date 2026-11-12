//! HTTP(S) downloads through the image's `uclient-fetch`. URLs are checked by the caller; they reach the program
//! as one argument that never starts with "-".

use std::path::Path;
use std::time::Duration;

use f50_core::Result;

use crate::Platform;

#[derive(Debug, Clone)]
pub struct Fetch {
    platform: Platform,
}

impl Fetch {
    pub(crate) fn new(platform: Platform) -> Self {
        Fetch { platform }
    }

    /// A small text resource (a release index, SHA256SUMS), at most `max` bytes.
    pub async fn text(&self, url: &str, timeout: Duration, max: usize) -> Result<String> {
        let _ = (&self.platform, url, timeout, max);
        todo!("f50-platform: Fetch::text")
    }

    /// A file to `dest` (resumed when a partial one is there). Cancel by dropping the future.
    pub async fn file(&self, url: &str, dest: &Path, timeout: Duration) -> Result<()> {
        let _ = (url, dest, timeout);
        todo!("f50-platform: Fetch::file")
    }
}
