//! What an uploaded bundle is, before anything is done with it (the shell's `inspect`): its SHA-256, the release it
//! says it is, whether its files match their checksums, whether it is for this device, how its signature checks, and
//! whether it is older than what is installed. Changes nothing; the answer is kept for `apply_file`, which takes the
//! file only if it still has the hash that was shown.

use std::fs;
use std::sync::Arc;

use f50_core::{Error, Result, fsx};
use serde::{Deserialize, Serialize};

use crate::Inner;
use crate::offline::{self, Bundle};
use crate::release::assets;
use crate::release::sig::{self, Key, SigState};
use crate::release::verify::{self, Origin, Sums};
use crate::tag::{newer, tag_ok};
use crate::util::{blocking, release_of, sha256_file};

/// The answer, as `upload_done` gives it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspected {
    pub sha256: String,
    pub size: u64,
    pub tag: Option<String>,
    pub signature: SigState,
    /// Every file of it matches its checksum.
    pub whole: bool,
    /// It holds the root filesystem of an installed system.
    pub fits: bool,
    pub systems: Vec<String>,
    pub kernel: bool,
    pub downgrade: bool,
    /// Why it cannot be used at all, when it cannot.
    pub refused: Option<String>,
}

/// Takes the upload (renamed out of the panel's reach) and looks at it.
pub async fn upload_done(inner: Arc<Inner>, size: u64, sha: String) -> Result<Inspected> {
    let l = inner.layout.clone();
    offline::upload_dir(&l)?;
    let staged = l.staged.clone();
    if l.upload.exists() || fs::symlink_metadata(&l.upload).is_ok() {
        if !offline::upload_file(&l.upload) {
            let _ = fs::remove_file(&l.upload);
            return Err(Error::not_found("uploaded file"));
        }
        let _ = fs::remove_file(&l.inspect);
        fs::rename(&l.upload, &staged).map_err(|e| Error::io("take the upload", &e))?;
    }
    if !offline::upload_file(&staged) {
        return Err(Error::not_found("uploaded file"));
    }
    let p = staged.clone();
    let got = blocking(move || sha256_file(&p).map_err(|e| Error::io("hash the upload", &e))).await?;
    let len = crate::util::file_len(&staged);
    if got != sha || len != size {
        offline::forget(&l);
        return Err(Error::failed("the uploaded file is not the one announced (size or SHA-256): upload it again"));
    }
    let r = look(&inner, got, len).await;
    fsx::write_atomic(&l.inspect, &serde_json::to_vec(&r).unwrap_or_default(), 0o600)?;
    Ok(r)
}

async fn look(inner: &Arc<Inner>, sha256: String, size: u64) -> Inspected {
    let mut r = Inspected {
        sha256,
        size,
        tag: None,
        signature: SigState::Unsigned,
        whole: false,
        fits: false,
        systems: Vec::new(),
        kernel: false,
        downgrade: false,
        refused: None,
    };
    let (allowed, path) = (assets::offline_names(&inner.cfg.systems), inner.layout.staged.clone());
    let b = match blocking(move || Bundle::open(&path, &allowed)).await {
        Ok(b) => b,
        Err(e) => {
            r.refused = Some(e.message().to_owned());
            return r;
        }
    };
    let b2 = b.clone();
    let parts = blocking(move || {
        Ok((
            b2.read(assets::SUMS, 256 * 1024)?,
            b2.read(assets::RELEASE_TAG, 64)?,
            b2.read(assets::SUMS_MINISIG, 4096)?,
            b2.read(assets::SUMS_SIG, 4096)?,
        ))
    })
    .await;
    let Ok((Some(sums_text), Some(tag), minisig, usig)) = parts else {
        r.refused = Some("the bundle has no checksums, or does not say which release it is".into());
        return r;
    };
    let tag = String::from_utf8_lossy(&tag).trim().to_owned();
    if !tag_ok(&tag) {
        r.refused = Some("the bundle does not say which release it is".into());
        return r;
    }
    r.tag = Some(tag.clone());
    let Some(sums) = Sums::parse(&sums_text) else {
        r.refused = Some("the checksums of the bundle are not readable".into());
        return r;
    };
    r.signature = signature(inner, &sums_text, minisig, usig).await;
    if let Err(e) = verify::verdict(&tag, r.signature, &sums, Origin::Offline, true) {
        r.refused = Some(e.message().to_owned());
    }
    // every payload against its checksum
    let names: Vec<String> = b
        .names()
        .into_iter()
        .filter(|n| n != assets::SUMS && n != assets::SUMS_SIG && n != assets::SUMS_MINISIG)
        .collect();
    let (b3, s3, n3) = (b.clone(), sums.clone(), names.clone());
    r.whole = blocking(move || {
        Ok(n3.iter().all(|n| s3.want(n).is_some_and(|w| b3.sha256(n).ok().flatten().as_deref() == Some(w))))
    })
    .await
    .unwrap_or(false);
    let installed = inner.installed();
    r.systems = installed.iter().filter(|s| b.has(&assets::rootfs(s))).cloned().collect();
    r.fits = !r.systems.is_empty();
    r.kernel = b.has(&assets::kernel(inner.layout.kernel_choice()));
    r.downgrade = r.systems.iter().any(|s| release_of(&inner.layout.system(s)).is_some_and(|v| newer(&v, &tag)))
        || fsx::read_line(&inner.layout.boot_file("installed.tag")).is_some_and(|v| newer(&v, &tag));
    r
}

async fn signature(inner: &Inner, sums: &[u8], minisig: Option<Vec<u8>>, usig: Option<Vec<u8>>) -> SigState {
    let key_file = inner.layout.sys(&inner.cfg.release_key);
    let key = match sig::key(&key_file) {
        Key::None => return SigState::NoKey,
        Key::Unsafe => return SigState::BadKey,
        Key::Ok(k) => k,
    };
    if let Some(m) = minisig {
        return sig::check_minisign(&key, &String::from_utf8_lossy(&m), sums);
    }
    let Some(s) = usig else { return SigState::Unsigned };
    // usign takes files: a directory of our own beside the upload
    let dir = inner.layout.upload_dir.join("inspect");
    if fsx::private_dir(&dir).is_err() {
        return SigState::NoTool;
    }
    let (sp, gp) = (dir.join(assets::SUMS), dir.join(assets::SUMS_SIG));
    if fsx::write_atomic(&sp, sums, 0o600).is_err() || fsx::write_atomic(&gp, &s, 0o600).is_err() {
        return SigState::NoTool;
    }
    let st = sig::check_usign(&inner.layout.sys("/usr/bin/usign"), &key_file, &key, &sp, &gp).await;
    let _ = crate::util::remove_tree(&dir);
    st
}

/// The kept answer about the staged upload.
pub fn kept(inner: &Inner) -> Option<Inspected> {
    serde_json::from_slice(&fs::read(&inner.layout.inspect).ok()?).ok()
}
