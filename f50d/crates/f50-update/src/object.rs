//! The `f50.update` object. Reading is in the read grant; everything that changes the device is in the write grant
//! and wants the word of its operation (`confirm`), so a stray call does nothing (FINDINGS 86). Long work is a job of
//! kind `update` (one at a time; the update lock decides across processes); `apply`, `apply_file` and `rollback` are
//! not cancellable, since they end by writing the systems and the boot image.

use std::sync::Arc;
use std::time::Duration;

use f50_core::jobs::JobSpec;
use f50_core::rpc::{Access, Field, Object, Params, Rule};
use f50_core::{Error, Result, fsx};
use serde_json::{Value, json};

use crate::Inner;
use crate::apply::{self, Request, Say};
use crate::release::index::Checked;
use crate::tag::TAG_RE;
use crate::util::{blocking, sha256_file};
use crate::{inspect, notice, offline, status};

const SHA_RE: &str = "[0-9a-f]{64}";
const KIND: &str = "update";

const APPLY: &[Field] =
    &[Field::opt("tag", Rule::Word { re: TAG_RE, max: 16 }), Field::req("confirm", Rule::Enum(&["apply"]))];
const ROLLBACK: &[Field] =
    &[Field::req("confirm", Rule::Enum(&["rollback"])), Field::opt("what", Rule::Enum(&["system", "boot"]))];
const UPLOAD: &[Field] = &[
    Field::req("size", Rule::Int { min: 1, max: 2047 << 20 }),
    Field::req("sha256", Rule::Word { re: SHA_RE, max: 64 }),
];
const APPLY_FILE: &[Field] = &[
    Field::req("sha256", Rule::Word { re: SHA_RE, max: 64 }),
    Field::req("confirm", Rule::Enum(&["apply"])),
    Field::opt("unsigned", Rule::Bool),
    Field::opt("downgrade", Rule::Bool),
];

fn idle(i: &Inner) -> Result<()> {
    match i.jobs.running(KIND) {
        Some(id) => Err(Error::busy(format!("an update task is already running ({id})"))),
        None => Ok(()),
    }
}

fn releases(i: &Inner) -> Value {
    let c: Option<Checked> = std::fs::read(&i.layout.releases).ok().and_then(|b| serde_json::from_slice(&b).ok());
    match c {
        Some(c) => json!({ "age": f50_core::time::unix_now().saturating_sub(c.checked), "check": c }),
        None => json!({ "age": null, "check": null }),
    }
}

/// The announced upload (size, SHA-256), kept between `upload_begin` and `upload_done`.
fn announced(i: &Inner) -> Option<(u64, String)> {
    let v: Value = serde_json::from_slice(&std::fs::read(i.layout.upload_dir.join("announced.json")).ok()?).ok()?;
    Some((v["size"].as_u64()?, v["sha256"].as_str()?.to_owned()))
}

fn upload_begin(i: &Inner, p: &Params) -> Result<Value> {
    idle(i)?;
    let size = u64::try_from(p.need_int("size")?).map_err(|_| Error::bad_value("size"))?;
    let sha = p.need_str("sha256")?.to_owned();
    if size > i.cfg.upload_max {
        return Err(Error::field(f50_core::Code::BadValue, "size", "the file is too large"));
    }
    offline::upload_dir(&i.layout)?;
    offline::forget(&i.layout);
    // cgi-io keeps an upload in /tmp until it is complete: the room there, and on the partition beside an update
    if crate::sys::free_kb(&i.layout.sys(&i.cfg.upload_tmp)) <= size / 1024 + 16 * 1024 {
        return Err(Error::failed("not enough memory for this upload"));
    }
    if crate::sys::free_kb(&i.layout.disk) <= size / 1024 + 2_000_000 {
        return Err(Error::failed("not enough free space on the Linux partition"));
    }
    fsx::write_atomic(
        &i.layout.upload_dir.join("announced.json"),
        json!({ "size": size, "sha256": sha }).to_string().as_bytes(),
        0o600,
    )?;
    Ok(json!({ "path": i.layout.upload.display().to_string() }))
}

/// `apply_file`'s gate: the file the page was shown, fit for this device, each thing wrong with it said yes to.
fn file_ok(i: &Inner, p: &Params) -> Result<(String, bool, bool)> {
    let sha = p.need_str("sha256")?.to_owned();
    let (unsigned, downgrade) = (p.flag("unsigned"), p.flag("downgrade"));
    let r = inspect::kept(i)
        .filter(|r| r.sha256 == sha)
        .ok_or_else(|| Error::failed("the file was not checked; upload it again"))?;
    if let Some(why) = &r.refused {
        return Err(Error::failed(why));
    }
    if !r.fits {
        return Err(Error::failed("this file is not for this device"));
    }
    if !r.whole {
        return Err(Error::failed("a file in the bundle does not match its checksum"));
    }
    use crate::release::sig::SigState;
    match r.signature {
        SigState::Valid => {}
        SigState::Bad | SigState::BadKey => return Err(Error::failed("the signature of this file is wrong")),
        _ if unsigned => {}
        _ => {
            return Err(Error::field(
                f50_core::Code::Missing,
                "unsigned",
                "this file is not signed: that needs confirmation",
            ));
        }
    }
    if r.downgrade && !downgrade {
        return Err(Error::field(
            f50_core::Code::Missing,
            "downgrade",
            "this file is older than what is installed: that needs confirmation",
        ));
    }
    Ok((sha, unsigned, downgrade))
}

pub fn object(inner: Arc<Inner>) -> Object {
    let (a, b, c, d, e, f, g, h, k) = (
        inner.clone(),
        inner.clone(),
        inner.clone(),
        inner.clone(),
        inner.clone(),
        inner.clone(),
        inner.clone(),
        inner.clone(),
        inner,
    );
    Object::builder("f50.update")
        .method(
            "status",
            Access::Read,
            &[],
            "What is installed, what is kept for a rollback, free space, the lock, the running job, the notice.",
            move |_p: Params| {
                let i = a.clone();
                async move { blocking(move || Ok(status::status(&i))).await }
            },
        )
        .method("releases", Access::Read, &[], "The releases the last check found, and its age.", move |_p: Params| {
            let r = releases(&b);
            async move { Ok(r) }
        })
        .method("check", Access::Write, &[], "Asks for the newest release (a job).", move |_p: Params| {
            let i = c.clone();
            let r = i
                .jobs
                .clone()
                .start(JobSpec::new("f50.update check", KIND, Duration::from_secs(180)), move |_ctx| async move {
                    notice::check(&i).await
                });
            async move { r.map(|id| json!({ "job": id })) }
        })
        .method(
            "apply",
            Access::Write,
            APPLY,
            "Installs the release the last check offered (a job; not cancellable).",
            move |p: Params| {
                let i = d.clone();
                let req = Request {
                    tag: p.str("tag").map(str::to_owned),
                    offline: None,
                    allow_unsigned: false,
                    allow_downgrade: false,
                };
                let r = idle(&i).and_then(|()| {
                    let spec = JobSpec::new("f50.update apply", KIND, Duration::from_secs(3 * 3600)).not_cancellable();
                    i.jobs.clone().start(spec, move |ctx| async move { apply::apply(i, req, Say(Some(ctx))).await })
                });
                async move { r.map(|id| json!({ "job": id })) }
            },
        )
        .method(
            "rollback",
            Access::Write,
            ROLLBACK,
            "Goes back to the kept system, or (what=boot) boot image (a job).",
            move |p: Params| {
                let i = e.clone();
                let what = p.str("what").unwrap_or("system").to_owned();
                let r = idle(&i).and_then(|()| {
                    let spec = JobSpec::new("f50.update rollback", KIND, Duration::from_secs(1800)).not_cancellable();
                    i.jobs
                        .clone()
                        .start(spec, move |ctx| async move { apply::rollback(i, &what, Say(Some(ctx))).await })
                });
                async move { r.map(|id| json!({ "job": id })) }
            },
        )
        .method(
            "upload_begin",
            Access::Write,
            UPLOAD,
            "Makes room for an offline bundle of that size and hash; the path to upload it to.",
            move |p: Params| {
                let r = upload_begin(&f, &p);
                async move { r }
            },
        )
        .method(
            "upload_done",
            Access::Write,
            &[],
            "Takes the upload and looks at it (a job; its result is what the file is).",
            move |_p: Params| {
                let i = g.clone();
                let r = idle(&i).and_then(|()| {
                    let (size, sha) =
                        announced(&i).ok_or_else(|| Error::failed("no upload was announced (upload_begin)"))?;
                    i.jobs.clone().start(
                        JobSpec::new("f50.update upload_done", KIND, Duration::from_secs(900)),
                        move |_ctx| async move { inspect::upload_done(i, size, sha).await },
                    )
                });
                async move { r.map(|id| json!({ "job": id })) }
            },
        )
        .method("upload_cancel", Access::Write, &[], "Deletes the upload.", move |_p: Params| {
            let r = idle(&h).map(|()| {
                offline::forget(&h.layout);
                json!({})
            });
            async move { r }
        })
        .method(
            "apply_file",
            Access::Write,
            APPLY_FILE,
            "Installs the uploaded bundle that upload_done showed (a job; not cancellable).",
            move |p: Params| {
                let i = k.clone();
                let r = idle(&i).and_then(|()| file_ok(&i, &p)).and_then(|(sha, unsigned, downgrade)| {
                    let spec =
                        JobSpec::new("f50.update apply_file", KIND, Duration::from_secs(3 * 3600)).not_cancellable();
                    i.jobs.clone().start(spec, move |ctx| async move {
                        let staged = i.layout.staged.clone();
                        let s2 = staged.clone();
                        // the file is taken only if it is still the one that was looked at
                        let now =
                            blocking(move || sha256_file(&s2).map_err(|e| Error::io("hash the upload", &e))).await?;
                        if now != sha {
                            offline::forget(&i.layout);
                            return Err(Error::failed(
                                "the uploaded file changed after it was checked: upload it again",
                            ));
                        }
                        let req = Request {
                            tag: None,
                            offline: Some(staged),
                            allow_unsigned: unsigned,
                            allow_downgrade: downgrade,
                        };
                        let r = apply::apply(i.clone(), req, Say(Some(ctx))).await;
                        // the upload has done its work, however it went
                        offline::forget(&i.layout);
                        r
                    })
                });
                async move { r.map(|id| json!({ "job": id })) }
            },
        )
        .build()
}
