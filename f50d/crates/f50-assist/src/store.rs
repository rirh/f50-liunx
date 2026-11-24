//! A settings or state file: JSON, 0600, written atomically; one that cannot be read is the defaults.

use std::path::Path;

use f50_core::{Error, Result, fsx};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub(crate) fn load<T: DeserializeOwned + Default>(dir: &Path, name: &str) -> T {
    let path = dir.join(name);
    match fsx::read_opt(&path) {
        Ok(Some(text)) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(file = %path.display(), error = %e, "unreadable; using the defaults");
            T::default()
        }),
        _ => T::default(),
    }
}

pub(crate) async fn save<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<()> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| Error::failed(e.to_string()))?;
    let path = dir.join(name);
    tokio::task::spawn_blocking(move || fsx::write_atomic(&path, &data, 0o600))
        .await
        .map_err(|e| Error::failed(e.to_string()))?
}
