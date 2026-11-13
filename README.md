# F50

Device software for the ZTE F50 pocket 5G router running OpenWrt.

| Directory | What it is |
|---|---|
| [`f50d/`](f50d/README.md) | The device daemon in Rust. It owns the modem, mobile data, SMS, Wi-Fi/LAN/USB, the system and updates, and publishes them as ubus objects (`f50.*`). |
| `f50-web/` | The web frontend (React + Vite). It calls `f50d` through uhttpd's `/ubus` JSON-RPC endpoint. |
| `install.sh`, `installer/`, `boot/` | The first installation from stock Android: the boot image, the Linux slot, the root filesystem. |
| `systems/openwrt/` | The OpenWrt root filesystem, built in docker from OpenWrt's own release with f50d and the web console in it. |
| `kernel/vendor/` | The 5.4 vendor kernel build (`out/`). |
| `device-tools/`, `android/vendor/`, `scripts/device/` | Small device programs, the Android vendor files taken from the device, backups and logs. |

Based on [dikeckaan/mu300-linux](https://github.com/dikeckaan/mu300-linux); everything an installation needs is built
from this repository (no release of that project is downloaded). The mainline kernels, Ubuntu's web panel and the
documentation are not part of it.

## Commands

`make` lists them all (`make/*.mk`; settings in `make/local.mk`, see `make/local.mk.example`).

```sh
make kernel     # once: the 5.4 kernel into out/ (docker)
make install    # first installation on a stock F50: builds f50d and the web console, flashes OpenWrt
make test       # f50d tests, fmt and clippy, the web console's lint and type check
make deploy     # f50d and the web console onto a running device over SSH
make reboot     # restart the device
```

## First installation

From a Mac or a Linux computer, on a ZTE F50 that still runs its stock firmware. Before `make install`:

1. **Tools on the computer**: `adb` (Android platform-tools; macOS: `brew install android-platform-tools`), Python 3,
   Go 1.20+ for ADB recovery, `lz4`, docker, and the kernel in `out/` (`make kernel`, once).
   The SMB helper is built from pinned dependencies into `work/bin/` before any device operation and reused later.
2. **Cable**: the F50 plugged into the computer with a cable that carries data (some charging cables do not), the
   USB not handed to a virtual machine.
3. **USB debugging on**: open <http://192.168.0.1/index.html#usb_port> (the computer on the F50's Wi-Fi or USB
   network), log in with the **admin password** (not the Wi-Fi password), turn ADB debugging on and save. If the
   computer still does not list the device (`adb devices`), restart the F50 once.
4. `make adb` first checks USB ADB. If it is already connected, it reports success, prints the shell command,
   and exits without preparing SMB/root, changing device settings, or waiting through the stability check.
   If disconnected, recovery writes the original tool's TCP 5555 properties in the first short-lived USB shell,
   then prepares a root channel. Connect the computer to the F50's **Wi-Fi** for the TCP fallback: recovery uses actual adb-only
   USB functions, which removes USB networking and MTP. A fallback connection is accepted only after matching
   the boot ID captured over USB. Existing correct properties and root channels are reused without rebooting.
   On macOS, preparation first checks that the Wi-Fi IPv4 subnet contains the F50 address. A phone hotspot or
   unrelated Wi-Fi subnet stops preparation before any USB toggle or reboot; the check does not replace confirming
   that the selected hotspot belongs to the F50.
   It reuses a direct root shell, working Magisk `su`, or the local root socket. On firmware such as B15 where
   ADB is an ordinary shell, it uses writable `/data/samba/etc/smb.conf` to install a Samba `root preexec` hook,
   enables guest access, and accesses a share to start a local root socket. The original Samba configuration is
   saved to `/data/local/tmp/f50-smb-before-root.conf`. A userspace SMB2/3 client connects to shares read from
   `smb.conf`, avoiding the `smbutil` share-enumeration path. On macOS, a failed userspace attempt can fall back
   to a temporary read-only `mount_smbfs` access to a known share, then unmount it. All SMB/root attempts share
   a 10-second deadline; retries continue within that budget, with time reserved for cleanup and root probes.
   The tool then checks the actual `sys.usb.config/state`, TCP connection and UID 0 for 30
   consecutive seconds before reporting success; property writes alone are not success.
   Without step 3 it offers to do it itself:
   enter the admin password and it switches USB debugging on and restarts the F50, as the Windows F50-ADB tool
   does. Nothing is flashed in this step.
   When a Web password was supplied, it checks `samba_switch` and uses `SAMBA_SETTING` to enable sharing before
   reboot. The ADB bootstrap separately reads `settings get global samba_enable` and enables it only when needed,
   before checking or writing its root hook. A stock service that regenerates the configuration gets one bounded
   repair attempt. An existing hook or socket file alone does not count as working root.
   SMB logs identify protocol negotiation versus session setup without logging authentication data. If automatic
   negotiation cannot establish a session, one SMB 2.1 compatibility attempt may use the remaining deadline;
   no global host SMB/security settings are changed. These are recovery attempts, not a confirmed cause of EOF.
   TCP `offline` transports at this device address are disconnected and reconnected with backoff. Every new
   connection must match the USB boot ID; subsequent writes are pinned to its physical serial and guarded by that ID.
5. `make install`. Installation and maintenance explicitly request `--prepare-root`, so they can prepare root
   even when USB ADB is already connected. Steps 3 and 4 are only needed by hand when that preparation fails.

USB debugging does **not** guarantee a root shell. The installer checks for UID 0 through the selected channel;
it can use `/data/local/tmp/f50-root/command.sock` without installing Magisk. This local socket permits Android's
shell user to execute root commands. Its listener must be started again through Samba after an Android reboot;
the installation/resume path prepares it again when needed. This requires firmware with a writable Samba
configuration and a functioning root hook; failure stops installation. Root preparation does not flash boot or
TrustOS, and does not establish that the device can boot a modified image. Take a backup first: `make backup`.

TCP ADB on port 5555 is now part of the default preparation, as in the original F50-ADB tool; `--wireless` remains
accepted for compatibility. `scripts/device/f50-enable-adb.py --adb-only` only confirms or restores USB ADB,
without root/Samba preparation or the stability claim. `make install` still uses USB and leaves the prepared
adb-only mode in place rather than restoring MTP at exit. This is a per-boot preparation, not a guarantee that
stock firmware will retain debugging forever.

The Python entry point is `scripts/device/f50-enable-adb.py`; its implementation lives in
`scripts/device/f50_adb/`. `config` handles arguments, `host` owns tools/logs/processes, `web` implements
Goform, `adb` tracks transports and device identity, `smb` handles share access, `payloads` supplies Android
commands, and `root`/`workflow` coordinate recovery. Imports do not start preparation or create logs.

Implementation references: [UFI-TOOLS sharing controls](https://github.com/kanoqwq/UFI-TOOLS/blob/cf83b815efb114ed026f06dc90550845aafbfceb/app/frontEnd/public/script/main.js#L1517),
[openSMB](https://github.com/kanoqwq/UFI-TOOLS/blob/cf83b815efb114ed026f06dc90550845aafbfceb/app/src/main/java/com/minikano/f50_sms/utils/ShellKano.kt#L760),
and [the author's short-lived ADB discussion](https://github.com/kanoqwq/UFI-TOOLS/issues/64).

## License

MIT, see [LICENSE](LICENSE).
