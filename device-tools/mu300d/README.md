# mu300d

One JSON API for the Unisoc MU300 / ZTE F50 / U30 Air router (Ubuntu, OpenWrt, OpenWrt + LuCI), in Rust. It is
meant to replace both `mu300-api` (adb, Cockpit) and the `mu300dash` rpcd backend (LuCI). It has one method table,
one validation path, and one response shape.

v1 is the API layer only. Each method runs the same device tools in `/opt/mu300/bin`, with the same arguments, that
`mu300-api`'s `m_<name>` runs: `mobile-data`, `mu300-sim`, `mu300-sim-radio`, `mu300-sim-state`, `wifi-client`,
`mu300-wifi-band`, `mu300-update`, `mu300-at`, `sms` / `mu300-sms`, `mu300-usb`, `mu300-cpu`, `mu300-traffic`, and
the control panel's `lock`, `cell` and `device-usb` adapters. Where `mu300-api` worked inline (hotspot.conf,
mobile-data.conf, lan.conf, UCI, systemd units), the same steps are written here in Rust, and `uci batch` and
`systemctl` still do the work.

## Contract

**Request.** A request is one JSON object of at most 64 KiB. An empty body means `{}`.

- Every method declares its fields. A field the method does not declare is refused with `unknown-field`.
  `ubus_rpc_session` is the one exception: OpenWrt's ubus adds it to every call, so it is ignored.
- `null` counts as a missing field.
- Each field has a type:
  - a string matched against an anchored regex, with a maximum length in bytes;
  - one line of UTF-8 text with no control characters;
  - an integer in a range (a JSON number, or a string of digits);
  - a boolean (`true`/`false` or `1`/`0`);
  - an enum.
- A value that is not a secret may not start with `-`, because it may reach a tool's command line.
- Secrets never go on a command line. They go to the tool's stdin, or to a 0600 job file the runner opens and then
  removes. The secrets are: the Wi-Fi and APN passwords, the login password, the SMS number and text, and AT
  commands.
- Secrets are never logged, never echoed back, and never written to the job log. `password_set` reports whether a
  password exists.

**Reply.** Every method answers with this envelope:

```json
{"ok": true,  "data": { ... }}
{"ok": false, "code": "bad-value", "error": "ssid: not an accepted value", "field": "ssid"}
```

`error` is sanitised: control characters are removed, the path and query of any URL become `...`, and it is at most
200 bytes. When a tool fails, `error` is that tool's last message.

| code | meaning | HTTP |
|---|---|---|
| `bad-request` | the body is not one JSON object, is too large, or has the wrong content type | 400 |
| `unknown-method` | no such method | 404 |
| `unknown-field` | the request has a field the method does not take (`field` names it) | 400 |
| `bad-value` | wrong type or a value outside the rule (`field`) | 400 |
| `missing` | a required field is missing (`field`); also an AT command that needs `confirm` | 400 |
| `unauthorized` | no session (HTTP), or not root (`call`) for a write method | 401 |
| `forbidden` | refused on purpose: a wedging or SIM-moving AT command, login throttling, an uncancellable job | 403 |
| `unsupported` | this device or system lacks the feature, or the tool is not installed | 501 |
| `busy` | a job of the same kind is running, the AT channel is in use, or the modem is restarting | 409 |
| `timeout` | the tool did not finish in time (its process group was killed) | 504 |
| `failed` | the tool ran and failed | 500 |
| `not-found` | no such job | 404 |

**Jobs.** Work that can take longer than about 15 s runs as a job. The reply is `{"ok":true,"data":{"job":"<id>"}}`.
`job.status {id}` returns:

```
{id, method, kind, state: running|done|failed|cancelled, started, finished?, cancellable, log_tail, result?: {rc, text}, error?}
```

- A job is a directory, `RUN/jobs/<id>` (0700), plus a detached runner, `mu300d job-run DIR`. The runner runs in its
  own session and is reparented to init through a double fork, which is what `mu300-api`'s `job_start` gets from
  `setsid`. A job therefore outlives the request, the HTTP connection, and a restart of the daemon. If a runner dies,
  for example on a reboot, its job reports `failed`, "interrupted".
- The runner starts the tool in its own process group and enforces the job's timeout: TERM to the group, then KILL
  5 s later.
- The job log is capped at 1 MiB; the runner keeps the last 256 KiB once it grows past that.
- `job.cancel` writes a cancel mark and sends TERM to the tool's group. `update.apply`, `lock.set`, `radio.set`,
  `sim.select` and `sms.send` cannot be cancelled, because stopping them half way leaves the device in an unknown
  state.
- Only one job of a kind runs at a time; starting a second one returns `busy`. A lock file makes the
  check-then-start atomic across processes. The kinds are:
  - `modem`: radio.set, sim.select, lock.set (all three restart the protocol stack)
  - `wifi`
  - `update`
  - `sms`
  - `at`
- The last 20 finished jobs are kept.

**Radio readings never block on the modem.** `signal.get`, `cell.get`, and the signal parts of `status.get` and
`data.get` come from a cache, and each reply carries `age_ms`.

- The cache is the collector's own files.
- When a reading is stale, the read starts one detached refresh and returns at once with `refreshing: true`.
- A stamp file rate-limits refreshes, so any number of polling clients costs the modem at most one round per
  interval (2 s for the fast tier, 8 s for the full tier).
- No refresh starts while the modem is restarting (`/run/unisoc-modem-lock-apply`, `/run/mu300/sim-switching`,
  `/run/mu300/sim-active-unknown`). Replies then carry `modem_busy: true`.
- This works the same for `call` (one process per request) and `serve`.

The collector sits behind the `cache::Collector` trait. **What v1 uses:**

- **`unisoc-modem/cell`** (openwrt-luci) is the control panel's collector. `cell fast` writes `sig.json` (the serving
  cell, refreshed when older than 3 s). `cell full` writes `cell.json` (neighbours, QoS, operator; refreshed when
  older than 10 s). Both files go in the adapters' runtime directory, which LuCI's dashboard shares.
- **`mu300d collect-at`** is used where that collector is not installed (Ubuntu, plain OpenWrt). It sends
  AT+COPS?, AT+CEREG? and AT+CESQ through `mu300-at`, mirroring mu300-api's `signal_json`, and writes
  `RUN/radio/sig.json`, refreshed when older than 15 s. It produces no cell table, so `cell.get` returns
  `data: null`.

v1 does not parse AT itself beyond those three replies: the existing collector already knows the Unisoc
engineering-mode output, and reusing it keeps one implementation.

**Subprocesses.** Every tool runs through `exec::Tool`:

- in its own process group, with a hard timeout that kills the whole group;
- with stdout and stderr read up to 256 KiB each, and the rest drained;
- with a sanitised error message.

The group is also killed when the request goes away. `later` (a restart two seconds after the reply) and the
detached refreshers run in a new session.

## Surfaces

- `mu300d call METHOD < REQUEST` is a drop-in for `mu300-api METHOD < REQUEST` over adb: it prints one reply and
  exits 0 (ok), 1 (refused or failed) or 2 (usage). Write methods need root, as mu300-api's callers are.
  `MU300D_NOROOT=1` lifts that for tests.
- `mu300d methods` prints the method names, one per line. The `methods` method returns the whole table: access and
  fields.
- `mu300d serve [--listen ADDR:PORT]... [--lan] [--open-reads]` serves the API over HTTP. The default address is
  `127.0.0.1:8300`; `--lan` adds the LAN address on the same port. Requests are `POST /api/<method>`.
  - **Content type.** `Content-Type: application/json` is required. A cross-site form cannot send it, and a script
    from another origin would need a CORS preflight. No CORS headers are sent at all.
  - **Login.** `auth.login {password}` checks the device password: the shadow hash of the account `system.password`
    sets (`root` on OpenWrt, `ubuntu` on Ubuntu) is passed to `mu300-pwcheck`, which reads the password and the hash
    on stdin and prints MATCH/NOMATCH (`device-tools/pwcheck`).
  - **Sessions.** A successful login returns a random 256-bit token as an `HttpOnly; SameSite=Strict` cookie, and in
    `data.token` for `Authorization: Bearer`. Sessions live in memory, expire after 30 min idle, and are capped at 32
    (the oldest goes). `auth.logout` ends one.
  - **Login throttling.** One login runs at a time. A wrong password costs 1 s, and after 5 failures in a minute
    every login is `forbidden`.
  - **Authorization.** Write methods need a session. Read methods do too, unless `--open-reads` is given: reads
    include SMS, the AT history, and APN users.
  - **Limits.** The body limit is 64 KiB. At most 8 methods run at once; a request waits up to 5 s for a slot, then
    gets `busy`.

## Configuration

Configuration comes from the environment (and the `serve` flags above). Unset variables mean the device's own paths.

| variable | default | |
|---|---|---|
| `MU300_BIN` | `/opt/mu300/bin` | the device tools |
| `MU300_SYSROOT` | (empty) | prefix for every system path (`/etc`, `/proc`, `/sys`, `/run` markers): tests |
| `MU300D_RUN` | `/run/mu300d` | jobs, the AT collector's cache (must be this user's own, 0700) |
| `MU300D_PATH` | `/usr/sbin:/usr/bin:/sbin:/bin:$MU300_BIN:/opt/mu300/busybox-bin` | PATH of every child |
| `MU300_UNISOC_DIR` | `$SYSROOT/usr/libexec/unisoc-modem` | the panel's adapters |
| `MU300_DASH_DIR` | `$SYSROOT/var/run/unisoc-modem` | their runtime directory (sig.json, cell.json, lock.json, at-history) |
| `MU300_CELL_BIN`, `MU300_LOCK_BIN` | `$MU300_UNISOC_DIR/cell`, `/lock` | the collector and the lock adapter |
| `MU300D_PWCHECK` | `$MU300_BIN/mu300-pwcheck` | the password checker |
| `MU300_DISK` | `/mnt/mu300-disk` | shown in status.get's storage |
| `MU300D_LISTEN` | `127.0.0.1:8300` | comma-separated listen addresses |
| `MU300D_OPEN_READS` | off | read methods without a session |
| `MU300D_SESSION_IDLE` | 1800 | session idle expiry, seconds (at least 60) |
| `MU300D_SELF` | the running binary | what `job-run` / `collect-at` are started as |

Running `serve` under systemd needs `KillMode=process` (or `mixed`). Otherwise stopping the unit kills the job
runners in its cgroup, and an update in progress with them. procd only signals the main process.

## Methods

Access: **P** public, **R** read (a session over HTTP unless `--open-reads`), **W** write (a session over HTTP, root
for `call`). Every reply's `data` is shown; `{}` means an empty object.

### System

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `hello` | P | – | `api` (2), `server`, `version`, `system`, `device`, `os`, `release`, `methods`, `features` | `mu300-device`, os-release |
| `methods` | P | – | `methods: [{name, access, fields:[{name,required,secret}]}]` | the registry |
| `auth.login` | P (HTTP) | `password`* (secret, ≤128 B) | `token`, `idle_s` | shadow + `mu300-pwcheck` (stdin) |
| `auth.logout` | P (HTTP) | – | `{}` | – |
| `system.get` | R | – | `hostname`, `timezone`, `user`, `ssh`, `time`, `release`, `kernel`, `system`, `os` | /proc, UCI or /etc/localtime, `systemctl is-enabled` |
| `system.reboot` | W | – | `{}` | `later sh -c 'sync; reboot'` |
| `system.hostname` | W | `name`* `[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?` | `{}` | `hostnamectl set-hostname` / UCI + `/etc/init.d/system reload` |
| `system.timezone` | W | `zone`* `UTC\|Area/City` (no `..`) | `{}` | `timedatectl set-timezone` / UCI zonename + TZ |
| `system.password` | W | `password`* (secret, one line, ≤128 B) | `user` | `chpasswd` (or `busybox chpasswd -c sha512`), `user:pw` on stdin |

### Status and radio

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `status.get` | R | – | `system`, `os`, `device`, `release`, `kernel`, `hostname`, `uptime`, `now`, `memory`, `storage{root,disk}`, `signal` (cached reading), `data{state,iface,address}`, `traffic`, `battery`, `temperatures`, `hotspot{ssid,enabled}`, `lan` | /proc, /sys, `df`, `ip`, `mu300-traffic json`, cache |
| `signal.get` | R | – | `{data, age_ms, refreshing, modem_busy, collector}` | cache (sig.json) |
| `cell.get` | R | – | same shape (cell.json; `data: null` with the AT collector) | cache |
| `radio.get` | R | – | `radio` | `mu300-sim-radio status --json` |
| `radio.set` | W, **job** `modem` | `slot`* `1\|2\|all`, `state`* `on\|off` | `job` | `mu300-sim-radio set 0\|1\|all on\|off` |
| `modem.restart` | W | – | `{}` | `later systemctl restart mu300-mobile-data.service` / `ifdown wan; ifup wan` |

### Mobile data and SIM

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `data.get` | R | – | `state`, `iface`, `address`, `apn`, `pdp`, `auth`, `user`, `password_set`, `signal`, `sim`, `roaming`, `roaming_switch` | mobile-data.conf / UCI, `mu300-sim status --json`, `mobile-data roaming --json` |
| `data.set` | W | `apn`, `pdp` `IP\|IPV6\|IPV4V6`, `auth` `none\|pap\|chap\|both`, `user`, `password` (secret), `clear_credentials`, `roaming` | `restart` | mobile-data.conf (0600) + `later systemctl restart` / UCI batch + `later ifup wan`; `mobile-data roaming on\|off` |
| `data.power` | W | `state`* | `{}` | `later systemctl start\|stop` / `ifup\|ifdown wan` |
| `sim.get` | R | – | `sim` | `mu300-sim status --json` |
| `sim.select` | W, **job** `modem` | `slot`* 1-2 | `job` | `mu300-sim default N` |
| `sim.state` | R | `fresh` | `state` (ready/locked/absent/null), `slot` | `mu300-sim-state get\|check`, `mu300-sim active` |
| `lock.get` | R | `fresh` | `lock`, `refreshing` | `lock get cached`; a missing cache or `fresh` starts `lock refresh` detached |
| `lock.set` | W, **job** `modem` | `kind`* `mode\|endc\|lte\|nr\|cell\|reset\|auto_apply`, `val` (rule by kind) | `job` | `lock apply KIND VAL` / `lock reset` |
| `traffic.get` | R | – | `traffic` | `mu300-traffic json` |
| `traffic.set` | W | `key`* `reset_day\|cap\|warn\|cut\|used\|clear\|daily\|daily_cut\|count\|plan`, `value` (rule by key) | `{}` | `mu300-traffic set …` / `used N` / `clear --yes` |

### SMS

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `sms.list` | R | `page` 1-9999 | cli: `{backend, messages}`; pool: `{backend, total, unread, page, pages, msgs, sim_storage}` | `sms json` / `mu300-sms list PAGE` |
| `sms.read` | R | `id`* `[0-9]{1,6}` | `id`, `text` | `sms read ID` / `mu300-sms show ID` |
| `sms.send` | W, **job** `sms` | `number`* `\+?[0-9]{1,20}`, `text`* (secret, ≤480 chars, newlines ok) | `job` | `sms send --stdin` (number and text on stdin) / `mu300-sms send --stdin NUMBER` (text on stdin) |
| `sms.delete` | W | `id`* `all\|[0-9]{1,6}`, `sim` | `{}` | `sms delete ID` / `mu300-sms delete ID [--sim]` |

### Network

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `wifi.get` | R | – | `enabled`, `client_mode`, `ssid`, `band`, `channel`, `channel_live`, `country`, `security`, `hidden`, `password_set`, `channels{2.4,5}`, `clients` | hotspot.conf / UCI, `iw`, DHCP leases |
| `wifi.set` | W | `ssid`, `password` (secret, 8-63 ASCII), `band` `2.4\|5`, `channel` `auto\|N` (allowed for the band and country), `hidden` | `restart` | hotspot.conf + `later systemctl restart mu300-hotspot` / UCI batch (+ WPA3→WPA2 fallback), `mu300-wifi-band`, `later wifi reload` |
| `wifi.power` | W | `state`* | `{}` | `systemctl enable\|disable` + `later start\|stop` / UCI + `later wifi up\|down` |
| `wificlient.status` | R | – | `active`, `text` | `wifi-client status` |
| `wificlient.scan` | W, **job** `wifi` | – | `job` (result text is the list) | `wifi-client scan` |
| `wificlient.join` | W, **job** `wifi` | `ssid`*, `password` (secret: passphrase or 64 hex), `open` | `job` | `wifi-client connect SSID -` (password on stdin) / `--open` |
| `wificlient.leave` | W, **job** `wifi` | `forget` | `job` | `wifi-client disconnect\|forget` |
| `lan.get` | R | – | `ip`, `default_ip`, `usb_role`, `usb_role_switch` | lan.conf / UCI, `mu300-lan-ip`, `mu300-usb` |
| `lan.set` | W | `ip`* (a private /24 ending in .1) | `restart` | lan.conf + `later systemctl restart mu300-lan` / UCI + `later network reload` |
| `usb.role` | W | `role`* `host\|device` | `{}` | `later mu300-usb ROLE` (U30 Air only) |
| `usb.net` | W | `mode` `ncm\|ecm\|rndis\|multi\|default`, `scope` `once\|permanent`, `name` `usb\|eth` | `mode`, `once`, `name`, `now` | `mu300-usb net …` / `device-usb set-net …` (openwrt-luci) |

### CPU and update

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `cpu.get` | R | – | `cpu`, `voltage` | `mu300-cpu status --json`, `mu300-cpu voltage status --json` |
| `cpu.profile` | W | `name`* `saving\|balanced\|performance` | `{}` | `mu300-cpu profile NAME` |
| `update.status` | R | – | `status`, `source`, `version` | `mu300-update status\|source\|version` |
| `update.check` | W, **job** `update` | – | `job` | `mu300-update check` |
| `update.apply` | W, **job** `update`, not cancellable | – | `job` | `mu300-update apply` |

### Debug

| method | access | fields | data | tool / source |
|---|---|---|---|---|
| `at.send` | W | `command`* (secret, ≤512 B), `timeout` 1-60, `confirm` | `rc`, `text`, `class`; a scan, or `timeout` > 8, gives `job` (kind `at`) | `mu300-at -t T -` (command on stdin) |
| `at.history` | R | – | `history: [{time, command}]`, newest first, redacted | the panel's `at-history` file |

`at.send` runs these checks before anything is sent. They are ported from mu300dash as pure functions in
`src/at_guard.rs`, with unit tests:

1. The command must be one line of printable ASCII, start with `AT`, and be at most 512 bytes. Otherwise:
   `bad-value`.
2. No `;` chaining: `forbidden`.
3. `AT+SPENGMD=0,1…` wedges the AT port until a reboot. It is refused in any spelling: `forbidden`.
4. AT+SPACTCARD= / AT+SMMSWAP= / AT+SPSWDATA move the SIM selection or the data route: `forbidden`.
5. The modem is restarting, or an AT job is running: `busy`.
6. Commands of the classes `scan`, `nv`, `radio`, `apn` and `sim` need `confirm: true`. Without it the reply is
   `missing`, with field `confirm` and the class named in the error.

A `scan` runs as a job, `mu300-at -t 99`. The history keeps the last 40 commands. Arguments of CPIN, CPWD, CLCK,
CGAUTH and $QCPDPP are redacted both when written and when read: mu300dash used to store them in clear.

### Jobs

| method | access | fields | data |
|---|---|---|---|
| `job.status` | R | `id`* | see Contract |
| `job.cancel` | W | `id`* | `id`, `cancelling` |
| `job.list` | R | – | `jobs: [{id, method, kind, started, state}]` |

### Dropped from mu300-api

The rest of mu300-api is not carried over:

- VPN, backup, extras, packages, docker
- zram, LED, power, TCP, TTL
- USSD, voice
- kernel and boot-OS switching
- SMS forwarding, `sms.job`, `adb.auth`, `system.ssh`, `logs`, `job.clear`, `sim.card`, `update.source`

## Deviations from mu300-api

- **Response shape.** Values are under `data`. `ok` is a boolean, the error code is in `code`, and the human text is
  in `error`.
- **More jobs.** `radio.set` (up to 150 s), `sim.select` (60 s), `wificlient.scan` (40 s) and `update.check` now run
  as jobs, along with the methods the brief named. Each can take longer than 15 s.
- **Job ids.** A job has an id (`<method>-<epoch>-<hex>`), not a single global slot. One job per *kind*, not one
  job overall.
- **`lock.get`** no longer waits up to 12 s for a fresh reading. It returns the cache plus `refreshing`.
- **`lock.set`** reports its outcome through `job.status`, instead of "started"/"queued".
- **`at.send`** gains mu300dash's guards, the `confirm` field, `busy`/`timeout` detection, and scan jobs. Its sync
  wait is at most 8 s.
- **SMS on openwrt-luci** uses `mu300-sms` directly, not through `mu300dash`. The list's shape depends on the
  backend (see open questions).
- **No leading `-` in non-secret strings.** No string that is not a secret may start with `-`. mu300-api only
  checked this for values that reach argv; it now also applies to hotspot SSIDs and APN user names.
- **`job.list` is new.** A frontend needs it to find a running job again after a reload.
- **`hello` is slimmer.** `dual_sim` and `forward` are gone from `features` (no subprocess in `hello` but
  `mu300-device`). `features.radio_collector` is new.
- **`lan.get`** no longer reports adb's state (adb.auth is dropped).

## Build and test

```sh
cargo test                       # unit tests + integration tests with shell-stub tools (tests/common)
cargo clippy --all-targets -- -D warnings
cargo fmt --check

rustup target add aarch64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl   # .cargo/config.toml links with rust-lld
# -> target/aarch64-unknown-linux-musl/release/mu300d: static, stripped, about 1.4 MiB
```

`tests/http.rs` binds a localhost port. Sandboxes that forbid local binding fail it with "Operation not permitted".
The integration tests build a fake BIN, PATH and system root per test, and record each tool's argv and stdin. They
check:

- validation refusals;
- argv and stdin shapes, and secrets kept off argv;
- tool failures and timeouts (the process group is killed);
- the job lifecycle: detached runs, busy, cancel, failure messages, uncancellable jobs;
- the radio cache and its rate limit;
- the AT guards and the history;
- the HTTP sessions.

**Dependencies:**

- `tokio` (current-thread runtime) and `axum` (http1 only): mainstream, and async subprocesses with timeouts plus
  the HTTP server share one small runtime. The release binary is about 1.4 MiB.
- `serde` / `serde_json`: JSON.
- `regex` (Unicode tables trimmed): the anchored field rules and the AT guards.
- `libc`: process groups, setsid, flock, geteuid.

There is no `thiserror`/`anyhow`: errors are the closed `ApiError` type.

## Left for later

- **A native AT daemon** to replace `mu300-at` / `mu300-atd`: a unix socket, real locking, no 10 ms `sleep` polling.
  The `Collector` trait is where a native collector plugs in.
- **Native collectors** for signal, cell, locks and SIM identity, so the shell `cell`/`lock` adapters are no longer
  needed.
- **Porting the device commands** (`mobile-data`, `mu300-sim*`, `wifi-client`, `sms`, `mu300-usb`, `mu300-cpu`,
  `mu300-traffic`, `mu300-update`) into this daemon, one area at a time, behind the same methods.
- **An rpcd/ubus shim** so LuCI can call this API, a systemd unit and a procd init script, and packaging.
- **A unified SMS model** for both backends.
