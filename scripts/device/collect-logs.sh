#!/bin/sh
# After a trial falls back to Android: collect LK log, pstore and the init log persisted in the Linux boot partition.
set -eu
TOP=$(cd "$(dirname "$0")/../.." && pwd)
. "$TOP/installer/lib/host-tools.sh"    # adb, su_do
OUT=${1:-logs-$(date +%Y%m%d-%H%M%S)}
mkdir -p "$OUT"
su_do "getprop ro.boot.slot_suffix; ls -la /sys/fs/pstore" | tee "$OUT/state.txt"
# the persistent log is in the slot Android is not on: boot_b, or boot_a when Android runs from b
slot=$(su_do "getprop ro.boot.slot_suffix" </dev/null | tr -d '\r')
case $slot in _b) LS=a ;; *) LS=b ;; esac
# via a file on the device, never streamed: su may run on a pty that rewrites LF as CRLF (issue #2)
dev_pull() {
    su_do "cat $1 > /data/local/tmp/f50-pull.bin" </dev/null >/dev/null
    adb pull /data/local/tmp/f50-pull.bin "$2" >/dev/null 2>&1
    su_do "rm -f /data/local/tmp/f50-pull.bin" </dev/null >/dev/null
}
for f in $(su_do "ls /sys/fs/pstore" | tr -d '\r'); do dev_pull "/sys/fs/pstore/$f" "$OUT/$f"; done
dev_pull /dev/block/by-name/uboot_log "$OUT/uboot_log.raw"
strings -n 6 "$OUT/uboot_log.raw" > "$OUT/uboot_log.txt"
# init writes stages + dmesg to boot_$LS at 48 MiB (8 MiB)
su_do "dd if=/dev/block/by-name/boot_$LS bs=4096 skip=12288 count=2048 2>/dev/null > /data/local/tmp/f50-pull.bin" </dev/null >/dev/null
adb pull /data/local/tmp/f50-pull.bin "$OUT/linux-persist.raw" >/dev/null 2>&1
su_do "rm -f /data/local/tmp/f50-pull.bin" </dev/null >/dev/null
tr -d '\000' < "$OUT/linux-persist.raw" > "$OUT/linux-persist.txt" && rm -f "$OUT/linux-persist.raw"
# (MU300-PERSIST-BEGIN: the marker of an init installed by the upstream project's installer)
sed -E -n '/(F50|MU300)-PERSIST-BEGIN/,/--- lsmod/p' "$OUT/linux-persist.txt" || true
echo "saved: $OUT"
