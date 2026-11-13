#!/bin/sh
# Build the ZTE F50's OpenWrt rootfs tarball (runs on the host; needs Docker with arm64 support, and Rust for f50d).
#   systems/openwrt/build-rootfs.sh [OUT.tar.gz]   (default f50-openwrt-<version>-rootfs.tar.gz)
# OUT is written into systems/openwrt/ under its base name. One image: OpenWrt's release for the 5.4 vendor kernel,
# with f50d (the device daemon, f50d/) and the web console (f50-web, /www/f50), installed by f50d/openwrt/install.sh.
# Inputs (all optional except modules), under F50_INPUTS (default: the checkout):
#   out/modules/*.ko  out/modules.builtin*  firmware/  android-subset/  android-gpu-subset/
#   device-tools/logdw/logdw  device-tools/bt-init/f50-bt-init  device-tools/keys/f50-keys  device-tools/gpu/cltest  busybox (static, full)
#   device-tools/adbd/adbd  device-tools/carry/f50-carry (static: an update carries everything of the user's over with it)
# F50_WEB: the web console's build (default f50-web/dist, `make web`); F50_WEB= builds the image without it.
set -eu
# the other systems and flavours this script once built (openwrt-luci, ImmortalWrt) are gone: refused, not ignored
[ "${F50_SYSTEM:-openwrt}" = openwrt ] && [ "${F50_FLAVOUR:-openwrt}" = openwrt ] ||
    { echo "only the plain OpenWrt image is built (F50_SYSTEM=openwrt, F50_FLAVOUR=openwrt)" >&2; exit 1; }
VER=${F50_WRT_VER:-25.12.6}
BASEURL=https://downloads.openwrt.org/releases
# The hash comes from the release's sha256sums, and that file counts only with a valid usign signature by the
# release's key, pinned in systems/openwrt/keys/usign/ (OpenWrt's keyring, git.openwrt.org/keyring.git usign/<number>): the
# 25.12 releases are signed by b5043e70f9a75cde. Another release series is signed by another key, which has to be
# pinned here first. Without the check, whoever could change the download (a mirror, a proxy) could change sha256sums
# with it, and the tarball would match.
case $VER in
    25.12.*) USIGN_KEY=b5043e70f9a75cde ;;
    *) echo "no pinned usign key for openwrt $VER (systems/openwrt/keys/usign, systems/openwrt/build-rootfs.sh)" >&2; exit 1 ;;
esac
KREL=5.4.254-gb50db5b6224c
OUT=${1:-f50-openwrt-$VER-rootfs.tar.gz}
TOP=$(cd "$(dirname "$0")/../.." && pwd)
# build inputs (out/, firmware/, android-subset/, tools binaries, busybox) may live outside the checkout
IN=${F50_INPUTS:-$TOP}
# f50d's side of the image: without its cellular protocol netifd has no wan and the image boots without mobile data
for f in install.sh netifd/f50cell.sh; do
    [ -s "$TOP/f50d/openwrt/$f" ] || { echo "required f50d file missing: f50d/openwrt/$f" >&2; exit 1; }
done
# sipa_eth0 into fw4's software flowtable (applied below, the build fails when it no longer applies)
FW4PATCH=$TOP/systems/openwrt/patches/fw4-sipa-offload.patch
[ -s "$FW4PATCH" ] || { echo "missing $FW4PATCH" >&2; exit 1; }
# fw4 reload/restart on the 5.4 kernel: its old flowtable goes first, in a transaction of its own (#95, FINDINGS 38)
FW4FTPATCH=$TOP/systems/openwrt/patches/fw4-old-kernel-flowtable.patch
[ -s "$FW4FTPATCH" ] || { echo "missing $FW4FTPATCH" >&2; exit 1; }
# never an access point on a DFS channel: the SC2355 driver has no radar detection (FINDINGS 42)
NODFSPATCH=$TOP/systems/openwrt/patches/wifi-scripts-no-dfs.patch
[ -s "$NODFSPATCH" ] || { echo "missing $NODFSPATCH" >&2; exit 1; }
# Docker (FINDINGS 130): in the image (F50_DOCKER=1) or ready to install with apk (0, the default: the image carries
# its settings and firewall zone only). In the image, its 190 MB are there twice while an update unpacks the new system
# beside the old one, and an update by zip or installer no longer fits the smallest region OpenWrt is installed in
# (800 MiB: "No space left on device", measured on an F50).
case ${F50_DOCKER:-0} in
    0|1) DOCKER=${F50_DOCKER:-0} ;;
    *) echo "F50_DOCKER is 0 or 1" >&2; exit 1 ;;
esac
# an access point is WPA2-PSK only: SAE (WPA3-Personal, its mixed and compatibility modes) is refused on the SC2355
# (FINDINGS 80)
WPA2PATCH=$TOP/systems/openwrt/patches/wifi-scripts-wpa2-only.patch
[ -s "$WPA2PATCH" ] || { echo "missing $WPA2PATCH" >&2; exit 1; }
TARBALL=openwrt-$VER-armsr-armv8-rootfs.tar.gz
URL=$BASEURL/$VER/targets/armsr/armv8

cd "$TOP"
if [ ! -f "systems/openwrt/$TARBALL" ]; then
    curl -fL -o "systems/openwrt/$TARBALL" "$URL/$TARBALL"
fi
# sha256sums (below) counts only with a valid usign signature by the release's key
SUMS=$(mktemp -d)
curl -fsL -o "$SUMS/sha256sums" "$URL/sha256sums" && curl -fsL -o "$SUMS/sha256sums.sig" "$URL/sha256sums.sig" ||
    { echo "cannot download $URL/sha256sums and its signature" >&2; rm -rf "$SUMS"; exit 1; }
python3 "$TOP/scripts/release/usign-verify.py" "$TOP/systems/openwrt/keys/usign/$USIGN_KEY" "$SUMS/sha256sums.sig" "$SUMS/sha256sums" >/dev/null ||
    { echo "$URL/sha256sums: no valid signature by OpenWrt's key $USIGN_KEY" >&2; rm -rf "$SUMS"; exit 1; }
want=$(sed -n "s/^\([0-9a-f]\{64\}\) \*$TARBALL$/\1/p" "$SUMS/sha256sums")
rm -rf "$SUMS"
[ -n "$want" ] || { echo "$TARBALL is not in the signed sha256sums" >&2; exit 1; }
have=$(shasum -a 256 "systems/openwrt/$TARBALL" 2>/dev/null || sha256sum "systems/openwrt/$TARBALL")
[ "${have%% *}" = "$want" ] || { echo "checksum mismatch for $TARBALL" >&2; exit 1; }
docker import --platform linux/arm64 "systems/openwrt/$TARBALL" f50-openwrt-base:$VER >/dev/null
# OpenWrt ships an unsigned regulatory.db; this kernel requires the signed database (wens key), so take Debian/Ubuntu's
REGDB=$(mktemp -d)
docker run --rm --platform linux/arm64 -v "$REGDB":/o ubuntu:26.04 sh -c \
  "apt-get update -qq >/dev/null && apt-get install -y -qq wireless-regdb >/dev/null && cp /usr/lib/firmware/regulatory.db /usr/lib/firmware/regulatory.db.p7s /o/"

opt() { [ -e "$IN/$1" ] && echo "-v $IN/$1:/in/$2:ro" || true; }

# The required input first, with a readable message: without it docker fails somewhere inside the build.
ls "$IN/out/modules"/*.ko >/dev/null 2>&1 || {
    echo "no kernel modules in $IN/out/modules - build them first (kernel/vendor/build-linux.sh), or point" >&2
    echo "F50_INPUTS at the directory that has out/modules, firmware/ and android-subset/." >&2
    exit 1
}
# The optional ones decide whether the image can use the modem, Wi-Fi or the GPU at all. Missing ones
# used to be skipped silently, which produces an image that boots and then does nothing useful.
for o in firmware android-subset android-gpu-subset device-tools/logdw/logdw device-tools/bt-init/f50-bt-init device-tools/keys/f50-keys device-tools/adbd/adbd device-tools/carry/f50-carry device-tools/gpu/cltest busybox; do
    [ -e "$IN/$o" ] && echo "  + $o" || echo "  - $o   (missing: the image is built without it)"
done
# f50d, the Rust device daemon (f50d/): built here for the device and installed in the image by
# f50d/openwrt/install.sh, which takes the shell layer of the overlays out of the image.
command -v cargo >/dev/null || { echo "f50d needs Rust (cargo)" >&2; exit 1; }
(cd "$TOP/f50d" && cargo build --release --locked --target aarch64-unknown-linux-musl -p f50d) || exit 1
# the docker mounts of f50d and the web console, as the positional parameters (OUT is read above): each path stays one
# argument, spaces and all
set -- -v "$TOP/f50d/target/aarch64-unknown-linux-musl/release/f50d":/in/f50d:ro \
    -v "$TOP/f50d/openwrt":/in/f50d-openwrt:ro
# the web console (f50-web's build, `make web`), served as /www/f50; F50_WEB= builds the image without it
F50_WEB=${F50_WEB-$TOP/f50-web/dist}
if [ -n "$F50_WEB" ]; then
    [ -f "$F50_WEB/index.html" ] || { echo "no web console build in $F50_WEB (make web; F50_WEB= builds without it)" >&2; exit 1; }
    set -- "$@" -v "$F50_WEB":/in/f50-web:ro
fi
# shellcheck disable=SC2046
docker run --rm --platform linux/arm64 \
  -v "$TOP/systems/base/overlay/opt/f50":/in/opt-f50:ro -v "$TOP/systems/openwrt/overlay":/in/overlay:ro \
  -v "$FW4PATCH":/in/fw4-sipa-offload.patch:ro \
  -v "$FW4FTPATCH":/in/fw4-old-kernel-flowtable.patch:ro \
  -v "$NODFSPATCH":/in/wifi-scripts-no-dfs.patch:ro \
  -v "$WPA2PATCH":/in/wifi-scripts-wpa2-only.patch:ro \
  -v "$TOP/boot/module-order.txt":/in/module-order.txt:ro -v "$IN/out/modules":/in/modules:ro \
  $(opt out/modules.builtin modules.builtin) $(opt out/modules.builtin.modinfo modules.builtin.modinfo) \
  $(opt firmware firmware) $(opt android-subset android-subset) $(opt android-gpu-subset android-gpu-subset) \
  $(opt device-tools/logdw/logdw logdw) $(opt device-tools/bt-init/f50-bt-init bt-init) $(opt device-tools/keys/f50-keys keys) $(opt device-tools/adbd/adbd adbd) $(opt device-tools/gpu/cltest cltest) \
  $(opt busybox busybox) $(opt device-tools/carry/f50-carry carry) -v "$TOP/systems/openwrt":/out -v "$REGDB":/in/regdb:ro "$@" \
  -e KREL=$KREL -e OUT="$(basename "$OUT")" -e F50_VERSION="${F50_VERSION:-dev}" -e F50_DOCKER="$DOCKER" f50-openwrt-base:$VER /bin/sh -eu -c '
mkdir -p /var/lock /var/run /tmp
# The packages come from the feed over the network while the image is built, and one connection cut in the middle of
# one package failed the whole image, and a release with it. update, upgrade and add are tried three times, ten
# seconds apart; what fails three times (a package the feed does not have) fails the build as before. Everything
# else (del) runs once. The idea is from kanoqwq/clean-tf-7.2 33640d01 (FINDINGS 66).
# --- apk-retry begin
apk() {
    case ${1:-} in
        update|upgrade|add) ;;
        *) command apk "$@"; return ;;
    esac
    _try=1
    until command apk "$@"; do
        [ "$_try" -lt 3 ] || return 1
        _try=$((_try + 1))
        echo "apk $1 failed; trying again ($_try of 3)" >&2
        sleep "${F50_APK_RETRY_WAIT:-10}"
    done
}
# --- apk-retry end
apk update >/dev/null
# the release tarball carries the packages of the day the release was cut; the feed of the release branch moves on
# (LuCI, mbedtls, the CA bundle, iwinfo ...), and "apk add" below installs only what is missing, so without this
# an image shipped months later still has the old LuCI and the package manager shows every update on the first look
apk upgrade >/dev/null
# openssl-util: the VPN module (github.com/dikeckaan/mu300-linux-vpn) fetches the VPN server certificate with it to
# pin, for links that ask for allowInsecure; jq: the VPN module reads vmess links and raw Xray/sing-box configs with
# it, and rewrites those configs at every start; curl: a fetch tool that tells "cannot be reached" from "answered"
# by its exit codes, which the wget family does not distinguish
apk add wpad-basic-mbedtls wifi-scripts iwinfo wireless-regdb iw bash flock ip-full coreutils-stty openssl-util \
    jq curl >/dev/null
# the router protocols LuCI offers, with their tools: WireGuard, PPTP/L2TP (PPPoE is in the base), 6in4/6rd/DS-Lite,
# GRE and VXLAN, ipset, and SQM (cake). Their kmod-* dependencies install the 6.12 modules of the feed, removed below like
# every other kmod: the 5.4 kernel has some of them only
apk add wireguard-tools luci-proto-wireguard ppp-mod-pptp xl2tpd 6in4 6rd ds-lite gre luci-proto-gre vxlan \
    luci-proto-vxlan ipset sqm-scripts luci-app-sqm >/dev/null
# Docker (F50_DOCKER=1, FINDINGS 130): the engine, its command and compose, from the feed (Docker 29). Their kmod-*
# dependencies are the 6.12 modules of the feed, removed below like every other kmod; what they add to /etc/modules.d
# and /etc/modules-boot.d goes here, or ubox would try to load br_netfilter, the legacy iptables tables, btrfs and
# raid6 at every boot (overlay/etc/sysctl.d/13-f50-docker.conf keeps the LAN bridge out of the firewall when
# br_netfilter is loaded all the same: an apk add on the device). The nftables backend of Docker needs none of them.
# The stress tool and the command line of containerd (ctr) are not used by docker: 39 MB less. dockerd does not start
# at boot (the enable link apk made goes).
if [ "$F50_DOCKER" = 1 ]; then
    ls /etc/modules.d /etc/modules-boot.d > /tmp/modules.before 2>/dev/null || true
    apk add dockerd docker docker-compose >/dev/null
    for d in /etc/modules.d /etc/modules-boot.d; do
        for m in $(ls $d 2>/dev/null); do grep -qx "$m" /tmp/modules.before || rm -f "$d/$m"; done
    done
    rm -f /usr/bin/containerd-stress /usr/bin/ctr /etc/rc.d/S*dockerd /etc/rc.d/K*dockerd
    # (the data of Docker is not in /opt/docker, the default of the package: etc/docker/daemon.json)
    rmdir /opt/docker 2>/dev/null || true
    [ -x /usr/bin/dockerd ] && [ -x /usr/bin/docker ] && [ -x /usr/lib/docker/cli-plugins/docker-compose ] ||
        { echo "Docker is not complete" >&2; exit 1; }
fi
# ujail drops CAP_PERFMON (38), which this 5.4 kernel does not know: jailed services (dnsmasq, ntpd) crash-loop
apk del procd-ujail procd-seccomp >/dev/null 2>&1 || true
# online firmware upgrades flash whole-disk armsr images: that would overwrite the eMMC, so remove them
apk del luci-app-attendedsysupgrade attendedsysupgrade-common owut >/dev/null 2>&1 || true
# what those leave in /etc/config, and the .apk-new copies the upgrade above writes beside a changed config: an update
# adds every config file of the image the device does not have yet, and these belong to no package
rm -f /etc/config/attendedsysupgrade /etc/config/*.apk-new
# LuCI in Turkish and Simplified Chinese besides English: the translation of luci-base and of every LuCI app the
# image has that the feed translates (apk checks each package against the signed index). Their own uci-defaults, run
# by apk here, register the languages in luci.languages.
i18n=
for c in base $(apk list --installed "luci-app-*" | sed -n "s/^luci-app-\([^ ]*\)-[0-9][^ -]* .*/\1/p"); do
    for l in tr zh-cn; do
        apk search "luci-i18n-$c-$l" | grep -q "^luci-i18n-$c-$l-[0-9]" && i18n="$i18n luci-i18n-$c-$l"
    done
done
echo "LuCI translations:$i18n"
for l in tr zh-cn; do
    case " $i18n " in *" luci-i18n-base-$l "*) ;; *) echo "no LuCI base translation $l in the feed" >&2; exit 1 ;; esac
done
apk add $i18n >/dev/null
R=/build/root; mkdir -p $R
# copy the live filesystem of this container (the OpenWrt rootfs plus packages), without runtime mounts
for e in /*; do
    case "$e" in /proc|/sys|/dev|/build|/in|/out|/tmp) continue ;; esac
    cp -a "$e" $R/
done
mkdir -p $R/proc $R/sys $R/dev $R/tmp $R/run $R/opt
# Docker bind-mounts these three into the container, so the copy above picks up the build host versions:
# a resolv.conf pointing at the internal Docker DNS (which broke every lookup the device itself made), a hosts
# file with the container id, and a hostname that was the container id. Put the OpenWrt ones back.
# (no apostrophes in here: this whole block is one single-quoted argument to sh -c)
ln -sf /tmp/resolv.conf $R/etc/resolv.conf
printf "127.0.0.1\tlocalhost\n\n::1\tlocalhost ip6-localhost ip6-loopback\nff02::1\tip6-allnodes\nff02::2\tip6-allrouters\n" > $R/etc/hosts
printf "f50\n" > $R/etc/hostname   # the real one comes from uci (etc/uci-defaults/90-f50)
cp -a /in/opt-f50 $R/opt/f50
cp -a /in/overlay/. $R/
# f50cell reports sipa_eth0 as l3_device only, so fw4 leaves it out of its software flowtable and the cellular
# downlink takes the slow forwarding path. --fuzz=0: a changed fw4 that no longer matches fails the build.
# (patch goes into the build container only: the image was copied above)
apk add patch >/dev/null
patch --batch --fuzz=0 -d $R -p1 -i /in/fw4-sipa-offload.patch
grep -q sipa_eth0 $R/usr/share/ucode/fw4.uc || { echo "fw4 patch not applied" >&2; exit 1; }
patch --batch --fuzz=0 -d $R -p1 -i /in/fw4-old-kernel-flowtable.patch
grep -q drop_old_flowtable $R/sbin/fw4 || { echo "fw4 flowtable patch not applied" >&2; exit 1; }
# wifi-scripts: a driver without radar detection (no radar_detect_widths, the SC2355) never gets an access point on a
# DFS channel - hostapd fails start_dfs_cac and the hotspot stays down otherwise (FINDINGS 42)
patch --batch --fuzz=0 -d $R -p1 -i /in/wifi-scripts-no-dfs.patch
grep -q "device_no_dfs(config);" $R/usr/share/ucode/wifi/hostapd.uc || { echo "wifi-scripts DFS patch not applied" >&2; exit 1; }
# Docker keeps its images and containers on the Linux partition beside the system (/mnt/f50-disk/docker/openwrt);
# the package reads etc/docker/daemon.json (alt_config_file)
grep -q "\"data-root\": \"/mnt/f50-disk/docker/openwrt\"," $R/etc/docker/daemon.json || { echo "Docker data-root not set" >&2; exit 1; }
grep -q "^.option alt_config_file ./etc/docker/daemon.json.$" $R/etc/config/dockerd || { echo "dockerd does not read etc/docker/daemon.json" >&2; exit 1; }
! ls $R/etc/rc.d/*dockerd >/dev/null 2>&1 || { echo "dockerd starts at boot" >&2; exit 1; }
# wifi-scripts: an access point that asks for SAE (sae, sae-mixed, sae-compat, psk3) is not started, with a line that
# says WPA2-PSK only - plain SAE never worked on the SC2355 (FINDINGS 80)
patch --batch --fuzz=0 -d $R -p1 -i /in/wifi-scripts-wpa2-only.patch
grep -q "f50_sae_refused(k, interface.config)" $R/usr/share/ucode/wifi/hostapd.uc || { echo "wifi-scripts WPA2-only patch not applied" >&2; exit 1; }
apk del patch >/dev/null   # out of packages.txt below, which lists what the image has
# the languages LuCI offers (System > System > Language and Style lists English and these); the base catalogs register
# their own, this covers one that did not
L=$R/usr/lib/lua/luci/i18n
for l in tr zh-cn; do
    [ -s $L/base.$l.lmo ] || { echo "LuCI catalog base.$l.lmo missing" >&2; exit 1; }
done
uci -c $R/etc/config -q get luci.languages.tr >/dev/null || uci -c $R/etc/config set luci.languages.tr="Türkçe (Turkish)"
uci -c $R/etc/config -q get luci.languages.zh_cn >/dev/null ||
    uci -c $R/etc/config set luci.languages.zh_cn="$(printf "\347\256\200\344\275\223\344\270\255\346\226\207 (Simplified Chinese)")"
uci -c $R/etc/config commit luci
uci -c $R/etc/config show luci.languages
# SQM: the default queue (disabled) shapes the generic eth1, which the F50 does not have, so LuCI offered a device
# that is not there; it names the cellular uplink, sipa_eth0, instead
if [ -f $R/etc/config/sqm ]; then
    for q in $(uci -c $R/etc/config -q show sqm | sed -n "s/^sqm\.\([^.]*\)\.interface=.eth1.$/\1/p"); do
        uci -c $R/etc/config set sqm.$q.interface=sipa_eth0
    done
    uci -c $R/etc/config commit sqm
    uci -c $R/etc/config -q show sqm | grep -q "\.interface=.eth1.$" && { echo "SQM still names eth1" >&2; exit 1; }
fi
mv $R/sbin/sysupgrade $R/sbin/sysupgrade.openwrt && mv $R/usr/libexec/f50-sysupgrade $R/sbin/sysupgrade
M=$R/lib/modules/$KREL; mkdir -p $M
cp /in/modules/*.ko $M/          # ubox kmodloader expects the modules flat in /lib/modules/<release>/
for f in modules.builtin modules.builtin.modinfo; do [ -e /in/$f ] && cp /in/$f $M/; done
[ -d /in/firmware ] && { mkdir -p $R/lib/firmware; cp -a /in/firmware/. $R/lib/firmware/; }
# f50d and f50-bt-init look under /usr/lib/firmware too; OpenWrt keeps firmware in /lib/firmware
mkdir -p $R/usr/lib && ln -sfn ../../lib/firmware $R/usr/lib/firmware
cp /in/regdb/regulatory.db /in/regdb/regulatory.db.p7s $R/lib/firmware/
if [ -d /in/android-subset ]; then
    mkdir -p $R/opt/f50/android && cp -a /in/android-subset/. $R/opt/f50/android/
    mv $R/opt/f50/android/dev/__properties__ $R/opt/f50/android/dev-properties && rmdir $R/opt/f50/android/dev
fi
[ -d /in/android-gpu-subset ] && cp -a /in/android-gpu-subset/. $R/opt/f50/android/
[ -e /in/cltest ] && { mkdir -p $R/opt/f50/android/system/bin; cp /in/cltest $R/opt/f50/android/system/bin/cltest; chmod 755 $R/opt/f50/android/system/bin/cltest; }
[ -e /in/logdw ] && { cp /in/logdw $R/opt/f50/bin/logdw; chmod 755 $R/opt/f50/bin/logdw; }
[ -e /in/bt-init ] && { cp /in/bt-init $R/opt/f50/bin/f50-bt-init; chmod 755 $R/opt/f50/bin/f50-bt-init; }
[ -e /in/keys ] && { cp /in/keys $R/opt/f50/bin/f50-keys; chmod 755 $R/opt/f50/bin/f50-keys; }
# adb over USB (device-tools/adbd, static)
[ -e /in/adbd ] && { cp /in/adbd $R/opt/f50/bin/adbd; chmod 755 $R/opt/f50/bin/adbd; }
# an update carries everything of the user over with it (device-tools/carry, static). Without it an update of this
# image keeps only the list of places, as before the first release with a four-field tag
[ -e /in/carry ] && { cp /in/carry $R/opt/f50/bin/f50-carry; chmod 755 $R/opt/f50/bin/f50-carry; }
# the VPN engines are not part of the image: they are the module of github.com/dikeckaan/mu300-linux-vpn
# full static busybox for the tools OpenWrt busybox leaves out (od, timeout, mountpoint, losetup, rfkill, telnetd)
if [ -e /in/busybox ]; then
    cp /in/busybox $R/opt/f50/bin/busybox; chmod 755 $R/opt/f50/bin/busybox
    mkdir -p $R/opt/f50/busybox-bin
    for a in od timeout losetup telnetd getty; do
        chroot $R /bin/sh -c "command -v $a" >/dev/null 2>&1 && continue
        ln -sf ../bin/busybox $R/opt/f50/busybox-bin/$a
    done
fi
mkdir -p $R/etc/f50
printf "%s\n" "${F50_VERSION:-dev}" > $R/etc/f50/image-version
# busybox PATH is /usr/sbin:/usr/bin:/sbin:/bin: the one command of /opt/f50/bin the image keeps for people
ln -sf /opt/f50/bin/f50-device $R/usr/bin/f50-device
# the OpenWrt button handlers (power: a tap is poweroff; wps, rfkill) are replaced: f50d owns the keys
mkdir -p $R/etc/rc.button
for b in power wps rfkill; do
    printf "#!/bin/sh\n# f50d owns the keys\nexit 0\n" > "$R/etc/rc.button/$b"
    chmod 0755 "$R/etc/rc.button/$b"
done
# no config file of a removed package and no .apk-new copy in the image: an update adds every etc/config file of the
# image the device lacks, so these would land on every device (v2026.10.18, built before the cleanup above, had
# both). Removed again here, in the image itself, and checked.
rm -f $R/etc/config/attendedsysupgrade $R/etc/config/*.apk-new
# no TLS key of the web server in the image: the uhttpd package makes the pair when apk installs it here, and every
# release up to v2026.10.16.1 shipped that one key to every device (FINDINGS 132). uhttpd makes the device its own
# at its first start, as it does whenever the pair is missing
rm -f $R/etc/uhttpd.key $R/etc/uhttpd.crt
[ ! -e $R/etc/uhttpd.key ] || { echo "the image holds a TLS key" >&2; exit 1; }
for f in $R/etc/config/attendedsysupgrade $R/etc/config/*.apk-new; do
    [ ! -e "$f" ] || { echo "stray config file in the image: $f" >&2; exit 1; }
done
# what apk installed, for comparing two builds (packages on the release feed are not pinned)
apk list --installed | sort > $R/etc/f50/packages.txt
# the packages the image asks for (its world file): an update installs the others again, the ones added on the
# device (f50-update, upk_record; read with its own function)
(F50_LIB=1; . /in/opt-f50/bin/f50-update; upk_manual $R openwrt) > $R/etc/f50/image-packages
[ -s $R/etc/f50/image-packages ] || { echo "no package list for etc/f50/image-packages" >&2; exit 1; }
# no kernel of its own: OpenWrt kmods (6.12) and grub are unused on this device
rm -rf $R/lib/modules/6.* $R/boot
# f50d in place of the shell layer of the overlays (before the manifest: it changes the image)
[ -x /in/f50d ] || { echo "f50d is not built" >&2; exit 1; }
sh /in/f50d-openwrt/install.sh $R /in/f50d /in/f50d-openwrt $([ -f /in/f50-web/index.html ] && echo /in/f50-web)
# The manifest of the image: every file it holds, with type, mode, owner, size and sha256 (scripts/release/image-manifest.py has
# the format and makes the same lines from the tarball; make-release.sh holds the two against each other). The next
# update reads it to tell the files of the image from those of the user. Last, when nothing in $R changes any more.
if [ -x $R/opt/f50/bin/f50-carry ]; then
    $R/opt/f50/bin/f50-carry manifest $R openwrt "${F50_VERSION:-dev}" | gzip -9n > /tmp/image-manifest.gz
    [ "$(gzip -dc /tmp/image-manifest.gz | wc -l)" -gt 1000 ] || { echo "the image manifest is not right" >&2; exit 1; }
    cp /tmp/image-manifest.gz $R/etc/f50/image-manifest.gz; chmod 644 $R/etc/f50/image-manifest.gz
fi
cd $R && tar -czf /out/$OUT .
ls -la /out/$OUT'
rm -rf "$REGDB"
