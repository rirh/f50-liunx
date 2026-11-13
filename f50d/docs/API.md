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
  `cells` (`f50.modem cells`, 5 s), `sms` (`f50.sms unread`, 5 s).

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
- **restart** (write) — Restarts the modem's protocol stack (a job).

## `f50.proto`

source: `crates/f50-data/src/proto/mod.rs`

- **setup** (write) — netifd's f50cell setup: starts the dial.
  - fields: `Field::req("config", Rule::Word { re: NAME_RE, max: 15 }), Field::opt("iface", Rule::Word { re: IFNAME_RE, max: 32 }),`
- **teardown** (write) — netifd's f50cell teardown.
  - fields: `Field::req("config", Rule::Word { re: NAME_RE, max: 15 }), Field::opt("iface", Rule::Word { re: IFNAME_RE, max: 32 }),`
- **renew** (write) — netifd's f50cell renew.
  - fields: `Field::req("config", Rule::Word { re: NAME_RE, max: 15 })`

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
  - fields: `Field::opt("page", Rule::Int { min: 1, max: 100_000 }), Field::opt("per_page", Rule::Int { min: 1, max: 100 }), Field::opt("peer", Rule::Text { max_bytes: 64, max_chars: 32 }),`
- **read** (write) — One message whole; a received one is marked read.
  - fields: `Field::req("id", Rule::WordNum { re: ID_RE, max: 9 })`
- **send** (write) — Sends a message (up to 10 parts) as a job.
  - fields: `Field::req("number", Rule::Word { re: r"\+?[0-9]{1,20}", max: 21 }), Field::secret("text", Rule::Body { max_chars: 1530 }, true),`
- **delete** (write) — Deletes a message or all; with sim, the matching ones in the modem's store too.
  - fields: `Field::req("id", Rule::WordNum { re: "all|[1-9][0-9]{0,8}", max: 9 }), Field::opt("sim", Rule::Bool)`
- **sync** (write) — Moves what the modem's store holds into the pool, in the background.
- **unread** (read) — How many messages the pool holds, and how many are unread.

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

