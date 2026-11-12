#!/bin/sh
# netifd protocol handler for the F50's cellular data: installed as /lib/netifd/proto/f50cell.sh.
#   config interface 'wan'
#       option proto 'f50cell'
#       option apn 'internet'            (SIM1, the tray card; SIM2, the internal card: apn_internal ...)
#       option pdptype 'IP|IPV6|IPV4V6'  option auth 'none|pap|chap|both'  option username / password
#       option iccid / roaming / ipv6 'relay' / peerdns / dl_reboot   (see f50-data's settings.rs)
#
# The script is thin on purpose: the dial runs in f50d, which owns the modem's AT channels, and f50d reports the
# result to netifd itself (ubus call network.interface notify_proto: the update, an error, unavailable, setup
# failed). So the handler declares no_proto_task: its setup returns at once and netifd waits for that report.
# Nothing from the configuration is passed on - f50d reads /etc/config/network itself - so the APN password is never
# on a command line (FINDINGS 65).
#
# Only when f50d cannot be asked at all does this script report, the way the old handler did: an error, a pause so
# netifd does not retry every second, then setup failed (netifd tears down and tries again).
[ -n "$INCLUDE_ONLY" ] || {
	. /lib/functions.sh
	. ../netifd-proto.sh
	init_proto "$@"
}

F50D=/usr/sbin/f50d

proto_f50cell_init_config() {
	available=1
	no_device=1
	no_proto_task=1
	renew_handler=1
	# the options LuCI's interface page edits; f50d reads them (and the per-card ones) from UCI
	proto_config_add_string "apn"
	proto_config_add_string "pdptype"
	proto_config_add_string "auth"
	proto_config_add_string "username"
	proto_config_add_string "password"
	proto_config_add_string "iccid"
	proto_config_add_string "apn_internal"
	proto_config_add_string "pdptype_internal"
	proto_config_add_string "ipv6"
	proto_config_add_boolean "peerdns"
}

proto_f50cell_setup() {
	local config="$1" iface="$2"
	"$F50D" proto setup "$config" "$iface" </dev/null >/dev/null 2>&1 && return 0
	logger -t f50cell "f50d did not take the setup of $config"
	proto_notify_error "$config" DAEMON_UNAVAILABLE
	sleep 20
	proto_setup_failed "$config"
}

proto_f50cell_renew() {
	"$F50D" proto renew "$1" </dev/null >/dev/null 2>&1
}

proto_f50cell_teardown() {
	"$F50D" proto teardown "$1" "$2" </dev/null >/dev/null 2>&1 ||
		logger -t f50cell "f50d did not take the teardown of $1"
}

[ -n "$INCLUDE_ONLY" ] || add_protocol f50cell
