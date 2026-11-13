#!/bin/sh
# Switch an installed F50 over USB, using this checkout's boot-control helpers.
set -eu
TOP=$(cd "$(dirname "$0")/../.." && pwd)
. "$TOP/installer/lib/host-tools.sh"
PYTHON=${PYTHON:-python3}
TARGET=${1:-}

die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
case $TARGET in android|openwrt) ;; *) die "usage: $0 android|openwrt" ;; esac
command -v adb >/dev/null || die 'adb not found'

usb() {
    if [ -n "${F50_SERIAL:-}" ]; then adb -s "$F50_SERIAL" "$@"; else adb -d "$@"; fi
}
select_usb() {
    ANDROID_SERIAL=$(usb get-serialno) || die 'Connect the F50 by USB; use F50_SERIAL when more than one is attached.'
    [ -n "$ANDROID_SERIAL" ] && [ "$ANDROID_SERIAL" != unknown ] || die 'No USB ADB device found.'
    export ANDROID_SERIAL
}
linux_default() {
    adb push "$TOP/systems/base/overlay/opt/f50/bin/f50-next-boot" /tmp/f50-next-boot >/dev/null
    adb shell 'bash /tmp/f50-next-boot linux' </dev/null
}

if [ "$TARGET" = openwrt ]; then
    [ -x "$TOP/work/busybox" ] || die 'work/busybox is missing; use the checkout that built the installed F50 image.'
    set -- --prepare-root --ip "${F50_IP:-192.168.0.1}"
    [ -z "${F50_SERIAL:-}" ] || set -- "$@" --serial "$F50_SERIAL"
    "$PYTHON" "$TOP/scripts/device/f50-enable-adb.py" "$@"
fi
select_usb

case $ANDROID_SERIAL in
    F50LINUX-*)
        adb shell 'test -f /etc/openwrt_release && test -x /usr/sbin/f50d' </dev/null ||
            die 'The selected device is not running F50 OpenWrt.'
        if [ "$TARGET" = openwrt ]; then
            linux_default
            echo 'Already running OpenWrt; OpenWrt is now the default boot.'
        else
            adb push "$TOP/systems/base/overlay/opt/f50/bin/f50-next-boot" /tmp/f50-next-boot >/dev/null
            adb shell 'bash /tmp/f50-next-boot android' </dev/null
            adb reboot
            echo 'Rebooting into Android (management address: 192.168.0.1).'
        fi
        exit 0 ;;
esac
model=$(adb shell getprop ro.product.model </dev/null | tr -d '\r')
case $model in *F50*|*f50*) ;; *) die "The selected Android device is not an F50 ($model)." ;; esac
if [ "$TARGET" = android ]; then
    echo 'Already running Android.'
    exit 0
fi

serial=$ANDROID_SERIAL
F50_ROOT_VIA=
root_via || die 'Android root preparation failed.'
adb push "$TOP/work/busybox" /data/local/tmp/f50-busybox >/dev/null
adb push "$TOP/android/magisk/f50-linux-switch/switch.sh" /data/local/tmp/f50-linux-switch.sh >/dev/null
# Check the completion marker as well: legacy su_do channels pipe output through tr.
result=$(su_do 'chmod 755 /data/local/tmp/f50-busybox && F50_BUSYBOX=/data/local/tmp/f50-busybox sh /data/local/tmp/f50-linux-switch.sh --no-reboot && echo F50-SWITCH-OK')
printf '%s\n' "$result"
printf '%s\n' "$result" | grep -qx F50-SWITCH-OK || die 'Boot selection was not confirmed; no reboot requested.'
adb reboot

echo 'Waiting for this F50 to boot OpenWrt (up to 180 seconds)...'
n=0
while [ "$n" -lt 180 ]; do
    linux_serial=$(adb devices | awk -v prefix="F50LINUX-$serial-" 'index($1, prefix) == 1 && $2 == "device" {print $1}')
    if [ -n "$linux_serial" ]; then
        ANDROID_SERIAL=$linux_serial
        export ANDROID_SERIAL
        if adb shell 'test -f /etc/openwrt_release && test -x /usr/sbin/f50d && test -s /run/f50/misc-bc-linux-trial.bin' </dev/null 2>/dev/null; then
            linux_default
            echo 'OpenWrt is running and is the default boot: http://192.168.77.1/f50/'
            exit 0
        fi
    fi
    n=$((n + 1))
    sleep 1
done
die 'OpenWrt did not appear over USB; boot selection will not be written again. Check the device and USB connection.'
