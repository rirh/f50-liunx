//! The daemon: configuration, construction, lifecycle and shutdown. The composition root.
//!
//! Order matters and follows what each part needs: ubus first (the platform, netifd and the server need it), then
//! the jobs, the AT channels (on the card the boot pinned), the modem, then everything that uses the modem, then the
//! API assembled from all of them. Shutdown (SIGTERM from procd) cancels one token; every task belongs to one
//! tracker, which is drained with a deadline so a stuck task cannot hold procd's stop for ever.

use std::process::ExitCode;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use f50_api::{Api, Services};
use f50_at::{At, AtConfig};
use f50_core::jobs::Jobs;
use f50_core::paths::Paths;
use f50_core::{Error, Result, fsx};
use f50_platform::Platform;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::{bus, config, internal, log, wiring};

const UBUS_WAIT: Duration = Duration::from_secs(60);
const DRAIN: Duration = Duration::from_secs(20);
const ACL_FILE: &str = "/usr/share/rpcd/acl.d/f50.json";

pub fn run() -> ExitCode {
    let rt =
        match tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("f50d").enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                log::init(None);
                tracing::error!(error = %e, "no runtime");
                return ExitCode::FAILURE;
            }
        };
    match rt.block_on(daemon()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "f50d stopped");
            ExitCode::FAILURE
        }
    }
}

async fn daemon() -> Result<()> {
    let paths = Paths::device();
    let cfg = config::load(&Platform::new(paths.clone(), None)).await;
    log::init(cfg.log.as_deref());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");

    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();
    watch_signals(&tracker, shutdown.clone())?;

    let ubus = bus::connect(UBUS_WAIT, &shutdown).await.map_err(|e| Error::failed(format!("ubus: {e}")))?;
    let platform = Platform::new(paths.clone(), Some(ubus));
    fsx::private_dir(&paths.run())?;
    let jobs = Jobs::new(paths.run(), tracker.clone(), shutdown.clone());

    let slot = f50_modem::boot_slot(&paths);
    let at = At::start(AtConfig::for_slot(slot.index()), &tracker, shutdown.clone()).await?;
    let modem = f50_modem::Modem::start(
        f50_modem::ModemConfig::default(),
        at.clone(),
        platform.clone(),
        jobs.clone(),
        &tracker,
        shutdown.clone(),
    )?;
    let data = f50_data::Data::start(
        f50_data::DataConfig::default(),
        at.clone(),
        modem.clone(),
        platform.clone(),
        jobs.clone(),
        &tracker,
        shutdown.clone(),
    )?;
    let sms = f50_sms::Sms::start(
        f50_sms::SmsConfig::default(),
        at.clone(),
        jobs.clone(),
        paths.clone(),
        &tracker,
        shutdown.clone(),
    )?;
    let net =
        f50_net::Net::start(f50_net::NetConfig::default(), platform.clone(), jobs.clone(), &tracker, shutdown.clone())?;
    let system =
        f50_system::System::start(f50_system::SystemConfig::default(), platform.clone(), &tracker, shutdown.clone())?;
    let hw = f50_hw::Hw::new(f50_hw::HwConfig::default(), platform.clone());
    let update = f50_update::Update::start(
        f50_update::UpdateConfig::default(),
        platform.clone(),
        jobs.clone(),
        &tracker,
        shutdown.clone(),
    )?;

    let services = Services { at, jobs, paths, modem, data, sms, net, system, hw, update };
    let acl = Arc::new(OnceLock::new());
    let internal =
        vec![services.data.proto_object(), internal::object(services.net.clone(), services.data.clone(), acl.clone())];
    let api = Api::new(&services, internal);
    let _ = acl.set(api.acl());
    install_acl(&platform, &api).await;

    wiring::start(&services, &platform, &cfg, &tracker, &shutdown);
    tracker.spawn(bus::serve(api, shutdown.clone()));
    tracing::info!("ready");

    shutdown.cancelled().await;
    tracing::info!("stopping");
    services.net.adbd_stop();
    tracker.close();
    if tokio::time::timeout(DRAIN, tracker.wait()).await.is_err() {
        tracing::warn!("some tasks did not stop in time");
    }
    Ok(())
}

fn watch_signals(tracker: &TaskTracker, shutdown: CancellationToken) -> Result<()> {
    let mut term = signal(SignalKind::terminate()).map_err(|e| Error::io("SIGTERM handler", &e))?;
    let mut int = signal(SignalKind::interrupt()).map_err(|e| Error::io("SIGINT handler", &e))?;
    let s = shutdown.clone();
    tracker.spawn(async move {
        tokio::select! {
            _ = term.recv() => s.cancel(),
            _ = int.recv() => s.cancel(),
            () = s.cancelled() => {}
        }
    });
    Ok(())
}

/// The ACL generated from the method table, written when it changed; rpcd then reloads its ACLs.
async fn install_acl(platform: &Platform, api: &Api) {
    let path = platform.paths().sys(ACL_FILE);
    let Ok(text) = serde_json::to_string_pretty(&api.acl()) else { return };
    if fsx::read_opt(&path).ok().flatten().as_deref() == Some(text.as_str()) {
        return;
    }
    if let Err(e) = fsx::write_atomic(&path, text.as_bytes(), 0o644) {
        tracing::warn!(error = %e, "could not write the rpcd ACL");
        return;
    }
    let r = platform.services().run("rpcd", f50_platform::services::Action::Reload).await;
    if let Err(e) = r {
        tracing::warn!(error = %e, "rpcd did not reload its ACLs");
    }
}
