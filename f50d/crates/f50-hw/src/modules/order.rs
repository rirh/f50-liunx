//! The vendor drivers Android loads and the initramfs does not, in their order (`extra-modules`).
//!
//! Loaded one by one; a failure is reported, never fatal. Which of them a kernel has decides what happens: the vendor
//! 5.4 boot image loads the modem stack itself, a mainline kernel leaves it to here.

use std::path::{Path, PathBuf};

use f50_core::exec::Cmd;
use f50_platform::Platform;

use crate::report::Steps;

/// The modem stack, in dependency order, for a kernel whose boot image does not load it (mainline). The SIPA
/// accelerator attaches to the USB gadget's usb0 as it is created, and a gadget created while these are loaded
/// transmits nothing the host ever sees: so they come here, once the gadget is long since bound. `sipa-dele` is not
/// here: it has to come after the modem is attached to the packet domain (FINDINGS 13f; [`crate::Hw::load_ipa_delegate`]).
const MODEM_STACK: &[&str] = &[
    "sprd_systimer",
    "sprd_time_sync",
    "sprd_time_sync_cp",
    "trusty",
    "trusty-pm",
    "trusty-log",
    "trusty-irq",
    "trusty-ipc",
    "trusty-virtio",
    "sprd_shm",
    "unisoc-mailbox",
    "sprd-sipc-virt-bus",
    "sprd_power_manager",
    "sipc-core",
    "spipe",
    "spool",
    "sipx",
    "seth",
    "slog_bridge",
    "sbuf_bridge",
    "sblock_bridge",
    "sprd_modem_loader",
    "sipa-sys",
    "sipa-core",
    "sipa_eth",
    "sprdbt_tty",
];

/// Mainline's own modules, by path where depmod has no index: the stock virtual thermal zone (FINDINGS 48), the
/// clusters' wake sources (FINDINGS 218), the SAR sensor where a kernel has it.
const MAINLINE_EXTRAS: &[&str] = &["mu300_thermal", "ums9620_lpintc", "zte_sar_stk"];

/// The PMIC's Type-C and BC1.2 detection, on by default as on 5.4 (FINDINGS 234).
const TYPEC: &[&str] = &["sc27xx_typec", "sprd_bc1p2"];

/// Thermal throttling, LEDs, SAR backoff, SIM tray detection, vibrator, DDR/CP DVFS and the Mali GPU.
const VENDOR_DRIVERS: &[&str] = &[
    "sprd_soc_thm",
    "thermal-generic-adc",
    "sprd_cpu_cooling",
    "leds-sc27xx-bltc",
    "zte_sar",
    "zte_card_holder_det",
    "sc27xx-vibra",
    "sprd_cp_dvfs",
    "sprd_ddr_dvfs",
    "sprd_gpu_cooling",
    "mali_kbase",
];

async fn insmod(f: &Path) -> f50_core::Result<String> {
    Cmd::new("/sbin/insmod").arg(f).ok().await
}

/// A file of the module, named either way (`a_b.ko`, `a-b.ko`), anywhere under `dir` (sorted walk).
fn find_ko(dir: &Path, m: &str) -> Option<PathBuf> {
    let names = [format!("{m}.ko"), format!("{}.ko", m.replace('_', "-"))];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<_> = rd.filter_map(Result::ok).collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries.iter().rev() {
            let p = e.path();
            let ft = e.file_type().ok();
            if ft.is_some_and(|t| t.is_dir()) {
                stack.push(p);
            } else if names.iter().any(|n| e.file_name().to_str() == Some(n)) {
                return Some(p);
            }
        }
    }
    None
}

#[derive(Default)]
struct Tally {
    loaded: Vec<String>,
    failed: Vec<String>,
}

impl Tally {
    fn report(self, steps: &mut Steps, name: &str) {
        if self.loaded.is_empty() && self.failed.is_empty() {
            steps.skipped(name, "nothing to load in this kernel");
        } else if self.failed.is_empty() {
            steps.done(name, format!("loaded {}", self.loaded.join(" ")));
        } else {
            steps.failed(name, format!("loaded {}; FAILED {}", self.loaded.join(" "), self.failed.join("; ")));
        }
    }

    fn add(&mut self, m: &str, r: f50_core::Result<String>) {
        match r {
            Ok(_) => self.loaded.push(m.to_owned()),
            Err(e) => self.failed.push(format!("{m}: {}", e.message())),
        }
    }
}

/// `modprobe`, else `insmod` of the file (depmod's index may not know it).
async fn modprobe_or_insmod(p: &Platform, m: &str, f: &Path) -> f50_core::Result<String> {
    match p.modules().load(m, &[]).await {
        Ok(()) => Ok(String::new()),
        Err(_) => insmod(f).await,
    }
}

/// The SC2355 (Marlin3) Wi-Fi's PCIe host drivers: modules on the vendor 5.4 kernel, built in on mainline.
const WIFI_PCIE: &[&str] = &["pcie-sprd-misc", "pcie-sprd"];
/// The WCN platform driver, then the WLAN driver that makes wlan0 (`wifi-start`).
const WIFI_DRIVERS: &[&str] = &["wcn_bsp", "sprd_wlan_combo"];
/// How long the WLAN driver may take to make wlan0 once it is loaded.
const WLAN0_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The Wi-Fi chip's drivers (`wifi-start`), before netifd: the PCIe host, `wcn_bsp`, `sprd_wlan_combo`, then wlan0,
/// then the firmware's ARM log off (`wcn_bsp` turns it on at boot and prints every packet of it to the kernel log).
/// The WCN drivers cannot be reloaded: nothing is loaded when wlan0 is there or a driver is loaded already.
pub(crate) async fn load_wifi(p: &Platform, krel: &str, steps: &mut Steps) {
    let wlan0 = p.paths().sys("/sys/class/net/wlan0");
    if wlan0.exists() {
        steps.skipped("wifi-drivers", "wlan0 is there already");
        return;
    }
    let m_dir = p.paths().sys(format!("/lib/modules/{krel}"));
    for m in WIFI_PCIE {
        // built in on mainline: a missing module is not a failure
        let _ = p.modules().load(m, &[]).await;
    }
    let mut t = Tally::default();
    for m in WIFI_DRIVERS {
        if p.modules().loaded(m) {
            continue;
        }
        let dir = m_dir.clone();
        let mm = (*m).to_owned();
        let r = match tokio::task::spawn_blocking(move || find_ko(&dir, &mm)).await.ok().flatten() {
            Some(f) => modprobe_or_insmod(p, m, &f).await,
            None => p.modules().load(m, &[]).await.map(|()| String::new()),
        };
        let failed = r.is_err();
        t.add(m, r);
        if failed {
            // sprd_wlan_combo needs wcn_bsp's exports
            break;
        }
    }
    if !t.failed.is_empty() {
        t.report(steps, "wifi-drivers");
        return;
    }
    let start = std::time::Instant::now();
    while !wlan0.exists() && start.elapsed() < WLAN0_WAIT {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    if !wlan0.exists() {
        steps
            .failed("wifi-drivers", format!("loaded {}; no wlan0 after {}s", t.loaded.join(" "), WLAN0_WAIT.as_secs()));
        return;
    }
    let armlog = p.paths().sys("/proc/mdbg/at_cmd");
    if armlog.exists() {
        let _ = std::fs::write(&armlog, "at+armlog=0\r");
    }
    let waited = start.elapsed().as_secs();
    if t.loaded.is_empty() {
        steps.skipped("wifi-drivers", format!("loaded already; wlan0 after {waited}s"));
    } else {
        steps.done("wifi-drivers", format!("loaded {}; wlan0 after {waited}s", t.loaded.join(" ")));
    }
}

/// Everything `extra-modules` loaded, in its order.
pub(crate) async fn load_extra(p: &Platform, krel: &str, steps: &mut Steps) {
    let m_dir = p.paths().sys(format!("/lib/modules/{krel}"));
    let dev = |n: &str| p.paths().sys(n);

    let mut t = Tally::default();
    if m_dir.join("sipc-core.ko").is_file() && !dev("/dev/modem").exists() {
        for m in MODEM_STACK {
            let dashed = m.replace('_', "-");
            let Some(f) =
                [m_dir.join(format!("{m}.ko")), m_dir.join(format!("{dashed}.ko"))].into_iter().find(|f| f.is_file())
            else {
                continue;
            };
            t.add(m, insmod(&f).await);
        }
    }
    t.report(steps, "modem-stack");

    // The PM co-processor's watchdog cuts the power ~290 s after boot unless this driver tells pm_sys to stop it
    // (FINDINGS 5). The 5.4 boot image loads it; mainline boot images from before it was ported do not.
    let mut t = Tally::default();
    let wdt = m_dir.join("sprd_pmic_wdt.ko");
    if wdt.is_file() && !p.modules().loaded("sprd_pmic_wdt") {
        t.add("sprd_pmic_wdt", insmod(&wdt).await);
    }
    t.report(steps, "pmic-watchdog");

    let mut t = Tally::default();
    for m in MAINLINE_EXTRAS {
        if p.modules().loaded(m) {
            continue;
        }
        if let Some(f) =
            [m_dir.join(format!("{m}.ko")), m_dir.join(format!("extra/{m}.ko"))].into_iter().find(|f| f.is_file())
        {
            t.add(m, modprobe_or_insmod(p, m, &f).await);
        }
    }
    for m in TYPEC {
        if p.modules().loaded(m) {
            continue;
        }
        let dir = m_dir.clone();
        let mm = (*m).to_owned();
        if let Some(f) = tokio::task::spawn_blocking(move || find_ko(&dir, &mm)).await.ok().flatten() {
            t.add(m, modprobe_or_insmod(p, m, &f).await);
        }
    }
    t.report(steps, "mainline-extras");

    let mut t = Tally::default();
    for m in VENDOR_DRIVERS {
        t.add(m, p.modules().load(m, &[]).await.map(|()| String::new()));
    }
    t.report(steps, "vendor-drivers");

    // The virtual sound card: write to hw:0,0, read it back on hw:0,1 (a program that bridges a call wants ALSA).
    let mut t = Tally::default();
    if !p.modules().loaded("snd_aloop") {
        match p.modules().load("snd-aloop", &[]).await {
            Ok(()) => t.loaded.push("snd-aloop".into()),
            Err(_) => {
                if let Some(f) =
                    [m_dir.join("extra/snd-aloop.ko"), m_dir.join("snd-aloop.ko")].into_iter().find(|f| f.is_file())
                {
                    t.add("snd-aloop", insmod(&f).await);
                }
            }
        }
    }
    t.report(steps, "snd-aloop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_wifi_drivers_are_never_loaded_twice() {
        let r = std::env::temp_dir().join(format!("f50-hw-wifi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(r.join("sys/class/net/wlan0")).expect("wlan0");
        let p = Platform::new(f50_core::paths::Paths::new(&r), None);
        let mut steps = Steps::new(crate::Stage::Hardware, None);
        load_wifi(&p, "5.4.0", &mut steps).await;
        let rep = steps.finish();
        assert_eq!(rep.steps.len(), 1);
        assert_eq!(rep.steps[0].name, "wifi-drivers");
        assert_eq!(rep.steps[0].outcome, crate::report::Outcome::Skipped);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn a_module_is_found_under_either_name() {
        let d = std::env::temp_dir().join(format!("f50-hw-ko-{}", std::process::id()));
        std::fs::create_dir_all(d.join("kernel/drivers/usb")).expect("mkdir");
        std::fs::write(d.join("kernel/drivers/usb/sc27xx-typec.ko"), "").expect("ko");
        assert_eq!(find_ko(&d, "sc27xx_typec"), Some(d.join("kernel/drivers/usb/sc27xx-typec.ko")));
        assert_eq!(find_ko(&d, "sprd_bc1p2"), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
