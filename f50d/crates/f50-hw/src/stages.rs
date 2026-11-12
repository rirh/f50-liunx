//! The boot stages other than the vendor one (`vendor.rs`): early (the recorder), hardware, post.

use std::path::PathBuf;

use crate::modules::{guard, order};
use crate::recorder::Recorder;
use crate::report::Steps;
use crate::{Hw, board, slot, wcn};

impl Hw {
    pub(crate) fn guard(&self) -> Option<guard::Guard> {
        let moddir = self.platform().modules().module_dir()?;
        let krel = moddir.file_name()?.to_string_lossy().into_owned();
        Some(guard::Guard { cache: self.platform().paths().run().join("modprobe-guard"), moddir, krel })
    }
}

/// preinit: the recorder, for its window (the preinit hook runs this in the background).
pub(crate) async fn early(hw: &Hw, steps: &mut Steps) {
    let c = hw.config();
    let s = slot::linux_slot(&hw.dev(&c.initramfs_run), &hw.cmdline_sources());
    let rec = Recorder {
        dev: hw.dev("/dev"),
        sys: hw.dev("/sys"),
        proc_uptime: hw.dev("/proc/uptime"),
        cpu_dir: hw.dev("/sys/devices/system/cpu"),
        part: format!("boot_{}", s.letter()),
        offset: c.recorder_offset,
        max: c.recorder_max,
        window: c.recorder_window,
        vendor_log: hw.dev(&c.vendor_log),
        tmp: hw.dev("/tmp"),
    };
    steps.result("recorder", rec.record().await);
}

/// S12: the module helper, the Wi-Fi's rules, board.json, then the vendor drivers.
pub(crate) async fn hardware(hw: &Hw, steps: &mut Steps) {
    let c = hw.config();
    hw.mark("hw-start");

    // the kernel's module helper once its table is made (~2 s; FINDINGS 163)
    match hw.guard() {
        None => steps.failed("modprobe-guard", "no module directory for the running kernel"),
        Some(g) => {
            let g2 = g.clone();
            let lines =
                tokio::task::spawn_blocking(move || g2.fresh_table().map(|t| t.lines().count().saturating_sub(1)))
                    .await
                    .ok()
                    .flatten();
            let allow = c.autoload_allow.iter().map(|a| hw.dev(a)).find(|a| a.exists());
            let helper = hw.dev(&c.modprobe_helper);
            if let Some(a) = allow {
                steps
                    .skipped("modprobe-guard", format!("{} is there: the kernel's helper stays as it is", a.display()));
            } else if !f50_core::exec::executable(&helper) {
                // never a helper that is not there: every on-demand load would fail
                steps.failed(
                    "modprobe-guard",
                    format!("{} is not installed: the kernel's helper stays", helper.display()),
                );
            } else {
                let r = hw
                    .platform()
                    .sysfs()
                    .write("/proc/sys/kernel/modprobe", &format!("{}\n", c.modprobe_helper.display()));
                steps.result(
                    "modprobe-guard",
                    r.map(|()| {
                        format!("the kernel's helper; {} names and aliases not loaded on demand", lines.unwrap_or(0))
                    }),
                );
            }
        }
    }

    // the rules first, then the chip: with the database and the country in place before the phy appears, netifd's own
    // start of hostapd already runs under them
    let fw: Vec<PathBuf> = ["/lib/firmware", "/usr/lib/firmware"].iter().map(|d| hw.dev(d)).collect();
    steps.result("regdb", wcn::load_regdb(&hw.dev("/sys"), &fw).await);
    match board::check(&hw.dev("/etc/board.json"), &hw.dev("/bin/board_detect")).await {
        Ok(true) => {
            steps.done("board-json", "board.json was not valid: made again by board_detect (wireless settings kept)")
        }
        Ok(false) => steps.skipped("board-json", "valid"),
        Err(e) => steps.failed("board-json", e.message()),
    }
    match hw.platform().uci().get("wireless", "radio0", "country").await {
        Ok(Some(cc)) if !cc.is_empty() => steps.result("country", wcn::set_country(&cc).await),
        Ok(_) => steps.skipped("country", "none configured"),
        Err(e) => steps.failed("country", e.message()),
    }
    hw.mark("hw-wifi-rules");

    // the chip before the vendor extras (ten modprobes and the Mali GPU held it back ~2 s): netifd's own start of
    // hostapd needs the phy, and the rules above are already in place for it
    match hw.platform().modules().module_dir().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())) {
        Some(krel) => {
            order::load_wifi(hw.platform(), &krel, steps).await;
            hw.mark("hw-wifi");
            order::load_extra(hw.platform(), &krel, steps).await;
        }
        None => steps.failed("modules", "no module directory for the running kernel"),
    }
    hw.mark("hw-modules");
}

/// S99: after the delay, the boot counts as good.
pub(crate) async fn post(hw: &Hw, steps: &mut Steps) {
    tokio::time::sleep(hw.config().post_delay).await;
    match hw.mark_boot_good().await {
        Ok(o) => steps.done("boot-good", format!("{o:?}")),
        Err(e) => steps.fatal("boot-good", e.message()),
    }
}
