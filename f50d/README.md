# f50d

The ZTE F50 device daemon for OpenWrt, in Rust. One process owns the modem's AT channels, the radio, mobile data,
SMS, Wi-Fi/LAN/USB, the system and updates, and publishes them as **ubus objects**. A web frontend reaches them
through uhttpd's `/ubus` JSON-RPC endpoint, where rpcd's login sessions and ACLs decide who may call what.

It replaces the shell layer this repository grew (`/opt/f50/bin/*`, `f50-api`, the LuCI backend `f50dash` and
the `unisoc-modem` adapters). Scope: **the F50, on OpenWrt**. No Ubuntu, Arch, U30 Air or V50 paths are carried over.

## Crates

```text
f50d (bin) ─────────► f50-api ─┬─► f50-modem ─┬─► f50-at ──────► f50-core
   │ composition root,         ├─► f50-data ──┤                  ▲
   │ ubus server, CLI,         ├─► f50-sms ───┘                  │
   │ boot stages               ├─► f50-net ─────────┐            │
   │                           ├─► f50-system ──────┤            │
   └─► f50-ubus                ├─► f50-hw ──────────┼─► f50-platform ─► f50-ubus
                               └─► f50-update ──────┘
```

| crate | owns | ports (shell it replaces) |
|---|---|---|
| `f50-core` | error codes, the RPC method model and field validation, the process runner, jobs, private/atomic files, paths | `json.awk`, `lib.sh`, the validation of `f50-api`/`f50dash` |
| `f50-ubus` | the ubus wire protocol (blob/blobmsg), client calls, serving objects | libubus (not linked) |
| `f50-platform` | OpenWrt: UCI, netifd, procd services, sysfs, modules, system settings, downloads | the `uci`/`ifstatus`/`/etc/init.d` calls scattered through every script |
| `f50-at` | the SIPC AT channels, lanes, sessions, URCs, the terminal's guards and history (`f50.at`) | `f50-at`, `f50-atd`, `init.d/f50-atd*`, `unisoc-modem/at`, `modem-log-drain`, `f50-cmt-spool` |
| `f50-modem` | SIM slots and state, the radio, network/band/cell/EN-DC locks and their boot replay, the signal/cell collector, the radio lock (`f50.modem`, `f50.sim`, `f50.lock`) | `unisoc-modem/{cell,lock,boot-replay,imei-diag,action}`, `f50-sim*`, `sim-config.sh`, `f50-modem-coldboot` |
| `f50-data` | the cellular data connection (dial, IPv4/IPv6, APN, roaming, watchdog), netifd's `f50cell` protocol, data usage, IPv6 relay (`f50.data`, `f50.traffic`) | `mobile-data`, `proto/f50cell*.sh`, `f50-traffic`, `ndp-learn`, `ra-deprecate`, `19-f50-fw4` |
| `f50-sms` | the SMS pool, PDU encode/decode, receiving (URC) and sending (`f50.sms`) | `f50-sms`, `f50-smsd`, `sms-pdu.awk`, `sms` |
| `f50-net` | the Wi-Fi hotspot, the Wi-Fi client uplink, the LAN, the USB port's role and network function, adbd (`f50.wifi`, `f50.wificlient`, `f50.lan`, `f50.usb`) | `wifi-client`, `hotspot-*`, `openwrt-wifi-*`, `f50-wifi-band`, `wifi-start`, `wifi-ready`, `f50-lan-ip`, `f50-usb*`, `device-usb`, `usb-host.sh`, `10-f50-usb`, `f50-adbd` |
| `f50-system` | device facts and live figures, CPU profiles, the thermal guard, LEDs, buttons, zram, hostname/time/password, reboot (`f50.system`, `f50.cpu`) | `dashboard-info`, `f50-device`, `f50-cpu`, `thermal-guard`, `f50-led`, `f50-buttons`, `f50-zram`, `zram-start` |
| `f50-hw` | boot-time bring-up: kernel modules, device permissions, the vendor daemons the modem needs, Wi-Fi/Bluetooth firmware, boot success marks | `init.d/f50-{hw,vendor,post,accounts}`, `android-vendor-start`, `android-run`, `sipa-dele-start`, `ueventd-perms.sh`, `extra-modules`, `f50-modprobe-guard`, `wcn-reopen`, `regdb-load`, `10-f50-wcn`, `bootmark`, `f50-next-boot`, `early-recorder`, `kmsg-forward` |
| `f50-update` | release check, download, signature, apply, rollback for F50 OpenWrt (`f50.update`) | `f50-update`, `unisoc-modem/update` |
| `f50-api` | assembles every object, the aggregate `f50.status`, the ACL file and API reference generated from the method table | `f50-api`, `f50dash` |
| `f50d` | the binary: configuration, wiring, lifecycle and shutdown, the ubus server, the CLI, the boot stages procd runs | the init scripts' logic |

**Dependency rules.** Arrows point one way. A capability crate never depends on `f50-api`, on `f50d`, or on a
sibling capability except as drawn (`f50-data → f50-modem`, `f50-modem/f50-sms → f50-at`). Transport (ubus) is the
binary's business: capability crates publish [`f50_core::rpc::Object`]s and never see a ubus message.

## Conventions

- **Errors.** Every operation returns `f50_core::Result`. Choose the `Code` a caller can act on (`busy`, `timeout`,
  `not-found`, ...); the message is for people and is sanitised by construction. Add context with `ResultExt`.
- **Requests.** A method's fields are a `&'static [Field]` with a rule each; the handler only ever sees checked
  `Params`. Secrets (passwords, SMS text, AT commands) are `Field::secret`: they go to a program's stdin or to the
  modem, are never logged, never echoed, never put in a job log.
- **Replies.** Handlers return serialisable data; the transport wraps it as `{"ok":true,"data":...}` or
  `{"ok":false,"code":"...","error":"...","field":"..."}`. Field names are `snake_case`; numbers are numbers,
  booleans are booleans, absent is `null`.
- **Long work** (over ~15 s, or anything that restarts the radio) is a job (`f50_core::jobs`); the method returns
  `{"job":"<id>"}` and the caller polls `f50.job status`.
- **Readings never wait for the modem.** Radio state is collected in the background and served from the latest
  snapshot with its age.
- **Programs** run only through `f50_core::exec::Cmd` (timeout, process group, bounded output, stdin for secrets).
  Prefer doing the work in Rust: a program is called only when it *is* the platform (`uci`, `ubus`, `wpa_supplicant`,
  `hostapd_cli`, `modprobe`, `uclient-fetch`, the vendor daemons).
- **Files** that f50d writes go through `f50_core::fsx` (atomic, private, no symlink following). Paths go through
  `Paths` so tests can move the root.
- **Async.** Every spawned task belongs to the `TaskTracker` it was given and stops at its `CancellationToken`. No
  lock is held across `.await` (clippy denies `await_holding_lock` for std locks; use `tokio::sync` where a guard must
  cross one). Blocking I/O on a slow device goes through `spawn_blocking` or is short and bounded.
- **unsafe** only in modules that talk to the kernel directly (termios, process groups, setuid, capabilities,
  mounts), with `#[allow(unsafe_code)]` on that module and a `// SAFETY:` comment on every block.
- **Files stay under 500 lines.** Split by responsibility (`lock/apply.rs`, `lock/state.rs`), never by size.
- **Docs.** Each module starts with `//!` saying what it owns. Where the behaviour encodes a hardware finding, cite it
  (`docs/research/FINDINGS.md 185`) so the reason survives the rewrite.

## Dependencies

All versions live in the workspace `Cargo.toml`; crates write `name.workspace = true`. The set is deliberately
small: `tokio`, `tokio-util`, `serde`, `serde_json`, `thiserror`, `tracing`, `tracing-subscriber`, `regex`,
`libc`, `sha2`, `minisign-verify`, and `md-5` (f50-sms only: the migrated SMS pool names messages by MD5, an
identity, never a security property). A new dependency is a workspace decision, recorded here with its reason.

## Build

```sh
cargo check --workspace --all-targets            # host check
cargo clippy --workspace --all-targets -- -D warnings
cargo device                                     # static aarch64 binary: target/aarch64-unknown-linux-musl/release/f50d
```

## Processes

One binary, several roles (`f50d <role>`, see `crates/f50d/src/cli.rs`):

| role | started by | what |
|---|---|---|
| `at-keeper` | `/etc/init.d/f50d-at` (S13) | opens each SIPC AT channel once and holds it; hands duplicates to the daemon over `/var/run/f50d/at-keeper.sock` (SCM_RIGHTS), so a crash, restart or update of the daemon never closes a channel (FINDINGS 13b); drains the modem's log channel |
| `daemon` | `/etc/init.d/f50d` (S19) | everything else; registers the `f50.*` objects on ubus |
| `boot vendor`, `boot vendor-exec <d>` | `/etc/init.d/f50d-vendor` (S09) | the vendor daemons in the chroot |
| `boot hardware` | `/etc/init.d/f50d-hw` (S12) | modules, regdb, board, the module helper |
| `boot post` | `/etc/init.d/f50d-post` (S99) | marks the boot good (the A/B fallback) |
| `boot early` | preinit `05_f50_early_recorder` | the early boot recorder |
| `boot trial` | f50-update | arms a trial boot of a new boot image |
| `proto setup/teardown/renew` | netifd (`proto f50cell`) | forwards to `f50.proto` |
| `hotplug iface/net/ntp/wcn` | hotplug.d | forwards to `f50.daemon` (wcn runs itself) |
| `call <object> <method> [json]`, `methods`, `acl` | a shell | one call to the running daemon |
| `openapi [--from <describe.json>]` | a shell, `docs/api/generate.sh` | the API as OpenAPI 3.1 (samples the read methods for response shapes) |
| (as `modprobe-guard`) | the kernel | the module helper |

## API

**OpenAPI / Swagger:** [`docs/api/openapi.json`](docs/api/openapi.json), generated from the method table by `f50d openapi`
on a device (`docs/api/generate.sh` regenerates it over adb or `SSH=root@...`). Request schemas are the validation rules;
response `data` schemas are inferred from sampled replies (shape only, never values). Methods appear as virtual paths
`POST /{object}/{method}`; the web console ships Swagger UI at `/f50/api-docs/` whose "Try it out" sends the real
`/ubus` JSON-RPC call with the console's login.

Every object is `f50.<area>`; `f50.meta describe` returns the whole table (methods, access, field rules), and
`f50.status get` the dashboard in one call. Over HTTP, uhttpd's `/ubus` JSON-RPC with an rpcd session:

```json
{"jsonrpc":"2.0","id":1,"method":"call","params":["<session>","f50.status","get",{}]}
```

The daemon writes the rpcd ACL generated from the method table (`/usr/share/rpcd/acl.d/f50.json`: `f50-read`, every
read method; `f50-admin`, everything) and reloads rpcd when it changed. `f50.proto` and `f50.daemon` are for the
device itself and are in no ACL.

## OpenWrt integration

The web console (`../f50-web`, React) is installed by the same script when its build is given as the fourth
argument: `install.sh ROOT BINARY SRC f50-web/dist` puts it in `/www/f50` and points the device's front page there.

`openwrt/` holds what the image installs beside the binary: the procd init scripts, the hotplug hooks, the netifd
protocol handler (`f50cell`) and `/etc/config/f50d`. `openwrt/install.sh ROOT BINARY` installs them and takes the shell
programs of the image's overlay that f50d replaces out of the root; `systems/openwrt/build-rootfs.sh` runs it after
everything else is installed.

Kept from /opt/f50: the programs that are not shell (busybox, adbd, logdw, the carry helper...), the vendor chroot
(`/opt/f50/android`, where the installer puts it), the release key, and the helpers preinit's early USB DHCP still
uses.

The names are the F50's throughout: `/etc/f50`, `/run/f50`, `/opt/f50`, `/mnt/f50-disk`, `.f50` on the Linux disk,
the `f50root` partition, the `f50vnd` account, the `f50_wifi_*` nft tables, and the release files `f50-release`,
`f50-<system>-rootfs.tar.gz` and `f50-kernel*.tar.gz` (with `etc/f50/image-version` and the `f50-image-manifest` inside
an image). Devices installed with the upstream project's names are not supported: they are installed anew.

## Status

Written, compiled and linted (`cargo clippy -D warnings`, host and aarch64-unknown-linux-musl); the unit tests pass on
the host (`cargo test --workspace`). **Not yet run on a device.** Each crate's module docs list what is unverified
without the hardware.
