# f50-proxy

The proxy engine (代理与分流) of the ZTE F50 on OpenWrt, in Go: Xray-core as a library, routed by One Node's own
rule engine (sing-box rule semantics), driven by f50d's `f50-proxy` crate over a token-protected loopback API.
The contract between engine, f50d and the web pages is [DESIGN.md](DESIGN.md); §1 is this module.

## Origin

Vendored from `one-node` (`/Users/zh/Documents/workspaces/voh/one/one-node`), no `replace` back to it:

| here | from | revision |
|---|---|---|
| `cmd/f50-proxy`, `internal/*` (but `rulespec`), `internal/geodata/rules/`, `cmd/genrules` | `pc/core` (onecore, module `onespike/desktopcore`) | pc repo `18d4b49` (2026-11-14), last core change `e8a8577` (2026-11-13) |
| `internal/rulespec`, `internal/rulespec/engine` | `backend/pkg/rulespec` (`srs` not used, not copied) | backend repo `353ec7f` (2026-10-31) |

Xray-core is `github.com/xtls/xray-core v1.260327.1-0.20261031132019-a4a3dff5b20c`, as onecore pins it.

## Layout

```text
cmd/f50-proxy/        entry point: serve, check, testserver, version
cmd/genrules/         regenerates internal/geodata/rules/cn-*.txt
internal/
  api/                loopback HTTP API /v1/* (server, subscription and 配置 fetches, device headers)
  engine/             the running instance: start/stop, 策略组, health watcher, resolving node servers
  router/             Xray's router feature replaced by a route.Policy
  route/              配置 document → rule-engine steps, groups, Policy; rule-set catalogue and its disk cache
  probe/              node delay: HTTP through the node, TCP, ICMP
  link/               share links and subscriptions → nodes and Xray outbounds
  tracker/            connections through the instance (连接 page) and their trouble lines
  logtap/             the process's logger: Xray's log handler, own notes, the 日志 page
  xrayconf/           the Xray JSON document, its build, the tags packages share inside Xray
  geodata/            embedded lists (rules/: mainland, overseas, apps) and private ranges
  netmark/            SO_MARK and bootstrap DNS for the engine's own sockets
  dailylog/ platform/ version/
  rulespec/           One Node's rule language and engine (vendored from the backend)
```

Dependencies only point down the list: `api` → `engine` → `router` → `route`, `probe` → `link`, `tracker` →
`xrayconf`/`logtap`/`geodata`/`netmark` → the leaves. `logtap` is the only process-wide state besides the rule-set
catalogue cache and the socket mark. Tests sit with the package they test; the end-to-end ones (a real Xray server in
a child process, `testserver`) are in `internal/engine`.

## What differs from onecore

- **Packages.** onecore is one flat `package main`; here it is split into `cmd/f50-proxy` and `internal/*`
  (see Layout), with the logic unchanged: names became exported, and a few accessors stand in
  for fields other packages read (`Engine.Tracker`, `Route.FinalTag`, `netmark.Set`/`Get`, `logtap.Note` …).
- **Linux only.** Removed: the darwin/windows files, the elevated TUN helper (`install-helper`, `--idle-stop`), the
  Windows service, the NetworkExtension entry point, the system default-route watch, the tunnel-address heuristics in
  the health watcher, SMBIOS device IDs.
- **Mode `gateway`** replaces `tun` (`proxy` stays): a `dokodemo-door` inbound `tproxy` on `[::]:<tproxy_port>`
  (`tcp,udp`, `followRedirect`, `sockopt.tproxy`, sniffing without `routeOnly`), DNS `UseIPv4`, `direct` with
  `ForceIPv4`, `SO_MARK <mark>` on every outbound but `block` (front/fragment hops, other nodes, `dns-out`) and on the
  tproxy inbound (Xray's FakeUDP reply sockets), node servers resolved to IPv4 before the start, and
  `tproxy` + port 53 → `dns-out` before any rule. `StartRequest` gains `tproxy_port` (default 17892) and `mark`
  (default 255); `port` defaults to 17890.
- **The engine's own sockets** (rule-set downloads, `tcp` probes, URL probes'
  instances, resolving node servers) carry the running gateway's mark too, and resolve through `223.5.5.5` /
  `119.29.29.29` directly instead of dnsmasq, whose upstream queries the nftables rules send back to the engine.
- **`Conn.source`**: the client's IP (from the inbound's source; IPv4-mapped addresses written as IPv4).
- **Subscription headers**: `User-Agent: v2rayN/7.15 F50Proxy/<ver>`, `X-Device-OS: OpenWrt`,
  `X-Ver-OS` (`DISTRIB_RELEASE` of `/etc/openwrt_release`), `X-Device-Model: ZTE F50`, and `X-HWID`: `/etc/machine-id`, else the eMMC CID
  (`/sys/block/mmcblk0/device/cid`), hashed. The One Node gateway shows the router as client `One Node F50`
  (policy id `one-node-f50`) and requires the HWID under its client policy.
- **Rule-set cache on disk**: `serve --cache-dir` (default `/etc/f50d/proxy/rulesets`; empty = memory only) keeps
  the catalogue's rule sets with their ETag, so a boot without network still has them.
- ICMP probes fall back to a raw socket as root (OpenWrt's `ping_group_range` admits no group).
- Names: `F50PROXY_TOKEN`, `F50PROXY_LISTEN <addr>` on start-up, log lines prefixed `f50-proxy:`.

The loopback API (`/v1/*`, Bearer token, `{"ok":…}` envelope with Content-Length) is otherwise onecore's.

## Build

```sh
make device    # dist/f50-proxy-linux-arm64: CGO_ENABLED=0 GOOS=linux GOARCH=arm64, -trimpath -s -w (~30 MB)
make build     # host binary dist/f50-proxy
make test      # go vet + go test ./... (the tests bind loopback ports)
make device VERSION=0.2.0   # stamps internal/version.Version (default 0.1.0)
```

`main/distro/all` stays: an explicit list of only the features used saved 0.2 MB, because `infra/conf` references
every config type anyway.

`cmd/genrules` regenerates `internal/geodata/rules/cn-ip.txt` and `cn-domain.txt` from APNIC data and a sing-box rule source
(both in the one-node workspace): `go run ./cmd/genrules --apnic … --domains …`.

## On the device

Installed as `/opt/f50/bin/f50-proxy`, run always by procd (`/etc/init.d/f50-proxy`, respawn):

```sh
f50-proxy serve --listen 127.0.0.1:17891 --token-file /etc/f50d/proxy/token \
  --log /var/log/f50-proxy/core.log --loglevel warning
```

f50d writes the token file (0600) before the service starts. The engine is idle until f50d sends `/v1/start` with
`mode: "gateway"`; f50d then installs the nftables table `inet f50_proxy` and the fwmark-1 policy route that bring
LAN (and optionally the router's own) traffic to the tproxy port, and removes them before `/v1/stop`. The engine
needs root (`CAP_NET_ADMIN` for `IP_TRANSPARENT` and `SO_MARK`). `--log` writes `core-<date>.log`, seven days kept.

`f50-proxy check --links file --mode proxy` starts every link of a file in turn and fetches through the mixed port;
`f50-proxy testserver` runs a loopback REALITY node; `f50-proxy version` prints the versions.
