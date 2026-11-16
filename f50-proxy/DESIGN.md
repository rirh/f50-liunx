# Proxy and split routing (代理与分流) on the F50 — design contract

Three pieces, one contract. Each piece is built against this document; change it here first.

```text
LAN client ─► br-lan ─► nft table inet f50_proxy (prerouting, tproxy) ─┐
router's own traffic ─► nft output (mark 1) ─► ip rule fwmark 1 → lo ─┤
                                                                      ▼
                     f50-proxy engine (Go: Xray-core + rulespec)  tproxy inbound :17892
                     loopback control API 127.0.0.1:17891 ◄── f50d crate f50-proxy (ubus f50.proxy)
                     every outbound socket: SO_MARK 0xff (never intercepted again)   ▲
                                                                     f50-web /proxy pages (uhttpd /ubus)
```

Ported from `one-node` (`/Users/zh/Documents/workspaces/voh/one/one-node`): the engine is `pc/core` (onecore) with
`backend/pkg/rulespec` vendored; the f50d crate takes the role of the Tauri shell (`pc/src-tauri`: profile, configs,
groups, failover); the pages are `web/src/app/pc` rebuilt in f50-web's own design system.

## 1. Engine: `f50-proxy/` (Go module, vendored onecore)

- Module `f50/proxy`; `rulespec` (+ `engine`, and `srs` only if used) under `internal/rulespec`. No `replace` to
  another repository. Only Linux is a target: the darwin/windows files, the elevated helper, the system proxy/DNS,
  native NetworkExtension and Windows service code are removed; `internal/engine/health.go` keeps what makes sense
  on a router. Entry point `cmd/f50-proxy`; the engine is split into `internal/` packages (README.md, Layout).
- Built `CGO_ENABLED=0 GOOS=linux GOARCH=arm64 go build -trimpath -ldflags "-s -w -X f50/proxy/internal/version.Version=<v>" ./cmd/f50-proxy` →
  `f50-proxy/dist/f50-proxy-linux-arm64`. Installed on the device as `/opt/f50/bin/f50-proxy`.
- Runs always (procd service `/etc/init.d/f50-proxy`, respawn):
  `f50-proxy serve --listen 127.0.0.1:17891 --token-file /etc/f50d/proxy/token --log /var/log/f50-proxy/core.log --loglevel warning`.
  The token file is written by f50d (0600, 32 hex chars) before the service starts. It is idle until `/v1/start`.
- The loopback API is onecore's unchanged (`/v1/status`, `/v1/start`, `/v1/stop`, `/v1/connections`,
  `/v1/connections/close`, `/v1/logs`, `/v1/probe`, `/v1/subscription`, `/v1/rules/check`, `/v1/rules/fetch`,
  `/v1/groups`, `/v1/groups/select`, `/v1/groups/test`, `/v1/shutdown`), Bearer token, envelope
  `{"ok":true,"data":…}` / HTTP 422 `{"ok":false,"error":"…"}`, every reply with Content-Length.

### Changes from onecore

1. **`mode: "gateway"`** (new; `proxy` stays, `tun` goes). Inbounds:
   - `mixed`: SOCKS5+HTTP on `127.0.0.1:<port>` (default 17890), as before.
   - `tproxy`: `dokodemo-door`, listen `::` (dual stack), port `tproxy_port` (default 17892), `network: "tcp,udp"`,
     `followRedirect: true`, `streamSettings.sockopt.tproxy: "tproxy"`, sniffing `["http","tls","quic"]`
     (`routeOnly: false`, as onecore's TUN).
   - DNS: `queryStrategy: "UseIPv4"` (no AAAA answers, as onecore's TUN), `freedom` direct with
     `domainStrategy: "ForceIPv4"`.
   - Every outbound (proxy, `node:*`, via/fragment hops, direct, dns-out; block needs none) gets
     `streamSettings.sockopt.mark: <mark>` (`mark`, default 255) so the nftables rules never catch the engine itself.
   - Node server domains are resolved to IPv4 before the start (onecore's `resolveServers`, used for TUN today):
     otherwise dialing the node would loop through the interception.
   - `fixedRoute`: inbound `tproxy` with port 53 → `dns-out` (DNS from LAN clients and from dnsmasq's upstream queries
     is answered by Xray's DNS with the split, cn domains by `223.5.5.5` DoH direct, the rest by `1.1.1.1` DoH through
     the node). The TUN-only IPv6 block does not apply to `tproxy`.
   - New `StartRequest` fields: `tproxy_port` (int), `mark` (int). Mode `gateway` needs no root beyond what the
     service has (it runs as root under procd; `CAP_NET_ADMIN` for IP_TRANSPARENT/SO_MARK).
2. **`Conn.source`**: the client address (IP only) of every connection, from the inbound's source.
3. **Subscription fetch headers**: `User-Agent: v2rayN/7.15 F50Proxy/<ver>`, `X-Device-OS: OpenWrt`,
   `X-Ver-OS` (`DISTRIB_RELEASE` of `/etc/openwrt_release`), `X-Device-Model: ZTE F50`, `X-HWID` = hashed `/etc/machine-id`, else the eMMC CID
   (`/sys/block/mmcblk0/device/cid`; stock OpenWrt has no machine-id). One Node's gateway calls the client
   `One Node F50` (policy id `one-node-f50`) and refuses it without X-HWID once its client policy is enforced.
4. Rule sets: embedded ones as in onecore; the remote catalogue base comes from `rule_sets` in the start request
   (f50d's default: `https://marseo.eu.org/api/v1/one/rules/`); cache under `--cache-dir` (default
   `/etc/f50d/proxy/rulesets`).

## 2. f50d crate `f50-proxy`, ubus object `f50.proxy`

Depends on `f50-core` (and `f50-platform` if it needs procd/services). Talks to the engine with a small HTTP/1.1
client over `tokio::net::TcpStream` (request with Content-Length, read the reply's Content-Length body; 30 s bound).
Owns:

- **State** in `<state>/proxy/` (`/etc/f50d/proxy/`, 0700): `profile.json` (0600, holds links = credentials),
  `token` (0600). Written through `f50_core::fsx`.
- **Supervisor task**: every 5 s, `GET /v1/status`. Desired on and engine not running (engine restarted, first
  boot) → send the start request again, then (re)apply the firewall. Desired off → firewall removed, `/v1/stop`.
  Subscription refresh every `subscription_refresh_hours` (0 = never). Auto failover: when the engine's health says
  the selected node is down for 2 checks and another node in the same subscription answers a probe, switch
  (as `pc/src-tauri/src/failover.rs`), logged.
- **Firewall** (only while running; removed on off and at f50d shutdown):
  - `nft -f` a whole table `inet f50_proxy` (replace atomically: `delete table` + definition in one file):
    - sets: `bypass4`/`bypass6` (private, loopback, link-local, multicast, CGNAT `100.64.0.0/10`, reserved),
      `lan_if` (ifname: the LAN bridge `br-lan` plus the USB network interface when it is bridged elsewhere — read from
      UCI/netifd), `bypass_mac` (ether_addr, from settings).
    - `prerouting` (filter, hook prerouting, priority mangle): `meta mark 0xff return`; packets on `lo` with mark 1
      (the router's own, re-routed) or on `@lan_if` continue, everything else returns; `ether saddr @bypass_mac return`;
      `udp/tcp dport 53` to a non-local address → tproxy (before the bypass sets, so carrier DNS on 10.x is still
      split); `fib daddr type local return`; `ip daddr @bypass4 return`; `ip6 daddr @bypass6 return`;
      `meta l4proto {tcp,udp} meta mark set 1 tproxy ip to 127.0.0.1:17892 accept` and
      `… tproxy ip6 to [::1]:17892 accept`.
    - `output` (route, hook output, priority mangle), only when `intercept_router`: `meta mark 0xff return`;
      the same DNS rule and bypass returns; `meta l4proto {tcp,udp} meta mark set 1`.
  - Policy routing: `ip rule add fwmark 1 lookup 117 pref 1170` and `ip route replace local default dev lo table 117`,
    the same with `ip -6`. (Check table 117 / pref 1170 are unused by f50-data/f50-net.)
  - Order on start: engine `/v1/start` OK → routes/rules → nft. On stop: nft → rules/routes → `/v1/stop`. A failed
    step undoes the earlier ones and reports `failed`.

### Profile (`profile.json`, returned whole by `profile`)

```jsonc
{
  "enabled": false,
  "routing": "rule",                    // "rule" 智能分流 | "global" 全局代理 | "direct" 全部直连
  "selected": "s1-3",                   // node id or null
  "subscriptions": [{
    "id": "s1", "name": "机场", "url": "https://…",
    "title": "Profile-Title or null",
    "usage": {"upload": 0, "download": 0, "total": 0, "expire": 0} /* or null */,
    "updated_at": 1763280000,           // unix seconds or null
    "error": null,                      // last refresh error or null
    "nodes": [Node]
  }],
  "local_nodes": [Node],                // ids "L1", "L2", …
  "configs": [Config],                  // configs[0] is the built-in {"id":"default","builtin":true,…}, never stored
  "active_config": "default",
  "group_selections": {"default": {"<group>": "<node name>"}},
  "settings": {
    "intercept_router": true,           // the router's own traffic goes through the proxy too
    "bypass_macs": ["aa:bb:cc:dd:ee:ff"],
    "probe_url": "https://www.gstatic.com/generate_204",
    "subscription_refresh_hours": 12,   // 0 = never
    "rule_sets_url": "https://marseo.eu.org/api/v1/one/rules/",
    "auto_failover": true,
    "log_level": "warning"              // error | warning | info | debug
  }
}
```

- `Node`: `{"id","name","kind","protocol","server","port","link","summary","problem":null|string,"warnings":[],"via":null|id}`
  (the engine's `Node` plus `id` and `via`). Subscription node ids are `<sub id>-<n>` (1-based); after a refresh the
  selection follows the node's name. Names are made unique across all nodes (`name (2)`) before they reach the engine,
  because rules and groups refer to nodes by name.
- `Config`: `{"id":"c1","name","text","url":null|string,"updated_at","builtin":false,"info":RulesInfo|null}`, `text`
  the route document JSON (`{groups, apps, rules, rule_set, final}`, as one-node's `route-doc.ts`). The built-in
  default is one-node's (`pc/src-tauri/src/configs.rs`): geosite-anthropic / -openai / -google-gemini → proxy,
  final → proxy.
- `RulesInfo`: the engine's `/v1/rules/check` reply `{"final","groups","source","rules","warnings"}`.

### Methods

Access: `R` = `f50-read`, `W` = admin. Everything that shows links, hosts or client addresses is `W`.

| method | access | request | reply `data` |
|---|---|---|---|
| `status` | R | – | `Status` (below); also a live topic `proxy` every 2000 ms |
| `profile` | W | – | `Profile` |
| `set` | W | `enabled?` bool, `routing?` enum, `selected?` id, `active_config?` id | `Status` (applies at once: restarts the engine with the new start request when enabled) |
| `settings_set` | W | `intercept_router?` bool, `bypass_macs?` (comma list, may be empty), `probe_url?`, `subscription_refresh_hours?` 0-168, `rule_sets_url?`, `auto_failover?` bool, `log_level?` enum | `Profile.settings` (re-applies firewall/engine when running) |
| `subscription_add` | W | `url`, `name?` | job; result `{"id","nodes":n}` |
| `subscription_update` | W | `id` (or `"all"`) | job; result `{"updated":n,"failed":n}` |
| `subscription_set` | W | `id`, `name?`, `url?` | `Profile` |
| `subscription_remove` | W | `id` | `Profile` |
| `node_inspect` | W | `text` (one or more share links, Body ≤ 200000) | `{"nodes":[Node without id/via]}` (each with its `problem`) |
| `node_save` | W | `id?` (absent = new local node), `link`, `name?`, `via?` (node id or empty) | `Node` |
| `node_delete` | W | `id` (local nodes only) | `Profile` |
| `probe` | W | `id`, `method?` `url`/`tcp`/`icmp` | `{"ms":n}` (`failed` with the reason when it does not answer) |
| `group_select` | W | `group`, `node` | `[GroupStatus]` (remembered in `group_selections`) |
| `group_test` | W | `group` | `[GroupStatus]` |
| `config_save` | W | `id?`, `name`, `text` (Body ≤ 256000), `url?` | `Config` (refused with the engine's error when the rules do not check) |
| `config_delete` | W | `id` (not `default`; the active one falls back to `default`) | `Profile` |
| `config_fetch` | W | `url` | `{"text"}` |
| `rules_check` | W | `text` | `RulesInfo` |
| `connections` | W | – | `{"epoch","active":[Conn],"closed":[Conn]}` |
| `connections_close` | W | `id` (`0` = all) | `{"closed":n}` |
| `logs` | W | `epoch?`, `after?` | `{"epoch","next","lines":[LogLine]}` |

```jsonc
// Status
{
  "enabled": true, "state": "running",   // "off" | "starting" | "running" | "error"
  "error": null,                          // why it is not running, for people
  "routing": "rule", "config": "default",
  "node": {"id": "s1-3", "name": "香港 01"}, // or null
  "since": 1763280000,                    // unix seconds or null
  "up": 0, "down": 0,                     // bytes since start (engine counters)
  "mem": 0, "xray": "26.10.29", "engine": "0.1.0",
  "intercept_router": true,
  "groups": [GroupStatus],
  "health": null                          // the engine's Health passthrough or null
}
// GroupStatus {"name","type":"select|urltest|fallback","selected","nodes":[{"name","delay_ms":null|n,"alive"}]}
// Conn {"id","start","end","network","host","ip","port","protocol","inbound","outbound","group","up","down","source"}
// LogLine {"seq","time","run","level","text"}
```

## 3. Web: f50-web `/proxy` pages

A new sidebar group 代理 / Proxy between 连接 and 系统, all pages in `src/views/proxy/`, i18n `proxy` namespace (zh and
en), API in `src/views/proxy/api.ts` + `queries.ts` over `f50()` (`f50.proxy`), types in `types.ts` matching §2.

- `/proxy` 代理 — the box: on/off switch with state, node, uptime, speed (from `status` deltas), 分流 mode
  (智能分流 / 全局代理 / 全部直连 segmented control), current config, 连通性测试 (probe the selected node); then
  策略组 (each group: picker, delays, 测速) and 节点 (subscriptions as groups with update/edit/delete, usage and expiry;
  本地节点; per-node latency; select by click; 添加 opens the node editor).
- `/proxy/node?id=` 节点 — add/edit: 类型 (订阅 · VLESS · Hysteria2 · Shadowsocks · Trojan · VMess · SOCKS5 · HTTP),
  paste links, the form fields of one-node's `node/form.ts` (build/parse share links), 前置代理 picker.
- `/proxy/rules` 分流 — configs list (默认 read-only, user configs, 使用中, add from URL), and the editor:
  策略组 (手动选择 / 自动选择 / 故障转移, nodes, regex filter, interval), 应用分流 packs (one-node's `app-packs.ts`),
  custom rules (完整域名 / 域名后缀 / IP 网段 / … + outbound; text mode in one-node's Clash-style syntax), 兜底出口,
  JSON view, 检查.
- `/proxy/data` 连接 — 流量 (rates, totals, 60 s chart), 连接 (client source, host, route bar filter, search, sort,
  close one/all), 日志 (level filter, search, pause, copy). Poll every 1 s while visible.
- `/proxy/settings` 设置 — 代理本机流量, 不走代理的设备 (pick from the LAN clients the console already lists, or a
  MAC), 测速地址, 订阅自动更新, 规则集地址, 自动切换节点, 日志级别.

## Daemon notes (f50d crate `f50-proxy`)

- Routing table 117 and preference 1170 are unused elsewhere in `f50d/` and `systems/openwrt/overlay` (checked): kept.
- `Status.engine` is read from the engine's `/v1/status` field `version` (else `engine`); onecore has neither, so
  it is `null` until the engine adds one.
- `log_level`: the engine takes `--loglevel` on its command line, so f50d writes `/etc/f50d/proxy/log_level` and the
  init script passes it (default `warning`); a change restarts the service and the supervisor starts it again.
- The init script skips the start while `/etc/f50d/proxy/token` is missing; f50d's supervisor runs
  `/etc/init.d/f50-proxy start` after 3 ticks without an answer (at most every 5 min). While the engine does not answer,
  f50d removes the nft table and the policy routing at once (nothing may be sent to a tproxy port nobody listens on).
- `install.sh` installs the engine from `F50_PROXY_BIN` (env), the service always; `S20f50-proxy`/`K11f50-proxy`.
