//! The boot stages procd runs (`f50d boot <stage>`), the trial arming the updater asks for, and the Wi-Fi/Bluetooth
//! firmware hotplug. They work without the daemon: they run before it, or beside it as short programs.

use std::process::ExitCode;
use std::str::FromStr;

use f50_core::paths::Paths;
use f50_hw::{ArmOutcome, Hw, HwConfig, Stage, VendorDaemon, WcnEvent};
use f50_platform::Platform;

fn hw() -> Hw {
    Hw::new(HwConfig::default(), Platform::new(Paths::device(), None))
}

fn report(r: &f50_core::Result<f50_hw::StageReport>) -> ExitCode {
    match r {
        Ok(rep) => {
            if let Ok(s) = serde_json::to_string(rep) {
                tracing::info!(report = %s, "boot stage");
            }
            if rep.ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(e) => {
            tracing::error!(error = %e, "boot stage");
            ExitCode::FAILURE
        }
    }
}

/// `f50d boot <early|vendor|hardware|post|trial>` and `f50d boot vendor-exec <daemon>`.
pub async fn run(args: &[String]) -> ExitCode {
    let hw = hw();
    match args {
        [s] if s == "trial" => match hw.arm_trial().await {
            Ok(ArmOutcome::Armed { .. }) => ExitCode::SUCCESS,
            // the shell's --trial exit status for "the default boot is Android": the updater says so and goes on
            Ok(ArmOutcome::NotDefaultLinux) => ExitCode::from(3),
            Err(e) => {
                tracing::error!(error = %e, "trial boot not armed");
                ExitCode::FAILURE
            }
        },
        [s] if s == "vendor" => {
            // prepare the chroot, then become modem_control (procd watches this very process)
            let r = hw.stage(Stage::Vendor).await;
            let _ = report(&r);
            let e = hw.exec_vendor(VendorDaemon::ModemControl).await;
            tracing::error!(error = %e, "modem_control did not start");
            ExitCode::FAILURE
        }
        [v, d] if v == "vendor-exec" => match VendorDaemon::from_str(d) {
            Ok(d) => {
                let e = hw.exec_vendor(d).await;
                tracing::error!(error = %e, "vendor daemon did not start");
                ExitCode::FAILURE
            }
            Err(e) => {
                tracing::error!(error = %e, "unknown vendor daemon");
                ExitCode::from(2)
            }
        },
        [s] => match Stage::from_str(s) {
            Ok(st) => report(&hw.stage(st).await),
            Err(_) => ExitCode::from(2),
        },
        _ => ExitCode::from(2),
    }
}

/// `f50d hotplug wcn`, from hotplug.d/misc with ACTION, SOURCE and EVENT in the environment.
pub async fn hotplug_wcn() -> ExitCode {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let ev = WcnEvent { action: var("ACTION"), source: var("SOURCE"), event: var("EVENT") };
    match hw().hotplug_wcn(&ev).await {
        Ok(o) => {
            tracing::debug!(outcome = ?o, "wcn hotplug");
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::warn!(error = %e, "wcn hotplug");
            ExitCode::FAILURE
        }
    }
}

/// argv[0] `modprobe-guard`: the kernel's module helper (FINDINGS 163), no runtime, as fast as it can be.
pub fn modprobe_guard(args: &[std::ffi::OsString]) -> ExitCode {
    let code = f50_hw::modprobe_helper(&HwConfig::default(), &Paths::device(), args);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
