//! `f50.update status`: what is installed, without the network - the release of the running system and of each
//! system on the partition with the copy kept for a rollback, the kernel and the boot image (and the one kept),
//! free space and the room an update wants, who holds the lock, the running job, the notice of a newer release,
//! whether this system has a release key, what recovery did after a restart, and whether the report is unread.

use std::fs;

use f50_core::fsx::read_line;
use serde::Serialize;
use serde_json::{Value, json};

use crate::Inner;
use crate::release::assets;
use crate::release::sig::{self, Key};
use crate::util::{file_len, is_running_root, release_of};
use crate::{apply::journal, apply::slot, inspect, lock, notice};

#[derive(Debug, Serialize)]
struct System {
    name: String,
    release: Option<String>,
    running: bool,
    /// The copy kept for a rollback.
    previous: Option<Value>,
}

pub fn status(inner: &Inner) -> Value {
    let l = &inner.layout;
    let root = l.paths.root();
    let systems: Vec<System> = inner
        .installed()
        .into_iter()
        .map(|os| {
            let old = l.sibling(&os, "old");
            System {
                running: is_running_root(root, &l.system(&os)),
                release: release_of(&l.system(&os)),
                previous: old
                    .is_dir()
                    .then(|| json!({ "release": release_of(&old), "running": is_running_root(root, &old) })),
                name: os,
            }
        })
        .collect();
    // between an update and the reboot / is the kept copy (or what a rollback left)
    let restart_pending = inner
        .cfg
        .systems
        .iter()
        .any(|os| is_running_root(root, &l.sibling(os, "old")) || is_running_root(root, &l.sibling(os, "broken")));
    let (free, total) = crate::sys::space(&l.disk).map_or((None, None), |(f, t)| (Some(f / 1024), Some(t / 1024)));
    let n = systems.len() as u64;
    let prev_boot = (file_len(&l.boot_file("prev.body")) > 0).then(|| {
        let ps = read_line(&l.boot_file("prev.slot")).unwrap_or_else(|| "b".into());
        json!({
            "release": read_line(&l.boot_file("prev.tag")),
            "kernel": read_line(&l.boot_file("prev.kernel")),
            "usable": ps == slot::linux_slot(l).to_string(),
        })
    });
    let key = match sig::key(&l.sys(&inner.cfg.release_key)) {
        Key::None => "none",
        Key::Unsafe => "unsafe",
        Key::Ok(_) => "ok",
    };
    let recovery: Option<Value> = fs::read(&l.recovery).ok().and_then(|b| serde_json::from_slice(&b).ok());
    json!({
        "installed": notice::installed(inner),
        "restart_pending": restart_pending,
        "systems": systems,
        "kernel": {
            "running": read_line(&l.sys("/proc/sys/kernel/osrelease")),
            "choice": l.kernel_choice(),
            "choices": crate::config::KERNELS,
        },
        "boot": {
            "release": read_line(&l.boot_file("installed.tag")),
            "kernel": read_line(&l.boot_file("installed.kernel")),
            "slot": slot::linux_slot(l).to_string(),
            "previous": prev_boot,
            "default": read_line(&l.sys("/etc/f50/default-boot")).filter(|d| d == "linux").unwrap_or_else(|| "android".into()),
        },
        "disk": { "free_kb": free, "total_kb": total, "need_one_kb": 2_000_000u64, "need_all_kb": 1_000_000 + 1_000_000 * n },
        "lock": lock::holder(l),
        "job": inner.jobs.running("update"),
        "unfinished": journal::read(l).is_some(),
        "recovery": recovery,
        "notice": notice::kept(inner),
        "signing": { "key": key },
        "source": inner.source(),
        "report_new": l.sys(format!("/{}.new", assets::REPORT)).exists(),
        "upload": {
            "present": crate::offline::upload_file(&l.staged) || crate::offline::upload_file(&l.upload),
            "inspect": inspect::kept(inner),
        },
    })
}
