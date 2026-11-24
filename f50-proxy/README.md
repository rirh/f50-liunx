# f50-proxy

The proxy engine (代理与分流) of the ZTE F50 on OpenWrt is **One Node's client core** — `one-node/app/client`
(`/Users/zh/Documents/workspaces/voh/one/one-node`), the same Go program the One Node PC and phone apps run: Xray-core
as a library, routed by One Node's own rule engine (sing-box rule semantics). This directory only builds it for the
router; f50d's `f50-proxy` crate drives it over a token-protected loopback API. The contract between engine, f50d and
the web pages is [DESIGN.md](DESIGN.md); §1 is the engine.

## Build

```sh
make device    # dist/f50-proxy-linux-arm64: CGO_ENABLED=0 GOOS=linux GOARCH=arm64, -trimpath -s -w (~30 MB)
make build     # host binary dist/f50-proxy, for `check` and trying the API locally
make test      # the client core's go vet + go test ./... (the tests bind loopback ports)
make device VERSION=0.2.0             # stamps the release (default 0.1.0)
make device ONE_NODE=/path/to/one-node   # where the one-node checkout is
```

The build stamps three things into the client core:

| ldflag | value | what it does |
|---|---|---|
| `main.appVersion` | `$(VERSION)` | the release: `status.version`, the UA's version |
| `internal/app.Product` | `F50Proxy` | the UA's product token, `F50Proxy/<ver> <arch>`; One Node's gateway calls the client `One Node F50` (policy id `one-node-f50`) |
| `internal/device.Model` | `ZTE F50` | `X-Device-Model` |

On OpenWrt the core tells subscription gateways `X-Device-OS: OpenWrt`, `X-Ver-OS` (`DISTRIB_RELEASE`) and an
`X-HWID` hashed from `/etc/machine-id`, else the eMMC CID (`/sys/block/mmcblk0/device/cid`).

What the router needs from the core — `mode: "gateway"` (tproxy inbound, SO_MARK on every socket, bootstrap DNS),
`Conn.source`, the rule-set disk cache, the probe cap, the raw ICMP fallback, the info/trouble log split — lives in
one-node (`internal/xrayconf`, `internal/netmark`, `internal/engine`, …); DESIGN.md §1 lists it. Change the engine
there, not here.

## On the device

Installed as `/opt/f50/bin/f50-proxy`, run always by procd (`f50d/openwrt/init.d/f50-proxy`, respawn):

```sh
f50-proxy serve --listen 127.0.0.1:17891 --token-file /etc/f50d/proxy/token \
  --log /var/log/f50-proxy/core.log --loglevel warning --info-stdout \
  --cache-dir /etc/f50d/proxy/rulesets --probe-limit 3 --link-key /etc/f50d/proxy/link.key \
  --local-socks-listen 0.0.0.0 --local-socks-mark 255
```

`--local-socks-*`: the nodes' 本地 SOCKS5 ports (a node's own SOCKS5 port, turned on in the node editor) listen on
every address, so LAN and USB devices can use them as their proxy (fw4 rejects input from the cellular WAN), and
their node connections carry the gateway mark so the interception of the router's own traffic never catches them.

`--link-key` is the router's OnLink device key (P-256, created at the first start, 0600). The engine sends its
public half with subscription downloads (`X-Link-Key`); the gateway binds it to this device at the first download and
then serves `onlink://` lines (VLESS without a user, TCP + REALITY/TLS). The binding is for good: a lost key
means the provider has to remove the device before a new one is accepted, so `/etc/f50d` survives updates.

f50d writes the token file (0600) before the service starts. The engine is idle until f50d sends `/v1/start` with
`mode: "gateway"`; f50d then installs the nftables table `inet f50_proxy` and the fwmark-1 policy route that bring
LAN (and optionally the router's own) traffic to the tproxy port, and removes them before `/v1/stop`. The engine
needs root (`CAP_NET_ADMIN` for `IP_TRANSPARENT` and `SO_MARK`). `--log` writes `core-<date>.log`, seven days kept.

`f50-proxy check --links file --mode proxy` starts every link of a file in turn and fetches through the mixed port;
`f50-proxy testserver` runs a loopback REALITY node; `f50-proxy version` prints the product token and Xray's version.
