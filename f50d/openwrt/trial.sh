#!/bin/sh
# A reversible trial of f50d on a device that runs the mu300 image, switched at the next reboot.
#
#   trial.sh stage [DIR]   DIR (default /tmp/f50d-trial) holds f50d, openwrt/ (this directory) and web/ (f50-web/dist)
#   trial.sh restore       puts the mu300 boot back as it was; reboot afterwards
#   trial.sh status
#
# Nothing running is stopped: the mu300 AT daemon keeps its channels until the reboot (a channel closed while the
# device runs can be lost until the next boot, FINDINGS 13b). `stage` only changes what the next boot starts:
# - the backup goes to /root/f50d-trial (the mu300 boot links, hotplug hooks and preinit hooks are *moved* there,
#   never deleted; /etc/config/network is copied);
# - mu300-adbd stays enabled: it is the way back in. f50d is told to leave adbd alone (f50d.main.adbd 0);
# - f50d's binary, init scripts, hooks, netifd handler and the web console (/www/f50) are installed, network.wan
#   becomes proto f50cell. LuCI stays the front page.
set -eu
B=/root/f50d-trial
SRC=${2:-/tmp/f50d-trial}

mu300_link() {
    case $1 in
        S19mu300-adbd) return 1 ;;
        *mu300-*|*unisoc-modem-ui) return 0 ;;
    esac
    return 1
}

HOOKS="iface/10-mu300-usb iface/19-mu300-fw4 iface/90-unisoc-usb-host iface/95-mu300-terminal iface/96-mu300-vpn-uplink
net/90-unisoc-usb-host misc/10-mu300-wcn ntp/20-mu300-clock"
PREINIT="04_mu300_early_leds 05_mu300_early_recorder"

stage() {
    [ ! -e "$B/staged" ] || { echo "already staged (trial.sh restore first)" >&2; exit 1; }
    [ -x "$SRC/f50d" ] && [ -d "$SRC/openwrt/init.d" ] || { echo "no f50d or openwrt/ in $SRC" >&2; exit 1; }
    file_ok=$(head -c 4 "$SRC/f50d" | od -An -c | tr -d ' ')
    [ "$file_ok" = '177ELF' ] || { echo "$SRC/f50d is not an ELF program" >&2; exit 1; }

    mkdir -p "$B/rc.d" "$B/hooks" "$B/preinit"
    cp "$0" "$B/trial.sh"
    cp -a /etc/config/network "$B/network"
    [ -e /etc/config/f50d ] && cp -a /etc/config/f50d "$B/f50d.config"
    ls -la /etc/rc.d > "$B/rc.d.before"

    # the next boot: no mu300 services (but adbd), no mu300 hooks
    for l in /etc/rc.d/*; do
        n=${l##*/}
        mu300_link "$n" && mv "$l" "$B/rc.d/$n"
    done
    for h in $HOOKS; do
        [ -e "/etc/hotplug.d/$h" ] || continue
        mkdir -p "$B/hooks/${h%/*}"
        mv "/etc/hotplug.d/$h" "$B/hooks/$h"
    done
    for p in $PREINIT; do [ -e "/lib/preinit/$p" ] && mv "/lib/preinit/$p" "$B/preinit/$p"; done

    # f50d
    # (busybox has no install(1))
    cp "$SRC/f50d" /usr/sbin/f50d.new
    chmod 0755 /usr/sbin/f50d.new
    mv /usr/sbin/f50d.new /usr/sbin/f50d
    mkdir -p /usr/libexec/f50d && ln -sf ../../sbin/f50d /usr/libexec/f50d/modprobe-guard
    for f in "$SRC"/openwrt/init.d/*; do cp "$f" "/etc/init.d/${f##*/}"; chmod 0755 "/etc/init.d/${f##*/}"; done
    for d in iface net misc ntp; do
        mkdir -p "/etc/hotplug.d/$d"
        for f in "$SRC"/openwrt/hotplug.d/$d/*; do cp "$f" "/etc/hotplug.d/$d/${f##*/}"; chmod 0755 "/etc/hotplug.d/$d/${f##*/}"; done
    done
    cp "$SRC/openwrt/netifd/f50cell.sh" /lib/netifd/proto/f50cell.sh && chmod 0755 /lib/netifd/proto/f50cell.sh
    for f in "$SRC"/openwrt/preinit/*; do cp "$f" "/lib/preinit/${f##*/}"; done
    cp "$SRC/openwrt/config/f50d" /etc/config/f50d
    uci set f50d.main.adbd='0'
    uci commit f50d
    for f in "$SRC"/openwrt/init.d/*; do
        n=${f##*/}
        s=$(sed -n 's/^START=//p' "$f"); [ -n "$s" ] && ln -sf "../init.d/$n" "/etc/rc.d/S$s$n"
        k=$(sed -n 's/^STOP=//p' "$f"); [ -n "$k" ] && ln -sf "../init.d/$n" "/etc/rc.d/K$k$n"
    done
    if [ -d "$SRC/web" ]; then
        rm -rf /www/f50 && mkdir -p /www/f50 && cp -R "$SRC/web/." /www/f50/
    fi
    [ "$(uci -q get network.wan.proto)" = mu300cell ] && { uci set network.wan.proto='f50cell'; uci commit network; }

    date > "$B/staged"
    echo "staged: the next boot runs f50d (backup in $B; undo with: sh /root/f50d-trial/trial.sh restore)"
}

restore() {
    [ -e "$B/staged" ] || { echo "nothing staged" >&2; exit 1; }
    for l in /etc/rc.d/*f50d*; do [ -e "$l" ] || [ -L "$l" ] && rm -f "$l"; done
    rm -f /etc/hotplug.d/iface/20-f50d /etc/hotplug.d/net/20-f50d /etc/hotplug.d/misc/10-f50d-wcn \
        /etc/hotplug.d/ntp/20-f50d /lib/preinit/05_f50_early_recorder
    for l in "$B"/rc.d/*; do [ -e "$l" ] || [ -L "$l" ] && mv "$l" /etc/rc.d/; done
    for h in $HOOKS; do [ -e "$B/hooks/$h" ] && mv "$B/hooks/$h" "/etc/hotplug.d/$h"; done
    for p in $PREINIT; do [ -e "$B/preinit/$p" ] && mv "$B/preinit/$p" "/lib/preinit/$p"; done
    cp -a "$B/network" /etc/config/network
    rm -f "$B/staged"
    echo "restored: the next boot runs the mu300 system again (f50d's files stay, unused; /www/f50 stays)"
}

case ${1:-} in
    stage) stage ;;
    restore) restore ;;
    status)
        [ -e "$B/staged" ] && echo "staged $(cat "$B/staged")" || echo "not staged"
        ls /etc/rc.d | grep -E 'mu300|unisoc|f50d' | tr '\n' ' '; echo
        echo "wan proto: $(uci -q get network.wan.proto)"
        ;;
    *) echo "usage: trial.sh stage [DIR] | restore | status" >&2; exit 2 ;;
esac
