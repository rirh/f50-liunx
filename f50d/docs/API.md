# f50d API reference (extracted from the Rust method tables)

The machine-readable version is the OpenAPI 3.1 document [`api/openapi.json`](api/openapi.json) (`f50d openapi`; Swagger UI
in the web console at `/f50/api-docs/`).

Every reply is `{"ok":true,"data":...}` or `{"ok":false,"code":...,"error":...,"field":...}`. Methods marked *job* return `{"job":"<id>"}`; poll `f50.job status {id}`.

## `f50.at`

source: `crates/f50-at/src/terminal.rs`

- **send** (write) — Sends one guarded AT command on the control channel; scans and long commands run as a job.
  - fields: `Field::secret("command", Rule::Text { max_bytes: guard::AT_MAX, max_chars: guard::AT_MAX }, true), Field::opt("confirm", Rule::Bool), Field::opt("timeout", Rule::Int { min: 1, max: 60 }),`
- **history** (write) — The last 40 AT commands of this boot, PINs and passwords cut off.

## `f50.boot`

source: `crates/f50-hw/src/object.rs`

- **get** (read) — The slot Linux runs from, the default boot, the failed boots allowed before Android, whether Linux is \
             locked, the boot control block, and the last result of every boot stage.
- **android** (write) — Makes the next boot Android's (the initramfs' Android block, written and read back) and reboots in two \
             seconds. The device stays in Android until Linux is started from there.

## `f50.cpu`

source: `crates/f50-system/src/objects.rs`

- **get** (read) — The CPU profile, the policies and the thermal zones.
- **profile** (write) — Applies and saves a CPU profile.
  - fields: `Field::req("name", Rule::Enum(crate::cpu::PROFILES))`

## `f50.daemon`

source: `crates/f50d/src/internal.rs`

- **hotplug_iface** (write) — netifd's iface hotplug (ACTION, INTERFACE).
  - fields: `Field::req("action", Rule::Word { re: "[a-z]{1,16}", max: 16 }), Field::req("interface", Rule::Word { re: "[A-Za-z0-9_.:@][A-Za-z0-9_.:@-]{0,31}", max: 32 }),`
- **hotplug_net** (write) — The kernel's net hotplug (ACTION, INTERFACE).
  - fields: `Field::req("action", Rule::Word { re: "[a-z]{1,16}", max: 16 }), Field::req("interface", Rule::Word { re: "[A-Za-z0-9_.:@][A-Za-z0-9_.:@-]{0,31}", max: 32 }),`
- **clock_synced** (write) — NTP set the clock: data usage may trust it.
- **acl** (read) — The rpcd ACL generated from the method table.

## `f50.data`

source: `crates/f50-data/src/objects.rs`

- **get** (read) — The mobile data connection and its settings.
- **set** (write) — The APN settings and data roaming of the SIM in use.
  - fields: `Field::opt("apn", Rule::Text { max_bytes: 100, max_chars: 100 }), Field::opt("auth", Rule::Enum(&["none", "pap", "chap"])), Field::opt("username", Rule::Text { max_bytes: 64, max_chars: 64 }), Field::secret("password", Rule::Text { max_bytes: 64, max_chars: 64 }, false), Field::opt("pdp", Rule::Enum(&["ipv4", "ipv6", "ipv4v6"])), Field::opt("roaming", Rule::Bool),`
- **power** (write) — Mobile data on or off.
  - fields: `Field::req("state", Rule::Enum(&["on", "off"]))`

## `f50.job`

source: `crates/f50-core/src/jobs.rs`

- **status** (read) — A job's state, result or error, and the tail of its log.
  - fields: `Field::req("id", Rule::Word { re: ID_RE, max: 48 })`
- **list** (read) — Every job kept, oldest first.
- **cancel** (write) — Stops a running job that may be stopped.
  - fields: `Field::req("id", Rule::Word { re: ID_RE, max: 48 })`

## `f50.lan`

source: `crates/f50-net/src/objects.rs`

- **get** (read) — The LAN: address, netmask, DHCP range, the USB host's address.
- **set** (write) — Changes the LAN address (a private /24's .1), netmask or DHCP range.
  - fields: `Field::opt("address", lan::ADDRESS), Field::opt("netmask", Rule::Word { re: "[0-9.]{7,15}", max: 15 }), Field::opt("dhcp", Rule::Bool), Field::opt("dhcp_start", Rule::Int { min: 2, max: 254 }), Field::opt("dhcp_limit", Rule::Int { min: 1, max: 253 }), Field::opt("leasetime", Rule::Word { re: "[0-9a-z]{1,9}", max: 9 }),`

## `f50.led`

source: `crates/f50-system/src/objects.rs`

- **get** (read) — The LED settings.
- **set** (write) — Changes one LED setting.
  - fields: `Field::req("key", Rule::Enum(crate::led::settings::KEYS)), Field::req("value", Rule::WordNum { re: "[a-z0-9]{1,8}", max: 8 }),`

## `f50.live`

source: `crates/f50-api/src/live.rs`

- **get** (read) — Every live topic now (the events a subscription to f50.live sends, by topic).
- **:subscribe** (read) — `GET /ubus/subscribe/f50.live` with `Authorization: Bearer <session>`: an event stream
  (`text/event-stream`). Each event's name is a topic, its data the envelope of the read method behind it, sent when it
  changed or at least every 15 s, and only while somebody is subscribed:
  `status` (`f50.status get`, 3 s), `system` (`f50.system status`, 2 s), `modem` (`f50.modem status`, 2 s),
  `cells` (`f50.modem cells`, 5 s), `sms` (`f50.sms unread`, 5 s), `proxy` (`f50.proxy status`, 2 s).

## `f50.lock`

source: `crates/f50-modem/src/objects/lock.rs`

- **get** (read) — The network mode, band, cell and EN-DC locks as last read, the replay switch and the last reset.
  - fields: `Field::opt("fresh", Rule::Bool)`
- **set** (write) — One lock change (mode, endc, lte, nr, cell; auto_apply at once), then one stack restart (a job).
  - fields: `Field::req("kind", Rule::Enum(Kind::WORDS)), Field::req("value", Rule::Word { re: "[a-z0-9:,_-]*", max: 127 })`
- **reset** (write) — Every lock back to automatic, verified (a job).

## `f50.meta`

source: `crates/f50-api/src/meta.rs`

- **describe** (read) — Every object and method, with access and field rules.

## `f50.modem`

source: `crates/f50-modem/src/objects/modem.rs`

- **status** (read) — Registration, RAT, operator, signal and bars from the latest reading, with its age.
- **cells** (read) — Serving cells, neighbours, carrier aggregation and QoS from the latest reading, with its age.
- **identity** (write) — IMEI, IMSI, ICCID, model, firmware and the SIM's own number.
- **radio** (write) — Switches the radio of the card in use on or off, kept across reboots (a job when it changes now).
  - fields: `Field::req("state", Rule::Enum(&["on", "off"]))`
- **own_number** (write) — Writes the SIM's own number to the card (its own-number phonebook, read by AT+CNUM; a job).
  - fields: `Field::req("number", Rule::Word { re: r"\+?[0-9]{3,15}", max: 16 })`
- **restart** (write) — Restarts the modem's protocol stack (a job).

## `f50.proto`

source: `crates/f50-data/src/proto/mod.rs`

- **setup** (write) — netifd's f50cell setup: starts the dial.
  - fields: `Field::req("config", Rule::Word { re: NAME_RE, max: 15 }), Field::opt("iface", Rule::Word { re: IFNAME_RE, max: 32 }),`
- **teardown** (write) — netifd's f50cell teardown.
  - fields: `Field::req("config", Rule::Word { re: NAME_RE, max: 15 }), Field::opt("iface", Rule::Word { re: IFNAME_RE, max: 32 }),`
- **renew** (write) — netifd's f50cell renew.
  - fields: `Field::req("config", Rule::Word { re: NAME_RE, max: 15 })`

## `f50.proxy`

source: `crates/f50-proxy/src/api.rs` (contract: `f50-proxy/DESIGN.md` §2)

Proxy and split routing through the engine `/opt/f50/bin/f50-proxy` (its own procd service, loopback API
`127.0.0.1:17891`). The profile is `<state>/proxy/profile.json` (0600: links are credentials); share links, subscription
URLs and pasted text are secret fields. Engine errors come back as `failed` with the engine's message.

- **status** (read) — The proxy's state, node, traffic counters and groups. Also the live topic `proxy` (2 s).
  - returns `{enabled, state ("off"|"starting"|"running"|"error"), error|null, routing, config, node: {id, name}|null, since|null, up, down, mem, xray|null, engine|null, intercept_router, groups: [GroupStatus], health|null}`; `GroupStatus` `{name, type, selected, nodes: [{name, delay_ms|null, alive}]}` (the engine's while running, else from the active config's last check).
- **profile** (write) — The whole profile: subscriptions, nodes, configs and settings.
  - returns `{enabled, routing, selected, subscriptions: [{id, name, url, title, usage, updated_at, error, nodes}], local_nodes, configs (configs[0] the built-in "default"), active_config, group_selections, settings}`.
- **set** (write) — Turns the proxy on or off, or changes its routing, node or config; applies at once.
  - returns what `status` does; a start that fails (engine, routes, nft: each undoes the ones before) is `failed`.
  - fields: `Field::opt("enabled", Rule::Bool), Field::opt("routing", Rule::Enum(Routing::WORDS)), Field::opt("selected", NODE_ID), Field::opt("active_config", CONFIG_ID),`
- **settings_set** (write) — Changes the proxy's settings; absent fields are kept.
  - returns `Profile.settings`; `bypass_macs` is a comma list (may be empty). A new `log_level` restarts the engine's service (it takes the level on its command line).
  - fields: `Field::opt("intercept_router", Rule::Bool), Field::opt("bypass_macs", Rule::Word { re: r"[0-9A-Fa-f:, -]{0,1600}", max: 1600 }), Field::opt("probe_url", URL), Field::opt("subscription_refresh_hours", Rule::Int { min: 0, max: 168 }), Field::opt("rule_sets_url", URL), Field::opt("auto_failover", Rule::Bool), Field::opt("log_level", Rule::Enum(LOG_LEVELS)),`
- **subscription_add** (write, job) — Adds a subscription and fetches its nodes as a job. Result `{"id", "nodes": n}`.
  - fields: `Field::secret("url", URL, true), Field::opt("name", NAME)`
- **subscription_update** (write, job) — Fetches one subscription or all again as a job. Result `{"updated": n, "failed": n}`; a failed one keeps its nodes and gets `error`. The selection and front nodes follow the nodes' names.
  - fields: `Field::req("id", Rule::Word { re: r"all|s[0-9]{1,6}", max: 8 })`
- **subscription_set** (write) — Renames a subscription or changes its address. Returns the profile.
  - fields: `Field::req("id", SUB_ID), Field::opt("name", NAME), Field::secret("url", URL, false)`
- **subscription_remove** (write) — Removes a subscription and its nodes. Returns the profile.
  - fields: `Field::req("id", SUB_ID)`
- **node_inspect** (write) — Reads share links without saving them: each node with its problem. Returns `{"nodes": [Node without id/via]}`.
  - fields: `Field::secret("text", Rule::Body { max_chars: 200_000 }, true)`
- **node_save** (write) — Adds a local node (no id) or replaces a node from its share link. Returns the `Node`; `via` "" clears the front node, a loop or a missing front node is refused (`field: via`).
  - fields: `Field::opt("id", NODE_ID), Field::secret("link", LINK, true), Field::opt("name", NAME), Field::opt("via", Rule::Word { re: r"|s[0-9]{1,6}-[0-9]{1,6}|L[0-9]{1,6}", max: 16 }),`
- **node_delete** (write) — Deletes a local node. Returns the profile.
  - fields: `Field::req("id", Rule::Word { re: r"L[0-9]{1,6}", max: 8 })`
- **probe** (write) — Times one request through a node (with its front nodes). Returns `{"ms": n}`; `failed` with the reason when it does not answer.
  - fields: `Field::req("id", NODE_ID), Field::opt("method", Rule::Enum(&["url", "tcp", "icmp"]))`
- **group_select** (write) — Points a select group at a node and remembers the pick. Returns `[GroupStatus]`.
  - fields: `Field::req("group", NODE_NAME), Field::req("node", NODE_NAME)`
- **group_test** (write) — Tests the delay of a group's nodes. Returns `[GroupStatus]`.
  - fields: `Field::req("group", NODE_NAME)`
- **config_save** (write) — Saves a rule config (no id: a new one) once its rules check. Returns the `Config`; `default` cannot be changed (`forbidden`).
  - fields: `Field::opt("id", CONFIG_ID), Field::req("name", NAME), Field::req("text", Rule::Body { max_chars: 256_000 }), Field::opt("url", Rule::Word { re: r"|https?://[!-~]{1,2040}", max: 2048 }),`
- **config_delete** (write) — Deletes a rule config; the active one falls back to the default. Returns the profile.
  - fields: `Field::req("id", CONFIG_ID)`
- **config_fetch** (write) — Downloads a rule config's text from its link. Returns `{"text"}`.
  - fields: `Field::req("url", URL)`
- **rules_check** (write) — Checks a route document: counts, groups and warnings, or the first error. Returns `{final, groups, source, rules, warnings}`.
  - fields: `Field::req("text", Rule::Body { max_chars: 256_000 })`
- **connections** (write) — The engine's open and recently closed connections. Returns `{epoch, active: [Conn], closed: [Conn]}`.
- **connections_close** (write) — Closes one connection, or all (id 0). Returns `{"closed": n}`.
  - fields: `Field::req("id", Rule::Int { min: 0, max: 9_007_199_254_740_991 })`
- **logs** (write) — The engine's log lines after a sequence number. Returns `{epoch, next, lines: [{seq, time, run, level, text}]}`.
  - fields: `Field::opt("epoch", Rule::Word { re: r"[0-9a-z]{0,32}", max: 32 }), Field::opt("after", Rule::Int { min: 0, max: 9_007_199_254_740_991 }),`
- `URL` is `Rule::Word { re: r"https?://[!-~]{1,2040}", max: 2048 }`, `NAME` `Rule::Text { max_bytes: 192, max_chars: 64 }`, `NODE_ID` `s<n>-<n>|L<n>`, `SUB_ID` `s<n>`, `CONFIG_ID` `default|c<n>`, `LINK` one line of up to 16384 characters.
- While on, the supervisor (every 5 s) starts the engine again when it is not running f50d's request, takes the interception down while the engine does not answer, refreshes subscriptions every `subscription_refresh_hours`, and (with `auto_failover`) moves off a node the engine reports down to the fastest other node of the same subscription.

## `f50.shell`

source: `crates/f50-system/src/objects.rs`, `crates/f50-system/src/shell/`

Root login shells on a pseudo-terminal for the web console's terminal page. Bytes cross as base64; `read` is a long
poll from an offset in the session's scrollback (256 KiB), so a reloaded page attaches again. A session nobody reads for
15 minutes is hung up; every session is hung up when f50d stops. Only `f50-admin` is granted these methods.

- **open** (write) — Starts a root login shell on a new terminal. Returns `{id, shell, pid}`.
  - fields: `Field::opt("cols", Rule::Int { min: 2, max: 1000 }), Field::opt("rows", Rule::Int { min: 2, max: 500 })`
- **read** (write) — A shell's output from an offset on (base64), waiting up to `wait` ms (at most 20000) for some.
  Returns `{data, offset, lost, ended, code}`; without `offset`, from the start of the scrollback.
  - fields: `id` (16 hex), `Field::opt("offset", Rule::Int { .. })`, `Field::opt("wait", Rule::Int { min: 0, max: 20000 })`
- **write** (write) — Types input (base64) into a shell. `data` is a secret field: never logged.
- **resize** (write) — Sets a shell's terminal size (`id`, `cols`, `rows`).
- **close** (write) — Hangs a shell up (`id`).
- **list** (write) — The open shells: `{sessions: [{id, shell, pid, started, idle, ended, code}], max}`.

## `f50.sim`

source: `crates/f50-modem/src/objects/sim.rs`

- **get** (read) — The card in use, the next boot's, the card's state and both cards' radio settings.
- **select** (write) — Chooses the card for the next boot (a job).
  - fields: `Field::req("slot", Rule::Int { min: 1, max: 2 })`
- **card** (write) — A card's radio on or off (or all: the persistent airplane mode), kept across reboots.
  - fields: `Field::req("slot", Rule::Enum(&["1", "2", "all"])), Field::req("radio", Rule::Enum(&["on", "off"]))`

## `f50.sms`

source: `crates/f50-sms/src/api.rs`

- **list** (write) — Conversations newest first with unread counts, or one conversation's messages (peer).
  - also returns `sim_storage` and `transport` (`{ims, cs, cs_stat, nas, ready, at}`, each `bool|null`; `ready: false` means the network currently gives the SIM no SMS path at all), as the last sync read them.
  - fields: `Field::opt("page", Rule::Int { min: 1, max: 100_000 }), Field::opt("per_page", Rule::Int { min: 1, max: 100 }), Field::opt("peer", Rule::Text { max_bytes: 64, max_chars: 32 }),`
- **read** (write) — One message whole; a received one is marked read.
  - fields: `Field::req("id", Rule::WordNum { re: ID_RE, max: 9 })`
- **send** (write) — Sends a message (up to 10 parts) as a job.
  - fields: `Field::req("number", Rule::Word { re: r"\+?[0-9]{1,20}", max: 21 }), Field::secret("text", Rule::Body { max_chars: 1530 }, true),`
- **delete** (write) — Deletes a message or all; with sim, the matching ones in the modem's store too.
  - fields: `Field::req("id", Rule::WordNum { re: "all|[1-9][0-9]{0,8}", max: 9 }), Field::opt("sim", Rule::Bool)`
- **sync** (write, job) — Restores SMS notifications and moves stored messages into the pool. Returns `{"job":"<id>","started":true}`; poll `f50.job status` for completion or failure.
- **unread** (read) — How many messages the pool holds, and how many are unread.
- **push** (write) — The Telegram push settings (never the token), the queue and the last outcome.
  - returns `{enabled, token_set, token_hint ("<bot id>:…<last four>"|null), chat_id, api, default_api, proxy, proxy_running, queued, last: {at, ok, id?, error?}|null}`.
- **push_set** (write) — Changes the Telegram push settings; absent fields are kept.
  - an empty `api` is the default (`https://api.telegram.org`); `proxy` (default `true`) sends through the proxy engine's local SOCKS5 inbound (`socks5h://127.0.0.1:17890`) while `f50.proxy` runs, directly otherwise; `enabled: true` needs a token and a chat id. Returns what `push` does. Kept in `<state>/sms/telegram.json` (0600).
  - fields: `Field::opt("enabled", Rule::Bool), Field::secret("token", TOKEN, false), Field::secret("chat_id", CHAT_ID, false), Field::opt("api", API), Field::opt("proxy", Rule::Bool)`
- **push_test** (write) — Sends a Telegram test message with the saved settings changed by the given fields (not saved).
  - returns `{"sent": true}`; a refusal (`failed`) carries Telegram's reason, the token redacted.
  - fields: `Field::secret("token", TOKEN, false), Field::secret("chat_id", CHAT_ID, false), Field::opt("api", API), Field::opt("proxy", Rule::Bool)`
- Every received message the pool takes is pushed (`sendMessage`, plain text: sender, time, hostname, text) while push is on. curl makes the request with the token and the body on its stdin (`-K -`); failures are retried after 5 s, 30 s, 2 min and 10 min, `429` after Telegram's `retry_after`, and `400/401/403/404` are not retried.

## `f50.status`

source: `crates/f50-api/src/status.rs`

- **get** (read) — The dashboard: every part's read method in one call.

## `f50.system`

source: `crates/f50-system/src/objects.rs`

- **info** (read) — The device's facts and live figures.
- **status** (read) — The home page's non-modem figures.
- **clients** (write) — The hotspot's clients and the DHCP leases.
- **set_hostname** (write) — Sets the hostname.
  - fields: `Field::req("name", Rule::Word { re: HOSTNAME_RE, max: 63 })`
- **set_timezone** (write) — Sets the time zone.
  - fields: `Field::req("zone", Rule::Word { re: ZONE_RE, max: 64 })`
- **set_password** (write) — Sets root's password.
  - fields: `Field::secret("password", Rule::Text { max_bytes: 128, max_chars: 128 }, true)`
- **reboot** (write) — Reboots in two seconds.
- **poweroff** (write) — Powers off in two seconds.
- **logs** (write) — The kernel's and the system log's last lines.
  - fields: `Field::opt("lines", Rule::Int { min: 10, max: 2000 })`

## `f50.traffic`

source: `crates/f50-data/src/objects.rs`

- **get** (read) — Data usage per day and billing cycle, and the cap.
- **set** (write) — A data usage setting, or clear the count.
  - fields: `Field::req("key", Rule::Enum(&["reset_day", "cap", "warn", "cut", "plan", "clear"])), Field::req("value", Rule::Text { max_bytes: 96, max_chars: 96 }),`

## `f50.update`

source: `crates/f50-update/src/object.rs`

- **status** (read) — What is installed, what is kept for a rollback, free space, the lock, the running job, the notice.
- **releases** (read) — The releases the last check found, and its age.
- **check** (write) — Asks for the newest release (a job).
- **apply** (write) — Installs the release the last check offered (a job; not cancellable).
  - fields: `Field::opt("tag", Rule::Word { re: TAG_RE, max: 16 }), Field::req("confirm", Rule::Enum(&["apply"]))`
- **rollback** (write) — Goes back to the kept system, or (what=boot) boot image (a job).
  - fields: `Field::req("confirm", Rule::Enum(&["rollback"])), Field::opt("what", Rule::Enum(&["system", "boot"]))`
- **upload_begin** (write) — Makes room for an offline bundle of that size and hash; the path to upload it to.
  - fields: `Field::req("size", Rule::Int { min: 1, max: 2047 << 20 }), Field::req("sha256", Rule::Word { re: SHA_RE, max: 64 }),`
- **upload_done** (write) — Takes the upload and looks at it (a job; its result is what the file is).
- **upload_cancel** (write) — Deletes the upload.
- **apply_file** (write) — Installs the uploaded bundle that upload_done showed (a job; not cancellable).
  - fields: `Field::req("sha256", Rule::Word { re: SHA_RE, max: 64 }), Field::req("confirm", Rule::Enum(&["apply"])), Field::opt("unsigned", Rule::Bool), Field::opt("downgrade", Rule::Bool),`

## `f50.usb`

source: `crates/f50-net/src/objects.rs`

- **get** (read) — The USB-C port: role, network mode, link speed, devices, adb.
- **set** (write) — Sets the port's role and the USB network mode and name (once or for good).
  - fields: `Field::opt("role", Rule::Enum(&["host", "device"])), Field::opt("net", Rule::Enum(&["ncm", "ecm", "rndis", "multi", "default"])), Field::opt("scope", Rule::Enum(&["once", "permanent"])), Field::opt("name", Rule::Enum(&["usb", "eth"])),`
- **adapters** (read) — USB network adapters in the host role, and whether they are in the LAN.
- **adapter_add** (write) — Adds a USB network adapter to the LAN bridge.
  - fields: `Field::req("iface", rules::IFNAME)`
- **adb** (write) — Turns adb over USB on or off, or sets whom it lets in (open or keys).
  - fields: `Field::opt("state", rules::ON_OFF), Field::opt("auth", Rule::Enum(&["open", "keys"]))`
- **adb_key_add** (write) — Authorises a computer's adb public key (adbkey.pub).
  - fields: `Field::req("key", Rule::Text { max_bytes: 800, max_chars: 800 })`

## `f50.wifi`

source: `crates/f50-net/src/objects.rs`

- **get** (read) — The hotspot: name, band, channel, country, security, clients (never the password).
- **credentials** (write) — The saved hotspot's name, password, security and hidden flag for sharing (administrator only).
  - response: `{ ssid, password, security, hidden }`; `password` is `null` for an open network or a missing key.
- **set** (write) — Changes the hotspot's name, password, band, channel, width, hidden flag or country.
  - fields: `Field::opt("ssid", rules::SSID), Field::secret("password", rules::PASSWORD, false), Field::opt("band", rules::BAND), Field::opt("channel", rules::CHANNEL), Field::opt("width", rules::WIDTH), Field::opt("hidden", Rule::Bool), Field::opt("country", rules::COUNTRY),`
- **power** (write) — Turns the hotspot on or off.
  - fields: `Field::req("state", rules::ON_OFF)`

## `f50.wificlient`

source: `crates/f50-net/src/objects.rs`

- **status** (read) — The Wi-Fi uplink: mode, network, signal, address, firewall, saved network.
- **scan** (write) — Lists the networks in range (a job when the radio must scan by itself).
- **join** (write) — Joins a network as the uplink and saves it (a job of kind wifi).
  - fields: `Field::req("ssid", rules::SSID), Field::secret("password", rules::PASSWORD, false), Field::opt("open", Rule::Bool),`
- **leave** (write) — Leaves the network and brings the hotspot back (a job of kind wifi).
  - fields: `Field::opt("forget", Rule::Bool), Field::opt("keep", Rule::Bool)`

