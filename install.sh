#!/bin/sh
# ZTE F50 Linux installer. Run on a macOS/Linux host with the device booted in rooted Android (adb + su).
#
#   ./install.sh --check         only inspect the device: is the free eMMC region there and empty? (writes nothing)
#
# On the 32 GB variant there is no free region at all (userdata fills the disk). The installer then offers
# to shrink userdata to make one - EXPERIMENTAL, it rewrites the partition table. Two ways (F50_ANS_MAKE_ROOM_METHOD):
# keep - Android keeps its data: its F2FS is shrunk while it runs, by its own kernel, then the table is written and
# Android restarts once (dikeckaan's method, kanoqwq's implementation; offered where /data is an F2FS on all of
# userdata); reset - the table is written and Android's own recovery resets Android, erasing everything in it (that
# works with any encryption). Take a full backup with scripts/device/backup-device.sh first. The installation resumes on the
# next run of this installer (by itself after keep, in a terminal), once Android is up again with USB debugging and
# root (installer/lib/resume.sh; a transaction in ~/f50-backups):
#   ./install.sh --resume        continue the prepared installation, or stop when there is none for this device
#   ./install.sh --no-resume     install as if nothing had been prepared (F50_RESUME=0)
#   ./install.sh --transaction FILE   that transaction, not the newest one for the device (F50_TRANSACTION)
#   ./install.sh                 install OpenWrt on the ZTE F50, built here: the 5.4 vendor kernel of out/
#                                (kernel/vendor/build-all.sh) and the root filesystem built in docker from this copy of
#                                the project (systems/openwrt/build-rootfs.sh), with the vendor files pulled from the device
#   ./install.sh --backup        only copy what an installation reads first to this computer: Android's boot image
#                                and the first block of misc, checked against the device (writes nothing to it)
#   ./install.sh --adb-path      print the adb this installer would use, and stop
#   ./install.sh --answers FILE  ask nothing: every question is answered by its variable, from FILE (NAME=value
#                                lines) or the environment; a question without an answer is an error that names the
#                                variable. F50_PROGRESS=1 adds a machine-readable progress stream. For programs
#                                that drive the installer: installer/lib/answers.sh, docs/guides/INSTALLER-AUTOMATION.md
# The variable of each question, for --answers (a question is asked only where the installer asks it by hand too):
#   F50_LANG=en|zh                       the language
#   ANDROID_SERIAL=<serial>                the adb device, with more than one attached (or F50_ANS_DEVICE=<its
#                                          number in the list)
#   F50_ANS_REBOOT_ANDROID=yes|no        the device runs Linux: reboot it into Android over SSH
#   F50_ANS_UNKNOWN_MODEL=yes|no         this is no ZTE F50: continue anyway
#   F50_STORAGE=internal|sd              with an SD card: where the Linux filesystem goes
#   F50_ANS_INTERNAL_OVER_CARD=internal  internal storage although the card holds an installation
#   F50_ANS_MAKE_ROOM=yes|no             no free space: shrink userdata, then
#   F50_ANS_MAKE_ROOM_METHOD=keep|reset    keep Android's data (asked only where that is possible) or reset it,
#   F50_ANS_LINUX_GIB=<number>             how many GiB for Linux,
#   F50_ANS_SHRINK=yes|no                  with keep: shrink now,
#   F50_ANS_ERASE_ANDROID=ERASE            with reset: the word that allows it, and
#   F50_ANS_REBOOT=yes|no                  reboot afterwards
#   F50_ANS_OVERWRITE=overwrite          the free space holds data: use it anyway
#   F50_ANS_DEFAULT_LINUX=yes|no         boot Linux by default
#   F50_ANS_BOOT_ATTEMPTS=1..6           failed boots before Android (asked with default Linux)
#   F50_ANS_HOTSPOT=yes|no               copy Android's hotspot name and password
#   F50_ANS_GPU=yes|no                   the Mali GPU userspace
#   F50_ANS_ADB=yes|no                   adb to Linux (on for every computer): this computer's key only
#   F50_ANS_EXISTING=update|wipe         Linux is installed already
#   F50_ANS_ERASE_SD=ERASE               a new filesystem on the SD card erases it
#   F50_ANS_KEEP_PASSWORD=yes|no         an update of a system with a login password: keep it (its current password
#                                          is asked and checked on the device) or set the new one below
#   F50_ANS_CURRENT_PASSWORD=<password>  with keep: the current password (empty: none, as OpenWrt's root ships);
#                                          F50_ANS_CURRENT_PASSWORD_OPENWRT takes precedence (F50_ANS_CURRENT_PASSWORD_UBUNTU
#                                          and F50_ANS_CURRENT_PASSWORD_OPENWRT_LUCI, of removed systems, are
#                                          ignored, though still kept secret)
#   F50_ANS_PASSWORD=<password>          for "root" (OpenWrt), 6 characters or more (not asked when an update keeps
#                                          the current password)
#   F50_ANS_CONFIRM=INSTALL              the summary's question; @stdin waits for it on standard input
# The Linux slot gets the boot chain of Android's slot (uboot, sml, teecfg, trustos, vbmeta; backed up first):
# F50_SYNC_BOOTCHAIN=0 skips that.
# An update never goes back to an older release by itself (the device refuses before it writes anything: the older
# release's updater may not carry over everything of the user's): F50_DOWNGRADE=yes allows it.
# It asks for the language (F50_LANG=en|zh skips that).
#
# Build:    needs adb, docker, python3, lz4 and the kernel outputs in $F50_KERNEL_OUT (default: out/, kernel/vendor/build-all.sh).
# The images contain no proprietary files from this repository: Wi-Fi/Bluetooth firmware and the Android modem/GPU
# userspace are pulled from *your* device into $F50_WORK (default: work/), never leave the host except to your device.
set -eu
TOP=$(cd "$(dirname "$0")" && pwd)
KOUT=${F50_KERNEL_OUT:-$TOP/out}
WORK=${F50_WORK:-$TOP/work}
OWRT_VER=25.12.6
T=/data/local/tmp
# the only mode: everything built here (F50_MODE and --build are still accepted and change nothing)
MODE=build
CHECK_ONLY=0; BACKUP_ONLY=0; ADB_PATH_ONLY=0; ANSWERS_FILE=; _opt=
for a in "$@"; do
    if [ "$_opt" = answers ]; then ANSWERS_FILE=$a; _opt=; continue; fi
    if [ "$_opt" = transaction ]; then F50_TRANSACTION=$a; export F50_TRANSACTION; _opt=; continue; fi
    case $a in
        --check) CHECK_ONLY=1 ;;
        --build) MODE=build ;;
        --backup) BACKUP_ONLY=1 ;;
        --adb-path) ADB_PATH_ONLY=1 ;;
        --answers) _opt=answers ;;
        --answers=*) ANSWERS_FILE=${a#--answers=} ;;
        --resume) F50_RESUME=1; export F50_RESUME ;;
        --no-resume) F50_RESUME=0; export F50_RESUME ;;
        --transaction) _opt=transaction ;;
        --transaction=*) F50_TRANSACTION=${a#--transaction=}; export F50_TRANSACTION ;;
        -h|--help) sed -n '2,/^set -eu/{/^set -eu/d;s/^# \{0,1\}//p;}' "$0"; exit 0 ;;
        *) echo "unknown option $a (see --help)" >&2; exit 2 ;;
    esac
done
[ -z "$_opt" ] || { echo "--$_opt needs a file or directory (see --help)" >&2; exit 2; }
. "$TOP/installer/lib/answers.sh"    # say, die, ask; the answers mode and the progress stream
answers_setup                # (before anything reads a F50_* variable: an answers file sets them)
. "$TOP/installer/lib/i18n.sh"
. "$TOP/installer/lib/storage.sh"
. "$TOP/installer/lib/resume.sh"
. "$TOP/installer/lib/host-tools.sh"
if [ $ADB_PATH_ONLY = 1 ]; then find_adb && command -v adb; exit; fi
if [ $ANSWERS_MODE = 1 ]; then
    case $F50_LANG in en|zh) ;; *) die "no answer to the language question: set F50_LANG to en or zh" ;; esac
fi
choose_language
progress hello script install.sh version 1 mode "$([ $CHECK_ONLY = 1 ] && echo check || { [ $BACKUP_ONLY = 1 ] && echo backup || echo $MODE; })"
# su_do: installer/lib/host-tools.sh (a root adb shell as stock F50 has it, or su)

# Never stream a binary through `adb exec-out "su -c ..."`. On some devices su gives the command a pty, and
# the tty layer's ONLCR rewrites every LF as CRLF: the bytes arrive inflated by about one in 256 and nothing
# reports an error. It is silent and it is ruinous - a boot image built from such a dump is the wrong size,
# and Wi-Fi firmware mangled this way loads without complaint and simply never brings up a phy.
#
# Writing the file on the device and pulling it keeps the bytes off any pty. It costs one round trip and is
# correct whether or not that device's su allocates one. Reported with proof in issue #2, on a device whose
# su does; the maintainer's does not, which is why this survived so long.
dev_pull() {  # dev_pull DEVICE_PATH LOCAL_PATH   (DEVICE_PATH may be a block device)
    su_do "cat $1 > /data/local/tmp/f50-pull.bin" >/dev/null
    # (|| true: under set -e a failed pull ended the installer here, without the message below)
    adb pull /data/local/tmp/f50-pull.bin "$2" >/dev/null 2>&1 || true
    su_do 'rm -f /data/local/tmp/f50-pull.bin' >/dev/null
    [ -s "$2" ] || die "$(t 'could not read {1} from the device' "$1")"
}

sha256_of() { (shasum -a 256 "$1" 2>/dev/null || sha256sum "$1" 2>/dev/null) | cut -d' ' -f1; }
# What an installation reads from the device before it writes anything, into $WORK/dumps: Android's own boot image
# (the base of the Linux one: its header, its AVB footer) and the first block of misc (the boot control block).
pull_boot_and_misc() {
    dev_pull /dev/block/by-name/boot_$ANDROID_SLOT "$WORK/dumps/boot_$ANDROID_SLOT.img"
    su_do 'dd if=/dev/block/by-name/misc bs=4096 count=1 2>/dev/null > /data/local/tmp/f50-pull.bin' >/dev/null
    adb pull /data/local/tmp/f50-pull.bin "$WORK/dumps/misc-head.bin" >/dev/null 2>&1 || true
    su_do 'rm -f /data/local/tmp/f50-pull.bin' >/dev/null
    [ -s "$WORK/dumps/misc-head.bin" ] || die "$(t 'could not read the misc header from the device')"
}
# --backup: those two files and nothing else, the boot image compared with the device's. The partitions that hold
# the unit's own identity and calibration (prodnv, the NV partitions) are not read here or anywhere in this script.
backup_only() {
    mkdir -p "$WORK/dumps"
    step backup 30
    say "$(t 'Copying the boot image of Android (boot_{1}) and the first block of misc to this computer' "$ANDROID_SLOT")"
    pull_boot_and_misc
    _bw=$(su_do "sha256sum /dev/block/by-name/boot_$ANDROID_SLOT" | cut -d' ' -f1)
    _bh=$(sha256_of "$WORK/dumps/boot_$ANDROID_SLOT.img")
    [ -n "$_bw" ] && [ "$_bw" = "$_bh" ] || die "$(t 'the copy of boot_{1} is not what the device holds; do not rely on it' "$ANDROID_SLOT")"
    echo "  $WORK/dumps/boot_$ANDROID_SLOT.img  sha256 $_bh"
    echo "  $WORK/dumps/misc-head.bin"
    echo "  $(t 'Not copied, here or during an installation: the NV and calibration partitions (IMEI, radio calibration). The installer never reads or writes them.')"
    echo "  $(t 'Nothing was written to the device.')"
    info backup.boot "$WORK/dumps/boot_$ANDROID_SLOT.img"; info backup.misc "$WORK/dumps/misc-head.bin"; info backup.sha256 "$_bh"
    progress done result ok
    exit 0
}

# ---------------------------------------------------------------- preflight
step preflight 2
say "$(t 'Checking host tools and device')"
need="adb"
[ $CHECK_ONLY = 1 ] || [ $BACKUP_ONLY = 1 ] || need="adb docker python3 lz4"
find_adb || true
for c in $need; do command -v $c >/dev/null || die "$(t '{1} not found' "$c")"; done
if [ $CHECK_ONLY = 0 ]; then
    [ -f "$KOUT/Image" ] && ls "$KOUT"/modules/*.ko >/dev/null 2>&1 || die "$(t 'kernel outputs missing in {1} (run kernel/vendor/build-all.sh)' "$KOUT")"
fi
. "$TOP/installer/lib/linux-mode.sh"
. "$TOP/installer/lib/usb-adb.sh"
require_android
# make adb may have seen Linux before require_android rebooted into Android.
# Installation can prepare the root channel here; read-only modes keep their existing boundary.
if [ $CHECK_ONLY = 0 ] && [ $BACKUP_ONLY = 0 ] && [ "$(su_do 'id -u' 2>/dev/null)" != 0 ]; then
    python3 "$TOP/scripts/device/f50-enable-adb.py" --prepare-root --ip "${F50_IP:-192.168.0.1}" --serial "$ANDROID_SERIAL"
    F50_ROOT_VIA=
fi
adb_prop() { adb shell getprop "$1" </dev/null 2>/dev/null | tr -d '\r' | head -n1; }
# The bootloader's lock is a warning and never a refusal (the owner's decision of 2026-10-13, FINDINGS 71): Linux
# needs a device that boots a modified boot image, and that is an unlocked bootloader or a locked one with a patched
# TrustOS - which no property tells. Without either the device goes back to Android by itself after the install.
# BL_STATE: locked (ro.boot.flash.locked=1 or ro.boot.vbmeta.device_state=locked), unlocked, or unknown.
BL_WARN="$(t 'The bootloader is locked. Did you patch TrustOS? With a patched TrustOS Linux boots on a locked bootloader; without it the device goes back to Android by itself after the install.')"
BL_LOCKED=$(adb_prop ro.boot.flash.locked); BL_DEVICE_STATE=$(adb_prop ro.boot.vbmeta.device_state)
BL_STATE=unknown
case "$BL_LOCKED:$BL_DEVICE_STATE" in 1:*|*:locked) BL_STATE=locked ;; 0:*|*:unlocked) BL_STATE=unlocked ;; esac
# For a program that reads the progress stream: what the device says about itself before anything needs root (the
# model, Android's build, the bootloader's lock and verified-boot state), then whether su works and whose it is.
# Read only, and only for the stream.
if [ -n "$PROGRESS_FD" ]; then
    info device.model "$(adb_prop ro.product.model) / $(adb_prop ro.product.device)"
    info android.build "$(adb_prop ro.build.display.id)"
    info bootloader.locked "$BL_LOCKED"
    info bootloader.device_state "$BL_DEVICE_STATE"
    info bootloader.state "$BL_STATE"
    info bootloader.verified "$(adb_prop ro.boot.verifiedbootstate)"
    if [ "$(su_do 'id -u')" = 0 ]; then
        info root yes; info root.su "$(adb shell su -v </dev/null 2>/dev/null | tr -d '\r' | head -n1)"
    else
        info root no
    fi
fi
[ "$(su_do 'id -u')" = 0 ] || die "$(t 'no root shell on the device: turn on USB debugging first (make adb, or http://192.168.0.1/index.html#usb_port), see README')"
usb_adb_only        # adb alone on USB for this run, no MTP (installer/lib/usb-adb.sh); back at the end while in Android
model="$(su_do 'getprop ro.product.model') / $(su_do 'getprop ro.product.device')"
echo "$(t 'device: {1}' "$model")"
[ $BL_STATE != locked ] || echo "$(t 'WARNING: {1}' "$BL_WARN")"
# Only the ZTE F50. Its Android reports model F50 and device MU300.
# --- model begin
case "$model" in
    *F50*|*f50*|*MU300*|*mu300*) DEVICE=f50 ;;
    *) info device.kind unknown
       ask go "$(t 'This does not look like a ZTE F50. Continue anyway? (yes/no)')" no F50_ANS_UNKNOWN_MODEL; [ "$go" = yes ] || exit 1
       DEVICE=f50; MODEL_KNOWN=0 ;;
esac
# --- model end
[ "${MODEL_KNOWN:-1}" = 0 ] || info device.kind $DEVICE
# --- slot begin
# Linux goes on the slot Android is not on: b next to an Android on a, a next to an Android on b (after an OTA the
# device often runs from b, issue #86) - the rule of the Magisk installer's slot_setup. Android's own boot partition
# is never written: it is the base the Linux image is built from, and the slot the bootloader falls back to.
slot_setup() {
    _ss=$(su_do 'getprop ro.boot.slot_suffix')
    case $_ss in
        _a) ANDROID_SLOT=a; LINUX_SLOT=b ;;
        _b) ANDROID_SLOT=b; LINUX_SLOT=a ;;
        *) die "$(t 'cannot tell which slot Android runs from')" ;;
    esac
    [ "$(su_do "readlink -f /dev/block/by-name/boot_a")" != "$(su_do "readlink -f /dev/block/by-name/boot_b")" ] ||
        die "$(t 'boot_a and boot_b are the same partition')"
    echo "$(t 'Android runs from slot {1}; Linux goes to slot {2} (boot_{2})' "$ANDROID_SLOT" "$LINUX_SLOT")"
}
# write_linux_boot IMAGE: IMAGE to boot_$LINUX_SLOT, verified, then the block beside it (IMAGE without .img +
# .misc-slot-$LINUX_SLOT-trial.bin) to misc, which arms Linux for one trial boot. Before anything is written it
# checks again that Android still runs from the slot it ran from when the image was built, and that the image was
# built for the other one.
write_linux_boot() {
    _wb=${1%.img}
    case $ANDROID_SLOT$LINUX_SLOT in ab|ba) ;; *) die "$(t 'cannot tell which slot Android runs from')" ;; esac
    [ "$(su_do 'getprop ro.boot.slot_suffix')" = "_$ANDROID_SLOT" ] ||
        die "$(t 'Android no longer runs from slot {1}; boot_{2} and misc were not changed' "$ANDROID_SLOT" "$LINUX_SLOT")"
    [ "$(python3 -c "import json,sys;print(json.load(open(sys.argv[1])).get('linux_slot'))" "$_wb.json")" = "$LINUX_SLOT" ] &&
        [ -s "$_wb.misc-slot-$LINUX_SLOT-trial.bin" ] ||
        die "$(t 'the boot image was not built for slot {1}; boot_{1} and misc were not changed' "$LINUX_SLOT")"
    say "$(t 'Writing boot_{1} and arming slot {1}' "$LINUX_SLOT")"
    EXP=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['sha256'])" "$_wb.json")
    adb push "$1" $T/f50-boot.img >/dev/null
    adb push "$_wb.misc-slot-$LINUX_SLOT-trial.bin" $T/f50-bc.bin >/dev/null
    [ "$(su_do "sha256sum $T/f50-boot.img" | cut -d' ' -f1)" = "$EXP" ] || die "$(t 'pushed boot image hash mismatch')"
    su_do "dd if=$T/f50-boot.img of=/dev/block/by-name/boot_$LINUX_SLOT bs=4M && sync"
    # read back from the medium (the page cache dropped first): exactly the image's length in whole 4096-byte blocks,
    # into a file, by one dd whose status counts - a read error leaves no hash to compare instead of a wrong one that
    # looks like anything (FINDINGS 50). A mismatch stops here: misc is not written, Android keeps booting.
    _len=$(wc -c < "$1" | tr -d ' ')
    [ "$(su_do "echo 3 > /proc/sys/vm/drop_caches; dd if=/dev/block/by-name/boot_$LINUX_SLOT of=$T/f50-boot.rb bs=4096 count=$(( (_len + 4095) / 4096 )) 2>/dev/null && head -c $_len $T/f50-boot.rb | sha256sum" | cut -d' ' -f1)" = "$EXP" ] ||
        die "$(t 'boot_{1} verify failed (slot {2} still active, Android keeps booting)' "$LINUX_SLOT" "$ANDROID_SLOT")"
    su_do "dd if=$T/f50-bc.bin of=/dev/block/by-name/misc bs=1 seek=2048 conv=notrunc && sync && rm $T/f50-boot.img $T/f50-bc.bin $T/f50-boot.rb"
}
# --- slot end
slot_setup
info slot.android $ANDROID_SLOT; info slot.linux $LINUX_SLOT; info work "$WORK"
[ $BACKUP_ONLY = 0 ] || backup_only
# --- bootchain begin
# The Linux slot gets the boot chain of Android's slot - uboot, sml, teecfg, trustos and vbmeta, one firmware build on
# both slots (issue #86; installer/android/android-bootchain.sh decides and checks, android-install.sh copies it before it installs
# anything, with a backup). F50_SYNC_BOOTCHAIN=0 skips that, for experts.
bootchain_plan() {
    SYNC_BOOTCHAIN=1; BC_LINE=; BC_REFUSED=
    if [ "${F50_SYNC_BOOTCHAIN:-1}" = 0 ]; then
        SYNC_BOOTCHAIN=0; BC_LINE=$(t 'boot chain:     not synced (F50_SYNC_BOOTCHAIN=0)'); return 0
    fi
    # android-bootchain.sh writes only on a model it knows: a device taken as an F50 anyway keeps its chain as it is
    if [ "${MODEL_KNOWN:-1}" = 0 ]; then
        SYNC_BOOTCHAIN=0; BC_LINE=$(t 'boot chain:     not synced (the model is not recognised)'); return 0
    fi
    adb push "$TOP/installer/android/android-bootchain.sh" $T/android-bootchain.sh >/dev/null 2>&1 || BC_REFUSED=$(t 'no answer from the device')
    bootchain_line "$(su_do "sh $T/android-bootchain.sh check")" && return 0
    if [ $CHECK_ONLY = 1 ]; then BC_LINE="$(t 'boot chain:     cannot be synced: {1}' "$BC_REFUSED")"; return 0; fi
    die "$(t 'the boot chain cannot be copied to the Linux slot: {1}' "$BC_REFUSED")
$(t 'Nothing was changed. Experts can skip this step with F50_SYNC_BOOTCHAIN=0.')"
}
bootchain_pull() {  # the backup android-install.sh made, into the work directory
    [ "$SYNC_BOOTCHAIN" = 1 ] || return 0
    [ "$(su_do "[ -d $T/f50-bootchain-backup ] && echo y")" = y ] || return 0
    _d=$WORK/bootchain/$(date +%Y%m%d-%H%M%S)
    mkdir -p "$WORK/bootchain"
    adb pull $T/f50-bootchain-backup "$_d" >/dev/null 2>&1 || true
    su_do "rm -rf $T/f50-bootchain-backup" >/dev/null
    if [ -s "$_d/SHA256SUMS" ] && (cd "$_d" && { shasum -a 256 -c SHA256SUMS >/dev/null 2>&1 || sha256sum -c SHA256SUMS >/dev/null 2>&1; }); then
        echo "  $(t 'boot chain backup: {1}' "$_d")"
    else
        echo "  $(t 'the boot chain backup could not be copied to this computer; it is on the Linux filesystem (.f50/bootchain)')"
    fi
}
# --- bootchain end
bootchain_plan
echo "$BC_LINE"
info bootchain "$BC_LINE"


# ---------------------------------------------------------------- making room on a small eMMC (EXPERIMENTAL)
# The 32 GB variant ships with userdata filling the disk, so there is no gap behind it and nothing to install
# into. Space can be made by shrinking userdata, and only userdata: it is the **last** partition, so its end
# moves and nothing else does - every other partition keeps its offset, and the AVB descriptors and the boot
# chain stay valid. That is the entire safety argument, and it is why this refuses to touch anything else.
#
# It is still the most dangerous thing this installer can do. It rewrites the partition table and it erases
# everything in Android. Reported and first done by hand in issue #2; treat it as experimental.
offer_repartition() {
    say "$(t 'Not enough free space - this looks like the 32 GB variant')"
    mkdir -p "$WORK/gpt"
    internal_disk
    su_do "dd if=/dev/block/$IDISK bs=512 count=$GPT_HEAD 2>/dev/null > /data/local/tmp/gpt.head" >/dev/null
    su_do "dd if=/dev/block/$IDISK bs=512 skip=$((disk - GPT_TAIL)) count=$GPT_TAIL 2>/dev/null > /data/local/tmp/gpt.tail" >/dev/null
    adb pull /data/local/tmp/gpt.head "$WORK/gpt/gpt.head" >/dev/null 2>&1 || true
    adb pull /data/local/tmp/gpt.tail "$WORK/gpt/gpt.tail" >/dev/null 2>&1 || true
    su_do 'rm -f /data/local/tmp/gpt.head /data/local/tmp/gpt.tail' >/dev/null
    [ -s "$WORK/gpt/gpt.head" ] && [ -s "$WORK/gpt/gpt.tail" ] || die "$(t 'could not read the partition table')"

    # resize-last-partition.py validates both GPT copies and every CRC before it says anything
    info=$(python3 "$TOP/installer/lib/resize-last-partition.py" "$WORK/gpt/gpt.head,$WORK/gpt/gpt.tail" \
             --disk-sectors "$disk" --sector-size "$ILBS" --name userdata --show) || die "$info"
    LAST_FIRST=$(echo "$info" | sed -n 's/^LAST_FIRST=//p')
    LAST_END=$(echo "$info" | sed -n 's/^LAST_END=//p')
    USABLE_END=$(echo "$info" | sed -n 's/^USABLE_END=//p')
    [ -n "$LAST_FIRST" ] && [ -n "$USABLE_END" ] || die "$(t 'could not read the userdata entry from the partition table')"

    total=$((USABLE_END - LAST_FIRST + 1))         # sectors that userdata and Linux have to share
    MIN_ANDROID=$((4 * 1024 * 1024 * 1024 / 512))  # below this Android has nowhere to put apps or updates
    MIN_LINUX=$((800 * 1024 * 1024 / 512))         # OpenWrt alone, with room for one update
    max_linux=$((total - MIN_ANDROID))
    info makeroom.first "$LAST_FIRST"; info makeroom.end "$LAST_END"; info makeroom.usable_end "$USABLE_END"
    info makeroom.min_android $MIN_ANDROID; info makeroom.min_linux $MIN_LINUX; info makeroom.max_linux $max_linux
    [ $max_linux -ge $MIN_LINUX ] || die "$(t 'there is not enough room to split: {1} in total,' "$(gib $((total * 512)))")
$(t 'and Android needs at least {1} of it. Nothing is changed.' "$(gib $((MIN_ANDROID * 512)))")"
    # the device's identity and how Android keeps its data (read only); kano's rule: encrypted, plain, or unknown
    resume_facts || die "$(t 'could not read what identifies this device (serial, eMMC CID, userdata)')"
    [ "$UD_START" = "$LAST_FIRST" ] && [ $((UD_START + UD_SIZE - 1)) = "$LAST_END" ] ||
        die "$(t 'the partition table and the running kernel disagree about userdata; reboot Android and try again')"
    info makeroom.crypto "$CRYPTO"; info makeroom.crypto_state "$CRYPTO_STATE"; info makeroom.crypto_type "$CRYPTO_TYPE"
    # the online way: can Android keep its data? (read only; installer/lib/resume.sh online_inspect)
    online_inspect
    info makeroom.online "$ONLINE"
    [ -z "$ONLINE_REASON" ] || info makeroom.online_reason "$ONLINE_REASON"
    [ "$ONLINE" != yes ] || info makeroom.android_used "$ON_USED"

    echo
    echo "  eMMC:                 $(gib $((disk * 512)))"
    echo "  $(t 'userdata now:         {1} (sectors {2}..{3})' "$(gib $(( (LAST_END - LAST_FIRST + 1) * 512 )))" "$LAST_FIRST" "$LAST_END")"
    echo "  $(t 'free behind it:       {1}' "$(gib $(( (USABLE_END - LAST_END) * 512 )))")"
    echo "  $(t 'to share:             {1}' "$(gib $((total * 512)))")"
    # (not a case inside $(...): macOS's bash 3.2 takes its first ")" for the end of the substitution)
    crypto_text=$(t 'unknown'); [ "$CRYPTO" = yes ] && crypto_text=$(t 'encrypted'); [ "$CRYPTO" = no ] && crypto_text=$(t 'not encrypted')
    echo "  $(t "Android's data:       {1} ({2})" "$crypto_text" "${CRYPTO_STATE:-?}${CRYPTO_TYPE:+/$CRYPTO_TYPE}")"
    echo
    echo "  $(t 'Linux needs at least  {1} (OpenWrt)' "$(gib $((MIN_LINUX * 512)))")"
    echo "  $(t 'Android keeps at least {1}' "$(gib $((MIN_ANDROID * 512)))")"
    echo
    echo "  $(t 'THIS IS EXPERIMENTAL. It rewrites the partition table. Take a full backup first:  sh scripts/device/backup-device.sh')"
    echo "  $(t 'Only userdata changes size; it is the last partition, so nothing else moves.')"
    echo "  $(t 'Two ways:')"
    if [ "$ONLINE" = yes ]; then
        echo "    keep   $(t "Android keeps its data (apps, files, accounts, settings): its filesystem is shrunk while Android runs,")"
        echo "           $(t "by Android's own kernel, then the partition table is written and Android restarts once. {1} of it is" "$(gib "$ON_USED")")"
        echo "           $(t 'in use now. Method: dikeckaan; implementation: kanoqwq.')"
    else
        echo "    keep   $(t "not possible on this device: {1}" "$ONLINE_REASON")"
    fi
    echo "    reset  $(t "ERASES everything in Android: Android's own recovery resets it (a factory reset, a few minutes and")"
    echo "           $(t 'more than one restart). Then set Android up again, turn USB debugging back on and bring root back')"
    echo "           $(t '(Magisk: install the Magisk app again and let Shell have root), and run this installer again.')"
    echo "  $(t 'Either way the installation continues where it was prepared. What was prepared, and a backup of the partition')"
    echo "  $(t 'table, misc, boot and the boot chain, are kept on this computer in {1}.' "$BACKUP_DIR")"
    echo
    if [ $CHECK_ONLY = 1 ]; then
        echo "  $(t '--check writes nothing. Run the installer without it to make room.')"
        progress done result ok
        exit 0
    fi
    # kano's rule for the reset: an encryption state that cannot be told is no ground to stand on
    [ "$ONLINE" = yes ] || [ "$CRYPTO" != unknown ] || die "$(t "Android's encryption state is unknown ({1}); nothing is changed." "${CRYPTO_STATE:-?}/${CRYPTO_TYPE:-?}")"
    ask go "$(t 'Make room by shrinking userdata? (yes/no)')" no F50_ANS_MAKE_ROOM
    [ "$go" = yes ] || die "$(t 'nothing was changed.')"
    METHOD=reset
    if [ "$ONLINE" = yes ]; then
        while :; do
            ask METHOD "$(t 'keep or reset')" keep F50_ANS_MAKE_ROOM_METHOD
            case $METHOD in keep|reset) break ;; esac
            echo "  $(t 'keep or reset')"
        done
    fi
    info makeroom.method "$METHOD"
    [ "$METHOD" = keep ] || [ "$CRYPTO" != unknown ] || die "$(t "Android's encryption state is unknown ({1}); nothing is changed." "${CRYPTO_STATE:-?}/${CRYPTO_TYPE:-?}")"

    while :; do
        ask lin "$(t 'How many GiB for Linux? (the rest stays with Android)')" 4 F50_ANS_LINUX_GIB
        case "$lin" in ''|*[!0-9.]*) echo "  $(t 'give a number, like 4 or 2.5')"; continue ;; esac
        lin_s=$(awk -v g="$lin" 'BEGIN { printf "%d", (g * 1073741824) / 512 }')
        lin_s=$(( (lin_s / 4096) * 4096 ))   # keep the boundary on a 2 MiB alignment
        # userdata ends where the region the installers find behind it (region_probe, resume_region) is lin_s
        # sectors: that region starts one 2 MiB block after userdata's end and stops one block short of the
        # disk's end, so a userdata that ended lin_s before the usable end left a region 4 MiB or more smaller
        # than asked - 0.79 GiB for OpenWrt's 0.78 was 804 MiB, 0.78125 would have been 796 and the resumed run
        # would have refused it after the reset (FINDINGS 112, the first device run)
        new_end=$(( ((disk - ${GPT_TAIL:-33} - 1) / 4096 - 1) * 4096 - lin_s - 4096 - 1 ))
        [ $new_end -le "$USABLE_END" ] || new_end=$((USABLE_END - lin_s))
        resume_region $((new_end + 1))
        if [ $((RG_END - RG_START)) -lt $MIN_LINUX ]; then
            echo "  $(t 'too small: Linux needs at least {1}' "$(gib $((MIN_LINUX * 512)))")"
        elif [ "$lin_s" -gt $max_linux ] || [ $((new_end - LAST_FIRST + 1)) -lt $MIN_ANDROID ]; then
            echo "  $(t 'too large: Android would be left with less than {1}' "$(gib $((MIN_ANDROID * 512)))")"
        elif [ "$METHOD" = keep ] && ! online_target $((new_end - LAST_FIRST + 1)); then
            echo "  $(t 'too large: {1}' "$ON_WHY")"
        else
            break
        fi
    done
    echo
    echo "  $(t 'userdata (Android):   {1}' "$(gib $(( (new_end - LAST_FIRST + 1) * 512 )))")"
    echo "  $(t 'Linux region:         {1}' "$(gib $(( (RG_END - RG_START) * 512 )))")"
    echo
    if [ "$METHOD" = keep ]; then
        echo "  $(t "Android's data:       {1} in use, kept; its filesystem becomes {2}" "$(gib "$ON_USED")" "$(gib $((ON_TARGET * 4096)))")"
        echo
        ask sure "$(t "Shrink userdata now, keeping Android's data? (yes/no)")" no F50_ANS_SHRINK
        [ "$sure" = yes ] || die "$(t 'nothing was changed.')"
    else
        ask sure "$(t "Type ERASE to rewrite the partition table and wipe Android's data")" "" F50_ANS_ERASE_ANDROID
        [ "$sure" = ERASE ] || die "$(t 'nothing was changed.')"
    fi

    # a backup on this computer, before anything is written (kano's backup: the table, misc, boot and the boot chain
    # of both slots, checked against the device)
    step backup 30
    RESUME_BD=$BACKUP_DIR/$(date +%Y%m%d-%H%M%S)-$(printf '%s' "$CID" | cut -c25-32)
    say "$(t 'Backing up to {1}' "$RESUME_BD")"
    resume_backup "$RESUME_BD"
    info resume.backup "$RESUME_BD"
    if [ "$METHOD" = keep ]; then
        online_prepare "$new_end"
        say "$(t 'Done - Android restarts once now, with the smaller userdata, and keeps its data')"
        echo "  $(t 'transaction: {1}' "$RESUME_FILE")"
        ask rb "$(t 'Reboot now? (yes/no)')" yes F50_ANS_REBOOT
        if [ "$rb" != yes ]; then
            progress done result ok repartitioned yes resume pending transaction "$RESUME_FILE"
            echo "  $(t 'Restart Android (adb reboot), then run the installer again: it continues from here.')"
            exit 0
        fi
        _boot=; [ $ANSWERS_MODE = 1 ] || _boot=$(adb shell cat /proc/sys/kernel/random/boot_id </dev/null 2>/dev/null | tr -d '\r')
        adb reboot
        # a program that drives the installer runs it again itself (the graphical installer: Refresh); a person at a
        # terminal is taken on to the installation once Android is up again
        if [ $ANSWERS_MODE = 1 ] || ! online_reboot_wait "$_boot"; then
            progress done result ok repartitioned yes resume pending transaction "$RESUME_FILE"
            echo "  $(t 'Run the installer again once Android is up: it continues from here.')"
            exit 0
        fi
        # the new boot has the user's USB functions again (MTP among them): adb alone once more, and the restore at
        # the end of the run is for this boot (installer/lib/usb-adb.sh)
        usb_adb_only
        return 0
    fi
    resume_prepare "$new_end"

    say "$(t 'Done - the device restarts into the reset now')"
    echo "  $(t "Android's recovery formats Android's data on the next start; then Android starts as new. Set it up,")"
    echo "  $(t 'turn USB debugging on, bring root back (the Magisk app, Shell allowed), and run this installer again.')"
    echo "  $(t 'transaction: {1}' "$RESUME_FILE")"
    ask rb "$(t 'Reboot now? (yes/no)')" yes F50_ANS_REBOOT
    progress done result ok repartitioned yes resume pending transaction "$RESUME_FILE"
    [ "$rb" = yes ] && adb reboot
    exit 0
}

# ---------------------------------------------------------------- free eMMC region
# (before resume_lookup: an interrupted online shrink that resumes prints sizes with it)
gib() { awk -v b="$1" 'BEGIN { printf "%.1f GiB", b / 1073741824 }'; }
step storage 6
say "$(t 'Locating free eMMC space after the last partition')"
region_probe || die "$(t 'could not read the partition table from the device (is su granted? try again)')"
info emmc.sectors "$disk"; info emmc.last_end "$last_end"; info region.offset "$OFF"; info region.size "$SIZE"
# a prepared installation waiting for this device (the make-room path below, on an earlier run): its checks first
resume_lookup
if [ $RESUME = 1 ]; then
    if [ $CHECK_ONLY = 1 ]; then resume_validate soft || RESUME=0; else resume_validate; fi
fi
# the partitions, for a picture of the eMMC: "name:first sector:sectors" of each (the stream only; sysfs, read only)
[ -z "$PROGRESS_FD" ] || info partitions "$(su_do 'for p in /sys/block/'"$IDISK/$IPART"'*; do echo $(sed -n s/^PARTNAME=//p $p/uevent):$(cat $p/start):$(cat $p/size); done' | tr '\n' ' ')"
# What OpenWrt needs: the installed system measures ~320 MiB, and an update keeps the previous one as openwrt.old
# while the new one is unpacked, so allow for two of it plus working room.
NEED_OPENWRT=$((800 * 1024 * 1024))
info need.openwrt $NEED_OPENWRT
echo "$(t 'eMMC: {1} ({2} sectors), partitions end at {3} (sector {4}), free after them: {5}' "$(gib $((disk * 512)))" "$disk" "$(gib $((last_end * 512)))" "$last_end" "$(gib $REG_SIZE)")"
# another installer's layouts (FINDINGS 179): a GPT partition named f50root is where Linux goes (boot/init boots it),
# an image file in Android's data is one this installer cannot update
info region.partition "$IN_PART"
[ "$IN_PART" = 0 ] || echo "$(t 'Linux partition: the GPT partition named f50root, made by another installer, {1} at offset {2}; Linux goes there, the partition table is not changed' "$(gib $SIZE)" "$OFF")"
if _note=$(region_image_note); then echo "$_note"; info region.foreign_image 1; fi
# the SD card as the other place for it (boot/init looks there first)
sd_probe
[ "$SD_SMALL" = 1 ] && echo "$(t 'SD card present but smaller than 700 MiB; not used')"
info sd.device "$SD_DEV"; info sd.bytes "$SD_BYTES"; info sd.small "$SD_SMALL"
if [ $CHECK_ONLY = 1 ]; then
    SD_MODE=0; sd_ex=no
    if [ -n "$SD_DEV" ]; then
        sd_ex=$(sd_existing)
        info sd.state "$sd_ex"
        echo "$(t 'SD card: {1}, {2}, existing f50sd filesystem: {3}' "$SD_DEV" "$(gib "$SD_BYTES")" "$(t "$([ $sd_ex = yes ] && echo yes || echo no)")")"
        [ $sd_ex = foreign ] && echo "  $(t 'the SD card holds another Linux (ext4) filesystem; the installer will not format it')"
    fi
elif [ $RESUME = 1 ]; then
    SD_MODE=0                    # the region that was prepared for it
    info storage internal
else
    choose_storage
    info storage "$([ $SD_MODE = 1 ] && echo sd || echo internal)"
fi
# Smaller eMMC variants leave less room behind userdata, and how much is needed depends on the choice further
# down - OpenWrt fits in a few hundred megabytes. So refuse only what cannot hold anything at all, and
# check the real requirement once the systems are known. There is nowhere else to put this region on these
# devices: userdata is metadata-encrypted (dm-default-key), so an image file inside it cannot be read from
# Linux, and the spare-looking blackbox and fulldumpdb partitions are written by the firmware itself.
if [ $SD_MODE = 0 ] && _why=$(region_part_refusal); then die "$_why"; fi
if [ $SD_MODE = 0 ] && [ $SIZE -lt $((700 * 1024 * 1024)) ]; then
    # --check with a card: a real run offers the card (as the default) and needs no repartitioning
    if [ $CHECK_ONLY = 1 ] && [ -n "$SD_DEV" ] && [ "$sd_ex" != foreign ]; then
        echo "$(t 'result: {1}' "$(t 'too little free eMMC space for Linux; the installer will offer the SD card instead (no repartitioning needed)')")"
        echo; echo "$(t 'Nothing was written. Android version: {1}' "$(su_do 'getprop ro.build.display.id')")"
        info verdict.code sd-only; progress done result ok
        exit 0
    fi
    offer_repartition   # exits, either by installing nothing or by rebooting for a second pass
    # it returns only after the online way in a terminal, with Android up again: the installation goes on from the
    # transaction it made, checked as on a second run
    region_probe || die "$(t 'could not read the partition table from the device (is su granted? try again)')"
    F50_RESUME=1; export F50_RESUME; resume_lookup
    resume_validate
    SD_MODE=0
    info storage internal
fi
region_find_existing || die "$(t 'no answer from the device')"
# what the partition holds is known now: another ext4 in it is not this installer's to format
if [ $SD_MODE = 0 ] && _why=$(region_part_refusal); then die "$_why"; fi
echo "$(t 'Linux region: offset {1}, {2}, existing f50root filesystem: {3}' "$OFF" "$(gib $SIZE)" "$(t "$existing")")"
INTERNAL_EXISTS=0; [ $existing = yes ] && INTERNAL_EXISTS=1; INT_SIZE=
info region.existing "$existing"; info region.offset "$OFF"; info region.size "$SIZE"
if [ $SD_MODE = 1 ]; then
    # from here on SIZE and existing describe the card; OFF and INT_SIZE keep the internal region for the marker
    INT_SIZE=$SIZE; SIZE=$SD_BYTES; existing=$(sd_existing); DIRTY=0
    echo "$(t 'Linux filesystem: SD card {1}, {2}, existing f50sd filesystem: {3}' "$SD_DEV" "$(gib $SIZE)" "$(t "$existing")")"
fi
# unpartitioned space should be unused: sample 16 x 1 MiB across the region and count those with data. Empty is
# 0x00 or 0xFF: an eMMC reads back what its erase leaves (EXT_CSD ERASED_MEM_CONT), and on some F50s that is 0xFF -
# counted as data, a region that was never written stopped the install with "not empty".
DIRTY=0
if [ $SD_MODE = 0 ] && [ $existing = no ] && [ $RESUME = 0 ]; then
    region_dirty || die "$(t 'no answer from the device')"
    echo "$(t 'data check: {1} of 16 samples contain data' "$DIRTY")"
    info region.dirty "$DIRTY"
    # what is there, for a report: the first bytes of the region
    [ "$DIRTY" -gt 0 ] && echo "  $(t 'start of the region: {1}' "$(su_do "dd if=/dev/block/$IDISK bs=1048576 skip=$(( OFF / 1048576 )) count=1 2>/dev/null | od -An -tx1 -N32" | tr -s ' \n' ' ')")"
fi
if [ $RESUME = 1 ]; then
    # (what the region holds is what was userdata, erased with the word ERASE when it was prepared, and the token)
    vcode=resume; verdict=$(t 'OK: the region prepared by making room, checked against its transaction')
elif [ $existing = yes ]; then
    vcode=existing; verdict=$(t 'OK: a F50 Linux installation is already present (it can be kept or replaced)')
elif [ $SD_MODE = 1 ]; then
    vcode=sd; verdict=$(t 'OK: the SD card will be formatted; everything on it is erased')
elif [ "$DIRTY" -gt 0 ] && [ "$IN_PART" = 1 ]; then
    vcode=dirty; verdict=$(t 'WARNING: the partition named f50root is not empty, and it holds no Linux filesystem of this project. Installing overwrites it')
elif [ "$DIRTY" -gt 0 ]; then
    vcode=dirty; verdict=$(t 'WARNING: the unpartitioned space is not empty; it may be used by this firmware. Installing overwrites it')
elif [ $SIZE -ge $((20 * 1024 * 1024 * 1024)) ]; then
    vcode=tested; verdict=$(t 'OK: free and empty, same layout as the tested device (~32 GiB after userdata on the 64 GB eMMC)')
else
    vcode=openwrt; verdict=$(t 'OK: free and empty, smaller than on the tested device; OpenWrt fits')
fi
echo "$(t 'result: {1}' "$verdict")"
info verdict "$verdict"; info verdict.code $vcode
if [ $CHECK_ONLY = 1 ]; then
    echo; echo "$(t 'Nothing was written. Android version: {1}' "$(su_do 'getprop ro.build.display.id')")"
    progress done result ok
    exit 0
fi
if [ "$DIRTY" -gt 0 ]; then
    ask ow "$(t 'Type overwrite to use this region anyway')" no F50_ANS_OVERWRITE
    [ "$ow" = overwrite ] || die "$(t 'cancelled')"
fi

# ---------------------------------------------------------------- choices
step choices 10
say "$(t 'Installing OpenWrt {1} (router, LuCI web UI) with the 5.4 vendor kernel' "$OWRT_VER")"
# the one system, the one kernel: nothing to choose, only whether it fits here
OSES=openwrt; BOOT_OS=openwrt; KERNEL=5.4
[ $SIZE -ge $NEED_OPENWRT ] ||
    die "$(t 'that choice needs about {1} MiB and this device has {2} MiB of free space' "$((NEED_OPENWRT / 1048576))" "$((SIZE / 1048576))")"
ask dl "$(t 'Boot Linux by default instead of Android (falls back to Android if Linux fails)? (yes/no)')" yes F50_ANS_DEFAULT_LINUX
DEFAULT_LINUX=0; [ "$dl" = yes ] && DEFAULT_LINUX=1
BOOT_ATTEMPTS=5
if [ $DEFAULT_LINUX = 1 ]; then
    echo
    echo "  $(t 'How many failed Linux boots in a row before the device goes back to Android by itself?')"
    echo
    echo "  $(t 'A boot counts as failed when it never finishes starting up - the power goes, the battery runs out, or')"
    echo "  $(t 'Linux hangs - before about a minute after power-on. One boot that does finish resets the count.')"
    echo
    echo "  $(t '* Higher is more forgiving: a flaky cable or a couple of power cuts while it is starting will not throw you')"
    echo "  $(t '  back into Android.')"
    echo "  $(t '* Lower gets you to Android sooner if Linux is really broken.')"
    echo "  $(t '* It is also your way back to Android with no computer at hand: cut the power while it is starting this')"
    echo "  $(t '  many times in a row.')"
    echo
    echo "  $(t "1 is how this project used to behave (a single interrupted boot returns to Android). 1-6; the device's")"
    echo "  $(t 'boot counter has no room for more. It can be changed later with: f50-next-boot attempts N')"
    ask BOOT_ATTEMPTS "$(t 'Failed boots before Android (1-6)')" 5 F50_ANS_BOOT_ATTEMPTS
    case $BOOT_ATTEMPTS in [1-6]) ;; *) die "$(t 'enter a number from 1 to 6')" ;; esac
fi
ask hs "$(t "Copy Android's hotspot name and password to Linux? (yes/no)")" yes F50_ANS_HOTSPOT  # kept as-is when updating
IMPORT_HOTSPOT=0; [ "$hs" = yes ] && IMPORT_HOTSPOT=1
ask gpu "$(t 'Include the Mali GPU (OpenCL) userspace (~90 MiB)? (yes/no)')" yes F50_ANS_GPU
# adb over USB to Linux is on in every system and lets every computer in, with a root shell and no password (the
# owner's decision of 2026-10-13, FINDINGS 70; on the device: f50-usb adb auth open|keys, f50-usb adb off). The
# one thing to choose here is to close it to all but this computer from the first boot on: its adb key, which the
# installer runs with anyway, is then the one authorised, and adb takes authorised keys only.
ADB=0 ADB_KEY=
ask ab "$(t "adb to Linux over USB is on and lets every computer in: a root shell without a password for anyone who plugs in a cable. Let in only this computer's adb key (~/.android/adbkey.pub) instead? (yes/no)")" no F50_ANS_ADB
if [ "$ab" = yes ]; then
    ADB_KEY=$(adb_pubkey "${ANDROID_USER_HOME:-$HOME/.android}/adbkey.pub") ||
        die "$(t 'no adb key of this computer to authorise ({1})' "$HOME/.android/adbkey.pub")"
    ADB=1
fi
FORMAT=0; WIPE_LEGACY=0; UPDATE=0
if [ $existing = no ]; then
    FORMAT=1
else
    echo
    echo "  $(t 'A F50 Linux installation is already on this device.')"
    echo "    $(t 'update  install the new release over the systems and keep everything you added or changed in them')"
    echo "    $(t '        (files, settings, accounts; packages you installed come back at the first boot)')"
    echo "    $(t 'wipe    erase the Linux filesystem and install from scratch')"
    ask mode "$(t 'update or wipe')" update F50_ANS_EXISTING
    case $mode in
        update) UPDATE=1 ;;
        wipe) FORMAT=1 ;;
        *) die "$(t 'invalid choice')" ;;
    esac
fi
if [ $SD_MODE = 1 ] && [ $FORMAT = 1 ]; then
    ask erase "$(t 'Everything on the SD card ({1}, {2}) will be erased. Type ERASE to continue' "$SD_DEV" "$(gib $SIZE)")" no F50_ANS_ERASE_SD
    [ "$erase" = ERASE ] || die "$(t 'cancelled')"
fi
# The password. An update keeps the accounts and passwords of the system it replaces when the user says so and shows
# that they know the current one (checked on the device by android-pwcheck.sh, with the hash's own method: MD5- or
# SHA-crypt on OpenWrt; an empty one, OpenWrt's root as it ships, is confirmed by an empty entry).
# Three wrong entries stop the run before anything is written: nothing is changed by a guess, and nothing is set that
# the user did not ask for (run again and answer no to set a new one). The systems installed new, and an update whose
# user says no, get the new password asked for below.
KEEP_PW_OSES=; NEW_PW_OSES=$OSES; pw1=
# What computes the hash on the device (installer/lib/storage.sh, pw_tools_push): the project's own tools, never a
# program of the Linux system being checked (it would run with Android's root). A build here has none: then only an
# empty password can be confirmed (answer no to set one).
PW_BIN= PW_BB=
if [ $UPDATE = 1 ]; then
    pw_have=
    for l in $(pw_states "$OSES" | tr -d '\r'); do
        case ${l#*:} in SET|EMPTY|LOCKED) pw_have="$pw_have ${l%%:*}" ;; esac
    done
    pw_have=${pw_have# }
    if [ -n "$pw_have" ]; then
        echo
        echo "  $(t 'The installed {1} has a login password of its own.' "$pw_have")"
        echo "    $(t 'yes  keep it: the current password is asked once and checked on the device')"
        echo "    $(t 'no   set a new password')"
        ask keep_pw "$(t 'Keep the current password? (yes/no)')" yes F50_ANS_KEEP_PASSWORD
        case $keep_pw in
            yes)
                for os in $pw_have; do
                    pwu=root
                    osv=$(echo "$os" | tr 'a-z-' 'A-Z_')
                    tries=0
                    while :; do
                        tries=$((tries + 1))
                        ask_secret cur "$(t 'Current password of "{1}" in the installed {2} (only Enter if it has none)' "$pwu" "$os")" \
                            "F50_ANS_CURRENT_PASSWORD_$osv" F50_ANS_CURRENT_PASSWORD
                        res=$(pw_check "$os" "$cur" | tr -d '\r' | tail -n1); cur=
                        progress info key "password.$os" value "$res"
                        case $res in
                            MATCH) echo "  $(t 'confirmed: the password of {1} stays as it is' "$os")"; KEEP_PW_OSES="$KEEP_PW_OSES $os"; break ;;
                            NOMATCH) echo "  $(t 'that is not the current password of "{1}" in {2}' "$pwu" "$os")" ;;
                            LOCKED) die "$(t '"{1}" in the installed {2} has no password login, so there is none to confirm. Nothing was changed: run the installer again and answer no to set a new password.' "$pwu" "$os")" ;;
                            *) die "$(t 'the current password of "{1}" in the installed {2} could not be checked on the device ({3}). Nothing was changed: run the installer again and answer no to set a new password.' "$pwu" "$os" "${res:-no answer}")" ;;
                        esac
                        [ $ANSWERS_MODE = 0 ] && [ $tries -lt 3 ] ||
                            die "$(t 'the current password of "{1}" in the installed {2} was not confirmed. Nothing was changed: run the installer again and answer no to set a new password.' "$pwu" "$os")"
                    done
                done ;;
            no) ;;
            *) die "$(t 'invalid choice')" ;;
        esac
    fi
    KEEP_PW_OSES=${KEEP_PW_OSES# }
    NEW_PW_OSES=$(for os in $OSES; do case " $KEEP_PW_OSES " in (*" $os "*) ;; (*) printf '%s ' "$os" ;; esac; done)
    NEW_PW_OSES=${NEW_PW_OSES% }
fi
if [ -n "$NEW_PW_OSES" ]; then
    ask_password pw1 "$(t 'Password for "root" (OpenWrt)')" "$(t 'Repeat')"
    [ ${#pw1} -ge 6 ] || die "$(t 'passwords differ or are shorter than 6 characters')"
fi

# ---------------------------------------------------------------- pull vendor data from the device
mkdir -p "$WORK/dumps" "$WORK/firmware"
step pull 15
say "$(t 'Pulling device data into {1} (stays on this computer)' "$WORK")"
pull_boot_and_misc
# the vendor extractors run as their own processes: hand them the root channel found here (B15 has no su)
root_via || die "$(t 'no answer from the device')"
export F50_ROOT_VIA
# what a complete subset must hold, not only the directory: an interrupted run left a partial one behind that the
# old check then accepted for good (issue #6)
[ -e "$WORK/android-subset/vendor/bin/modem_control" ] && [ -e "$WORK/android-subset/linkerconfig/ld.config.txt" ] ||
    sh "$TOP/android/vendor/extract-subset.sh" "$WORK/android-subset"
for f in wcnmodem.bin gnssmodem.bin wifi_board_config.ini wifi_board_config_ab.ini bt_configure_pskey.ini bt_configure_rf.ini; do
    for d in /odm/firmware /vendor/firmware /vendor/etc; do
        if [ "$(su_do "[ -f $d/$f ] && echo y")" = y ]; then dev_pull "$d/$f" "$WORK/firmware/$f"; break; fi
    done
done
# Nothing downstream notices a firmware file that was never found: the system installs, boots, brings up 5G
# and has no hotspot, with nothing in any log pointing at it. Say so here instead.
missing=
for f in wcnmodem.bin gnssmodem.bin wifi_board_config.ini wifi_board_config_ab.ini bt_configure_pskey.ini bt_configure_rf.ini; do
    [ -s "$WORK/firmware/$f" ] || missing="$missing $f"
done
[ -z "$missing" ] || die "$(t 'these firmware files could not be read from the device:{1}' "$missing")
$(t 'Wi-Fi and Bluetooth need them; without them the system installs and boots but has no hotspot.')"
if [ "$gpu" = yes ] && [ ! -d "$WORK/android-gpu-subset" ]; then
    sh "$TOP/android/vendor/extract-gpu-subset.sh" "$WORK/android-gpu-subset"
fi

# ---------------------------------------------------------------- build
say "$(t 'Building helper binaries')"
mkdir -p "$WORK/out" "$WORK/device-tools/logdw" "$WORK/device-tools/bt-init" "$WORK/device-tools/gpu"
rm -rf "$WORK/out/modules" && cp -R "$KOUT/modules" "$WORK/out/modules"
cp "$KOUT/modules.builtin" "$KOUT/modules.builtin.modinfo" "$WORK/out/" 2>/dev/null || true
docker build -q -t f50-ubuntu:24.04 "$TOP/systems/base" >/dev/null
docker run --rm f50-ubuntu:24.04 cat /bin/busybox > "$WORK/busybox"; chmod +x "$WORK/busybox"
# init's e2fsck for a root filesystem with recorded errors (FINDINGS 182)
sh "$TOP/device-tools/e2fsck/build.sh" "$WORK/device-tools/e2fsck/e2fsck" >/dev/null && E2FSCK=$WORK/device-tools/e2fsck/e2fsck
# adbd (adb to Linux is on by default; a build here takes a few minutes)
sh "$TOP/device-tools/adbd/build.sh" "$WORK/device-tools/adbd/adbd" >/dev/null
docker build -q -t f50-kbuild "$TOP/kernel/vendor" >/dev/null
docker run --rm -v "$TOP/device-tools":/src:ro -v "$WORK/device-tools":/o f50-kbuild sh -c '
  gcc -O2 -static -o /o/logdw/logdw /src/logdw/logdw.c &&
  gcc -O2 -static -o /o/bt-init/f50-bt-init /src/bt-init/f50-bt-init.c &&
  mkdir -p /o/keys && gcc -O2 -static -o /o/keys/f50-keys /src/keys/f50-keys.c &&
  mkdir -p /o/carry && gcc -O2 -static -o /o/carry/f50-carry /src/carry/f50-carry.c'
if [ -d "$WORK/android-gpu-subset" ]; then
    L=$(mktemp -d "$WORK/cllibs.XXXX")
    cp "$WORK/android-gpu-subset/vendor/lib64/libOpenCL.so" "$WORK/android-subset/apex/com.android.runtime/lib64/bionic/libc.so" \
       "$WORK/android-subset/apex/com.android.runtime/lib64/bionic/libdl.so" "$L/"
    docker run --rm -v "$TOP/device-tools/gpu":/w -v "$L":/l:ro -v "$WORK/device-tools/gpu":/o f50-kbuild sh -c \
      'ln -sf /usr/bin/clang-12 /usr/local/bin/clang; cp -r /w /tmp/gpu && sh /tmp/gpu/build.sh /l && cp /tmp/gpu/cltest /o/'
    rm -rf "$L"
fi

# F50_REUSE_BUILD=1 keeps rootfs tarballs from an earlier run of this script
reuse() { [ "${F50_REUSE_BUILD:-0}" = 1 ] && [ -s "$WORK/f50-$1.tar.gz" ] && echo "reusing $WORK/f50-$1.tar.gz"; }
reuse openwrt || {
    say "$(t 'Building the OpenWrt root filesystem')"
    F50_SYSTEM=openwrt F50_INPUTS="$WORK" sh "$TOP/systems/openwrt/build-rootfs.sh" f50-openwrt-rootfs.tar.gz >/dev/null
    mv "$TOP/systems/openwrt/f50-openwrt-rootfs.tar.gz" "$WORK/f50-openwrt.tar.gz"; }
BUSYBOX=$WORK/busybox; LOGDW=$WORK/device-tools/logdw/logdw
PWHASH=; [ -z "$pw1" ] || PWHASH=$(printf '%s' "$pw1" | docker run --rm -i f50-ubuntu:24.04 openssl passwd -6 -stdin)

step bootimage 68
say "$(t 'Building the boot image')"
sed "s/^ROOT_OFFSET=[0-9]*/ROOT_OFFSET=$OFF/" "$TOP/boot/init" > "$WORK/init"
python3 "$TOP/boot/build-boot-image.py" --stock-boot "$WORK/dumps/boot_$ANDROID_SLOT.img" --misc-head "$WORK/dumps/misc-head.bin" \
  --kernel "$KOUT/Image" --modules "$KOUT/modules" --init "$WORK/init" ${BUSYBOX:+--busybox "$BUSYBOX"} --device "$DEVICE" \
  ${LOGDW:+--logdw "$LOGDW"} ${E2FSCK:+--e2fsck "$E2FSCK"} --ueventd-perms "$TOP/android/vendor/ueventd-perms.sh" \
  --android-subset "$WORK/android-subset" --linux-slot "$LINUX_SLOT" --out "$WORK/boot-linux-slot$LINUX_SLOT.img" >/dev/null

# ---------------------------------------------------------------- confirm and install
step confirm 72
say "$(t 'Ready to install')"
echo "  $(t 'source:         {1}' "$(t 'local build')")"
echo "  $(t 'systems:        {1} (boots: {2})' "$OSES" "$BOOT_OS")"
echo "  $(t 'kernel:         {1}' "$KERNEL")"
echo "  $(t 'adb to Linux:   {1}' "$([ "$ADB" = 1 ] && t 'on, only for the adb key of this computer' || t 'on, for every computer: a root shell without a password (on the device: f50-usb adb auth keys, or f50-usb adb off)')")"
echo "  $(t 'default boot:   {1}' "$([ $DEFAULT_LINUX = 1 ] && t 'Linux (Android after {1} failed boots in a row)' "$BOOT_ATTEMPTS" || t 'Android, Linux on demand')")"
echo "  $(t 'filesystem:     {1}' "$([ $FORMAT = 0 ] && t 'keep existing' || { [ $SD_MODE = 1 ] && t 'CREATE new ext4 (erases the SD card)' || t 'CREATE new ext4 (erases the Linux region)'; })")"
[ $UPDATE = 1 ] && echo "  $(t 'update:         everything you added to the chosen systems or changed in them is kept (/etc/f50/update-report)')"
[ -z "$KEEP_PW_OSES" ] || echo "  $(t 'password:       kept as it is in {1} (confirmed){2}' "$KEEP_PW_OSES" "$([ -z "$NEW_PW_OSES" ] || echo "; $(t 'the new one in {1}' "$NEW_PW_OSES")")")"
[ $UPDATE = 0 ] && [ $FORMAT = 0 ] && echo "  $(t 'note:           the chosen systems are installed fresh; their previous files and settings are replaced')"
if [ $SD_MODE = 1 ]; then
    echo "  $(t 'writes:         SD card {1}, boot_{2}, 32 bytes of misc (the eMMC region, boot_{3}, GPT and userdata are not touched)' "$SD_DEV" "$LINUX_SLOT" "$ANDROID_SLOT")"
else
    echo "  $(t 'writes:         Linux region at offset {1}, boot_{2}, 32 bytes of misc (boot_{3}, GPT and userdata are not touched)' "$OFF" "$LINUX_SLOT" "$ANDROID_SLOT")"
fi
echo "  $BC_LINE"
[ $BL_STATE != locked ] || echo "  $(t 'WARNING: {1}' "$BL_WARN")"
ask confirm "$(t 'Type INSTALL to continue')" no F50_ANS_CONFIRM
[ "$confirm" = INSTALL ] || die "$(t 'cancelled')"

step copy 75
say "$(t 'Copying to the device')"
adb push "$TOP/installer/android/android-mount-f50root.sh" "$TOP/installer/android/android-install.sh" "$TOP/installer/android/android-bootchain.sh" $T/ >/dev/null
adb push "$WORK/f50-openwrt.tar.gz" $T/f50-openwrt.tar.gz >/dev/null
env=$(mktemp)
write_install_env > "$env"
# an update to an older release: the device refuses it unless told (installer/android/android-install.sh, the downgrade block)
[ "${F50_DOWNGRADE:-}" != yes ] || echo DOWNGRADE=1 >> "$env"
adb push "$env" $T/f50-install.env >/dev/null; rm -f "$env"
step device 80
say "$(t 'Installing on the device')"
su_do "sh $T/android-install.sh" | tee "$WORK/device-install.log"
# a timed-out SD-card stage (FINDINGS 50) names itself in that log, with the way out (reboot Android first)
grep -q F50-INSTALL-OK "$WORK/device-install.log" ||
    die "$(t 'installation on the device failed; boot_{1} and misc were not changed' "$LINUX_SLOT")
$(t 'the device log was kept at {1}' "$WORK/device-install.log")"
bootchain_pull

step boot 92
write_linux_boot "$WORK/boot-linux-slot$LINUX_SLOT.img"
# the prepared installation is done: its transaction no longer waits (kano: state installed)
[ $RESUME = 0 ] || resume_py set "$RESUME_FILE" state=installed || echo "  $(t 'could not mark {1} as done' "$RESUME_FILE")"

# on-device switch for later: one command in Android instead of plugging into a computer (needs Magisk)
step switch 96
say "$(t 'Installing the on-device switch (Magisk module)')"
# (built from this checkout's android/magisk/f50-linux-switch, nothing downloaded)
sh "$TOP/installer/lib/install-magisk-module.sh" || echo "  $(t '(skipped; the installer keeps working either way)')"

step done 100
say "$(t 'Done. Rebooting into {1}' "$BOOT_OS")"
IP=192.168.77.1
progress done result ok boot_os "$BOOT_OS" systems "$OSES" ip "$IP" kernel "$KERNEL"
echo "  $(t 'USB network: {1}   SSH: {2}' $IP "root@$IP, LuCI http://$IP")"
echo "  $(t 'back to Android: f50-next-boot android')"
echo "  $(t 'back to Linux from Android (with Magisk): su -c f50-linux')"
echo "  $(t 'News and new releases are on the Telegram channel of the project, you can follow it: {1}' https://t.me/f50project)"
adb reboot </dev/null
