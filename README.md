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
   `lz4`, docker, and the kernel in `out/` (`make kernel`, once).
2. **Cable**: the F50 plugged into the computer with a cable that carries data (some charging cables do not), the
   USB not handed to a virtual machine.
3. **USB debugging on**: open <http://192.168.0.1/index.html#usb_port> (the computer on the F50's Wi-Fi or USB
   network), log in with the **admin password** (not the Wi-Fi password), turn ADB debugging on and save. If the
   computer still does not list the device (`adb devices`), restart the F50 once.
4. `make adb` checks that the F50 gives a root adb shell and keeps adb on across reboots
   (`persist.sys.usb.config=adb,rndis`; the installer restarts Android). Without step 3 it offers to do it itself:
   enter the admin password and it switches USB debugging on and restarts the F50, as the Windows F50-ADB tool
   does. Nothing is flashed in this step.
5. `make install`. It runs `make adb` first, so steps 3 and 4 are only needed by hand when that fails.

Stock F50 needs no Magisk and no `su`: with USB debugging on, its adb shell is root, and the installer uses it
directly (Magisk's `su` works too). Take a backup first: `make backup`.

Wireless adb, the rest of what the F50-ADB tool does (TCP port 5555 and Samba opened to guests to start adbd), is not
needed for an installation: `scripts/device/f50-enable-adb.sh --wireless` (macOS).

## License

MIT, see [LICENSE](LICENSE).
