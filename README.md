# F50

Device software for the ZTE F50 (MU300) pocket 5G router running OpenWrt.

| Directory | What it is |
|---|---|
| [`f50d/`](f50d/README.md) | The device daemon in Rust. It owns the modem, mobile data, SMS, Wi-Fi/LAN/USB, the system and updates, and publishes them as ubus objects (`f50.*`). |
| `f50-web/` | The web frontend (React + Vite). It calls `f50d` through uhttpd's `/ubus` JSON-RPC endpoint. |

Based on [dikeckaan/mu300-linux](https://github.com/dikeckaan/mu300-linux). The kernel, boot chain, installer,
Ubuntu/Arch images and the old shell API and LuCI panel are no longer part of this repository.

## Build

```sh
cd f50d
cargo check --workspace --all-targets
cargo device        # static aarch64 binary: target/aarch64-unknown-linux-musl/release/f50d
```

## License

MIT, see [LICENSE](LICENSE).
