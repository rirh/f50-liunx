# f50d

The ZTE F50 device daemon for OpenWrt, in Rust. One process owns the modem's AT channels, the radio, mobile data,
SMS, Wi-Fi/LAN/USB, the system and updates, and publishes them as **ubus objects**. A web frontend reaches them
through uhttpd's `/ubus` JSON-RPC endpoint, where rpcd's login sessions and ACLs decide who may call what.

It replaces the shell layer this repository grew (`/opt/mu300/bin/*`, `mu300-api`, the LuCI backend `mu300dash` and
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
| `f50-core` | error codes, the RPC method model and field validation, the process runner, jobs, private/atomic files, paths | `json.awk`, `lib.sh`, the validation of `mu300-api`/`mu300dash` |
| `f50-ubus` | the ubus wire protocol (blob/blobmsg), client calls, serving objects | libubus (not linked) |
| `f50-platform` | OpenWrt: UCI, netifd, procd services, sysfs, modules, system settings, downloads | the `uci`/`ifstatus`/`/etc/init.d` calls scattered through every script |
| `f50-at` | the SIPC AT channels, lanes, sessions, URCs, the terminal's guards and history (`f50.at`) | `mu300-at`, `mu300-atd`, `init.d/mu300-atd*`, `unisoc-modem/at`, `modem-log-drain`, `mu300-cmt-spool` |
| `f50-modem` | SIM slots and state, the radio, network/band/cell/EN-DC locks and their boot replay, the signal/cell collector, the radio lock (`f50.modem`, `f50.sim`, `f50.lock`) | `unisoc-modem/{cell,lock,boot-replay,imei-diag,action}`, `mu300-sim*`, `sim-config.sh`, `mu300-modem-coldboot` |
| `f50-data` | the cellular data connection (dial, IPv4/IPv6, APN, roaming, watchdog), netifd's `f50cell` protocol, data usage, IPv6 relay (`f50.data`, `f50.traffic`) | `mobile-data`, `proto/mu300cell*.sh`, `mu300-traffic`, `ndp-learn`, `ra-deprecate`, `19-mu300-fw4` |
| `f50-sms` | the SMS pool, PDU encode/decode, receiving (URC) and sending (`f50.sms`) | `mu300-sms`, `mu300-smsd`, `sms-pdu.awk`, `sms` |
| `f50-net` | the Wi-Fi hotspot, the Wi-Fi client uplink, the LAN, the USB port's role and network function, adbd (`f50.wifi`, `f50.wificlient`, `f50.lan`, `f50.usb`) | `wifi-client`, `hotspot-*`, `openwrt-wifi-*`, `mu300-wifi-band`, `wifi-start`, `wifi-ready`, `mu300-lan-ip`, `mu300-usb*`, `device-usb`, `usb-host.sh`, `10-mu300-usb`, `mu300-adbd` |
| `f50-system` | device facts and live figures, CPU profiles, the thermal guard, LEDs, buttons, zram, hostname/time/password, reboot (`f50.system`, `f50.cpu`) | `dashboard-info`, `mu300-device`, `mu300-cpu`, `thermal-guard`, `mu300-led`, `mu300-buttons`, `mu300-zram`, `zram-start` |
| `f50-hw` | boot-time bring-up: kernel modules, device permissions, the vendor daemons the modem needs, Wi-Fi/Bluetooth firmware, boot success marks | `init.d/mu300-{hw,vendor,post,accounts}`, `android-vendor-start`, `android-run`, `sipa-dele-start`, `ueventd-perms.sh`, `extra-modules`, `mu300-modprobe-guard`, `wcn-reopen`, `regdb-load`, `10-mu300-wcn`, `bootmark`, `mu300-next-boot`, `early-recorder`, `kmsg-forward` |
| `f50-update` | release check, download, signature, apply, rollback for F50 OpenWrt (`f50.update`) | `mu300-update`, `unisoc-modem/update` |
| `f50-api` | assembles every object, the aggregate `f50.status`, the ACL file and API reference generated from the method table | `mu300-api`, `mu300dash` |
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
`libc`, `sha2`, `minisign-verify`. A new dependency is a workspace decision, recorded here with its reason.

## Build

```sh
cargo check --workspace --all-targets            # host check
cargo clippy --workspace --all-targets -- -D warnings
cargo device                                     # static aarch64 binary: target/aarch64-unknown-linux-musl/release/f50d
```

## OpenWrt integration

`openwrt/` holds what the image installs beside the binary: the procd init script, the netifd protocol handler
(`f50cell`), the rpcd ACL (`/usr/share/rpcd/acl.d/f50.json`, generated from the method table), and the UCI
defaults. `systems/openwrt/build-rootfs.sh` builds `f50d` and installs them.
