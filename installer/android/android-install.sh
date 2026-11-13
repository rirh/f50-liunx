#!/system/bin/sh
# Device side of install.sh (runs as root on Android). Settings come from $T/f50-install.env (T below):
#   OFF SIZE           free eMMC region (bytes) after the last GPT partition, as strings
#   OFF_S SIZE_S       the same in 512-byte sectors (Android's mksh has 32-bit arithmetic: never compute with bytes)
#   SD_MODE=0|1 SD_DEV with SD_MODE=1 the filesystem (label f50sd) is the SD card block device SD_DEV instead of
#                      that region; OFF/SIZE are then not used
#   INTERNAL_EXISTS=0|1  with SD_MODE=1: an internal f50root exists at OFF/SIZE and gets the root-on-sd marker
#   FORMAT=0|1         create the ext4 filesystem (f50root in the region, f50sd on the card)
#   OSES="openwrt"     the system to (re)install from $T/f50-<os>.tar.gz; openwrt is the only one (anything else is
#                      refused before anything is written). An optional f50-vendor-<os>.tar.gz with the device's own
#                      vendor files (the Magisk installer's) is unpacked over it.
#   UPDATE=0|1         keep everything of the user's in the systems being reinstalled (the carry block of the new
#                      image's f50-update; F50_UPDATE_KEEP=list in the environment: only the list of places an
#                      update kept before the first release with a four-field tag)
#   DOWNGRADE=0|1      with UPDATE=1: 1 lets an image of an older release replace a system (see the downgrade block);
#                      not set or 0: that is refused before anything is written
#   BOOT_OS            system started by the initramfs
#   DEFAULT_LINUX=0|1  keep booting Linux (otherwise every Linux boot is one-shot and returns to Android)
#   BOOT_ATTEMPTS=1-6  with DEFAULT_LINUX=1: failed boots in a row before Android (.f50/boot-attempts)
#   PWHASH             SHA-512 crypt hash for OpenWrt's "root" account; empty with
#                      UPDATE=1: the accounts and passwords of the previous installation stay as they are
#   PW_SET=0|1         the Magisk installer: 1 puts PWHASH into that account even when it is empty (OpenWrt's root
#                      without a password, as the image ships it); 0 or not set (install.sh): as above
#   KEEP_PW_OSES="..."  with UPDATE=1: the systems whose accounts and passwords stay exactly as they are (PWHASH and
#                      PW_SET do not touch them); install.sh: those whose current password the user confirmed
#   IMPORT_HOTSPOT=0|1 copy Android's hotspot SSID/passphrase into each system
#   KERNEL=5.4         the kernel in the new boot image (boot/kernel); the vendor 5.4 is the only one, and not set
#                      means 5.4 (anything else is refused before anything is written)
#   ADB=0|1 ADB_KEY    adb over USB to each system: 1 lets in only the adb public key ADB_KEY ("BASE64 name",
#                      etc/f50/adb_keys; "keys" in etc/f50/adb_auth); 0 writes nothing, and a system that was
#                      never told otherwise has adb on for every computer (f50-adbd)
# Environment (not the env file): F50_LOCK_DIR, where the lock of a timed-out SD-card stage lives (default $T;
# the Magisk installer: /data/adb), F50_STAGE_STALL / F50_STAGE_POLL / F50_BOOT_ID, the stage guard's knobs for
# the tests (see the stage block).
set -e
# the systems' directories are made here: never with the modes of a caller whose umask is not this one
umask 022
# install.sh pushes everything to /data/local/tmp. The Magisk installer runs this as root from a directory only root
# can write (F50_DEVICE_WORK): files in /data/local/tmp can be replaced by the shell user after they were checked.
T=${F50_DEVICE_WORK:-/data/local/tmp}
. $T/f50-install.env
# F50 + OpenWrt + the vendor 5.4 kernel only: a request for anything else (an older computer installer's Ubuntu,
# openwrt-luci or a mainline kernel) is refused here, before anything is touched
for _os in $OSES; do
    case $_os in openwrt) ;; *) echo "[device] $_os is not supported (only openwrt); nothing was changed"; exit 1 ;; esac
done
case ${KERNEL:-5.4} in 5.4) ;; *) echo "[device] kernel $KERNEL is not supported (only 5.4); nothing was changed"; exit 1 ;; esac
# --- android-disk begin
# The internal disk, as Android's by-name links have it: the disk that holds misc - mmcblk0 on an eMMC unit, sda on a
# UFS one (issue #396; installer/lib/storage.sh's internal_disk asks the same) - and what its partitions are named after
# (mmcblk0p, sda). mmcblk0 when Android does not say; F50_DISK names it.
_ad=$(readlink -f "${F50_BY_NAME:-/dev/block/by-name}/misc" 2>/dev/null) || _ad=
_ad=$(readlink -f "${F50_SYSFS:-/sys}/class/block/${_ad##*/}/.." 2>/dev/null) || _ad=
IDISK=${F50_DISK:-${_ad##*/}}
case $IDISK in mmcblk[0-9]|sd[a-z]) ;; *) IDISK=mmcblk0 ;; esac
case $IDISK in mmcblk*) IPART=${IDISK}p ;; *) IPART=$IDISK ;; esac
# its logical block size, and its GPT in 512-byte sectors: the first GPT_HEAD and the last GPT_TAIL
ILBS=$(cat "${F50_SYSFS:-/sys}/block/$IDISK/queue/logical_block_size" 2>/dev/null) || ILBS=
case $ILBS in 4096) GPT_HEAD=48 GPT_TAIL=40 ;; *) ILBS=512 GPT_HEAD=34 GPT_TAIL=33 ;; esac
# --- android-disk end
M=$T/f50root
say() { echo "[device] $*"; }
# --- stage begin
# The long stages on an SD card run bounded (FINDINGS 50). A card, or the SD host, that stops answering leaves a
# request in the kernel that never completes: the command sits in D state, kill -9 does nothing, and every retry adds
# another one (38a's dd past the end of the eMMC did the same, one core at 100 % until a reboot). So in SD mode each
# long stage - the format, the unpack of every system, the flush at the end - runs in a session of its own (setsid:
# one process group to end) under a deadline, and is ended when the card's completion counters
# (/sys/class/block/DISK/stat) have not moved for STAGE_STALL seconds. The installer then writes nothing more: no
# sync and no unmount (both would wait on the same request), the card stays mounted, and a lock keyed by Android's
# boot id refuses every run until Android has rebooted. On the eMMC the stages run as they are: Android itself keeps
# the eMMC's counters moving, and the eMMC has not failed this way. (After kanoqwq's TF installer.)
STAGE_DISK=                                # the card's disk (mmcblk1), set in SD mode; empty: a stage runs as it is
STAGE_STALL=${F50_STAGE_STALL:-120}      # seconds without I/O progress that end a stage
STAGE_POLL=${F50_STAGE_POLL:-2}
STAGE_LOCK=${F50_LOCK_DIR:-$T}/f50-install.stalled
STAGE_BOOT_ID=${F50_BOOT_ID:-/proc/sys/kernel/random/boot_id}
STALLED=                                   # set once a stage timed out: sd_exit then flushes and unmounts nothing
boot_id() { _bi=$(cat "$STAGE_BOOT_ID" 2>/dev/null | tr -d ' \n'); echo "${_bi:-unknown}"; }
stall_check() {  # a lock of this Android boot refuses the run; one of an earlier boot is stale and goes
    [ -f "$STAGE_LOCK" ] || return 0
    if [ "$(sed -n 1p "$STAGE_LOCK" 2>/dev/null)" != "$(boot_id)" ]; then rm -f "$STAGE_LOCK"; return 0; fi
    say "an earlier run of the installer timed out in this Android boot ($(sed -n 2p "$STAGE_LOCK")): the SD card request it waited on is still stuck in the kernel"
    say "reboot Android, then run the installer again (nothing was written now; the lock is $STAGE_LOCK)"
    return 1
}
stall_clear() { rm -f "$STAGE_LOCK"; }
stage_stat() {  # the card's completion counters: reads and writes completed, sectors read and written (the time
    # fields keep moving while a request only waits); nothing when the file cannot be read. Always true.
    set -- $(cat "${F50_SYSFS:-/sys}/class/block/$STAGE_DISK/stat" 2>/dev/null)
    [ $# -lt 7 ] || echo "$1 $3 $5 $7"
}
stage_stop() {  # stage_stop PGID: TERM, then KILL, for the whole session (a process in D state ignores both; it is
    # never waited for). dash and busybox take -PGID, bash wants -- first; a shell that takes neither gets the pid.
    kill -TERM -- "-$1" 2>/dev/null || kill -TERM "-$1" 2>/dev/null || kill -TERM "$1" 2>/dev/null || true
    sleep 1
    kill -KILL -- "-$1" 2>/dev/null || kill -KILL "-$1" 2>/dev/null || kill -KILL "$1" 2>/dev/null || true
}
# stage_run NAME LIMIT CMD...: in SD mode CMD in a session of its own, ended after LIMIT seconds, or after
# STAGE_STALL seconds without I/O progress on STAGE_DISK: the timeout is reported, locks, and returns 1. Otherwise
# CMD's status. On the eMMC (STAGE_DISK empty) CMD runs as it is.
stage_run() {
    _sn=$1 _sl=$2; shift 2
    if [ -z "$STAGE_DISK" ]; then "$@"; return; fi
    _sd=$T/f50-stage.$$; rm -rf "$_sd"; mkdir -p "$_sd"
    _sz=$(stage_stat)
    [ -n "$_sz" ] || say "cannot read the I/O counters of $STAGE_DISK: the $_sn stage is bounded by its limit of $_sl s only"
    # the worker: a session of its own where setsid exists (Android's toybox has it), its pid - the group to end - in
    # one file, the command's status in another. It is never waited for before that status is there: a process stuck
    # in the kernel never ends, and a wait on it would hang the installer the way the dd of 38a did.
    if command -v setsid >/dev/null 2>&1; then _ss=setsid; else _ss=; fi
    $_ss sh -c '_d=$1; shift; echo $$ > "$_d/pid"; "$@"; echo $? > "$_d/rc.tmp" && mv "$_d/rc.tmp" "$_d/rc"' sh "$_sd" "$@" &
    _wp=$!
    _t0=$(date +%s); _tl=$_t0; _why=
    while [ ! -f "$_sd/rc" ]; do
        sleep "$STAGE_POLL"
        [ ! -f "$_sd/rc" ] || break
        _now=$(date +%s); _cur=$(stage_stat)
        if [ "$_cur" != "$_sz" ]; then _tl=$_now; _sz=$_cur; fi
        if [ $((_now - _t0)) -ge "$_sl" ]; then _why="it ran longer than $_sl s"
        elif [ -n "$_cur" ] && [ $((_now - _tl)) -ge "$STAGE_STALL" ]; then _why="no I/O progress on $STAGE_DISK for $STAGE_STALL s"
        fi
        [ -z "$_why" ] || break
        # a worker that is gone without its status (ended from outside): a failed stage, not a hang
        _lp=$(cat "$_sd/pid" 2>/dev/null); kill -0 "${_lp:-$_wp}" 2>/dev/null || { sleep 1; [ -f "$_sd/rc" ] || echo 1 > "$_sd/rc"; }
    done
    if [ -n "$_why" ]; then
        stage_stop "$(cat "$_sd/pid" 2>/dev/null || echo "$_wp")"
        STALLED=1
        { boot_id; echo "stage $_sn, $(date '+%Y-%m-%d %H:%M'): $_why"; } > "$STAGE_LOCK" 2>/dev/null || true
        say "the $_sn stage timed out ($_why) and was stopped; nothing more is written"
        say "a request the SD card ($STAGE_DISK) did not answer is stuck in the kernel: reboot Android before running the installer again (until then every run is refused: $STAGE_LOCK)"
        say "the card stays mounted at $M, with no sync and no unmount (both would wait on that request)"
        rm -rf "$_sd"
        return 1
    fi
    _rc=$(cat "$_sd/rc"); wait "$_wp" 2>/dev/null || true
    rm -rf "$_sd"
    return "$_rc"
}
# --- stage end
# --- release-tag begin
# The release tags (docs/research/FINDINGS.md 125): vYYYY.MM.DD up to v2026.10.27, where the "day" was a counter, and from then
# on vYYYY.MM.DD.N: the real (UTC) day the release was built, N = 1..99 for the releases of that day. Every four-field
# tag is newer than every three-field one; four-field tags compare by the date, then by N, as numbers. The same block,
# word for word, is in installer/lib/releases.sh (install.sh, make-release.sh), f50-update and installer/android/android-install.sh
# (tests/test_releases.py holds the copies to each other and to tests/fixtures/release-tags.tsv).
# tag_ok TAG: TAG is a release tag (in the shell's own patterns: no grep, no locale, no regex engine to differ)
tag_ok() {
    case $1 in
        v[0-9][0-9][0-9][0-9].[0-9][0-9].[0-9][0-9]) return 0 ;;
        v[0-9][0-9][0-9][0-9].[0-9][0-9].[0-9][0-9].[1-9]|v[0-9][0-9][0-9][0-9].[0-9][0-9].[0-9][0-9].[1-9][0-9]) ;;
        *) return 1 ;;
    esac
    case ${1#v?????} in 0[1-9].*|1[0-2].*) ;; *) return 1 ;; esac
    case ${1#v????????} in 0[1-9].*|[12][0-9].*|3[01].*) ;; *) return 1 ;; esac
}
# tag_key TAG: a number in the order of the releases (1YYYYMMDD00 for vYYYY.MM.DD, 2YYYYMMDDNN for vYYYY.MM.DD.N);
# nothing and status 1 for what is no release tag
tag_key() {
    tag_ok "$1" || return 1
    case $1 in
        *.*.*.*) _tn=${1##*.}; [ ${#_tn} = 2 ] || _tn=0$_tn; _tk=${1%.*}; _te=2 ;;
        *) _tn=00; _tk=$1; _te=1 ;;
    esac
    _tk=${_tk#v}; _td=${_tk#*.}
    echo "$_te${_tk%%.*}${_td%.*}${_td#*.}$_tn"
}
# newer A B: release A is newer than release B; false when either is no release tag. The key is compared in two
# parts (the era and the date, nine digits; then N without its leading zero), so that no shell's integers overflow
# and none reads 08 as octal.
newer() {
    _na=$(tag_key "$1") && _nb=$(tag_key "$2") || return 1
    [ "${_na%??}" -gt "${_nb%??}" ] && return 0
    [ "${_na%??}" = "${_nb%??}" ] || return 1
    _na=${_na#?????????}; _nb=${_nb#?????????}
    [ "${_na#0}" -gt "${_nb#0}" ]
}
# --- release-tag end
# --- sd begin
sd_ext4() {  # true when the device holds an ext4 filesystem, labelled or not
    [ "$(dd if="$1" bs=1 skip=1080 count=2 2>/dev/null | od -An -tx1 | tr -d ' \n')" = 53ef ]
}
sd_label() {  # the ext4 label of a device, empty when it is not ext4 or has none
    sd_ext4 "$1" || return 0
    dd if="$1" bs=1 skip=1144 count=16 2>/dev/null | tr -d '\000'
}
sd_disk() {  # sd_disk DEV: the disk of a partition (mmcblk1p1 -> mmcblk1); a whole disk stays as it is
    case $1 in *mmcblk[0-9]*p[0-9]*) echo "${1%p*}" ;; *) echo "$1" ;; esac
}
sd_check() {  # sd_check DEV: DEV must be on an SD card, never on the eMMC this Android runs from
    case $1 in
        */"$IDISK"|*/"$IPART"[0-9]*) say "refusing $1: that is the internal eMMC, not the SD card"; return 1 ;;
    esac
    d=$(sd_disk "$1")
    # the MMC core names the card type: SD for a memory card (the eMMC says MMC, a Wi-Fi chip SDIO)
    t=$(cat "${F50_SYSFS:-/sys}/block/${d##*/}/device/type" 2>/dev/null) || t=
    [ "$t" = SD ] || { say "refusing $1: not an SD card (type '$t')"; return 1; }
}
sd_release() {  # sd_release DEV: make Android let go of the card; nothing has been written when this fails
    # no answer from the volume manager is no proof that Android has let go: refuse
    vols=$(sm list-volumes 2>/dev/null) || { say "cannot ask Android about its volumes (sm list-volumes failed)"; return 1; }
    # adopted as internal storage, in any state: encrypted, and part of Android's data. Android's own data
    # partition is the line "private mounted null" (no colon), which is not a card.
    echo "$vols" | grep -q '^private:[^ ]* ' && {
        say "the SD card is adopted as Android internal storage; format it as portable storage first"; return 1; }
    # only the card: public:179,N (179 is the MMC block major); a USB stick (public:8,N) is not ours to touch
    for v in $(echo "$vols" | sed -n 's/^\(public:179,[^ ]*\) mounted.*/\1/p'); do
        say "asking Android to unmount $v"
        sm unmount "$v" 2>/dev/null || true
    done
    vols=$(sm list-volumes 2>/dev/null) || { say "cannot ask Android about its volumes (sm list-volumes failed)"; return 1; }
    if echo "$vols" | grep -q '^public:179,[^ ]* mounted'; then
        say "Android keeps the SD card mounted; eject it under Settings > Storage and run the installer again"; return 1
    fi
    # anything still mounted from the card's own nodes (any of its partitions). vold mounts the card through
    # /dev/block/vold/public:179,N, so this only catches mounts made by hand: the sm check above is the real
    # guard. A whole-card DEV (mmcblk1) stays as it is, so the eMMC (mmcblk0) can never match.
    d=$(sd_disk "$1")
    for m in $(sed -n -e "s|^$d \([^ ]*\) .*|\1|p" -e "s|^${d}p[0-9][0-9]* \([^ ]*\) .*|\1|p" \
            "${F50_MOUNTS:-/proc/mounts}" 2>/dev/null); do
        umount "$m" 2>/dev/null || umount -f "$m" 2>/dev/null || true
    done
}
sd_prepare() {  # sd_prepare DEV FORMAT: create f50sd, or check that it is there
    label=$(sd_label "$1")
    if [ "$2" = 1 ]; then
        # any other ext4, with or without a label, is someone's Linux data; FAT, exFAT or a blank card is not
        if sd_ext4 "$1" && [ "$label" != f50sd ]; then
            say "refusing to format: foreign ext4 (${label:-no label}) on the SD card"; return 1
        fi
        # Android's own mke2fs, not a busybox one that su's PATH may find first. Its default of one inode per
        # 16 KiB is millions of inodes on a big card, and writing their tables is what made 128 GB cards slow and
        # fail (kanoqwq). So fewer: the ratio doubles up to 1 MiB while at least 65536 inodes remain (the two
        # systems, each with an old copy during an update, use about 25000). awk: the byte count is past mksh's
        # 32-bit arithmetic.
        mk=/system/bin/mke2fs; [ -x $mk ] || mk=mke2fs
        s=$(cat "${F50_SYSFS:-/sys}/class/block/${1##*/}/size" 2>/dev/null) || s=0
        i=$(awk -v s="${s:-0}" 'BEGIN { i = 16384; while (i < 1048576 && s * 512 / (i * 2) >= 65536) i *= 2; print i }')
        say "creating ext4 f50sd on $1 (one inode per $((i / 1024)) KiB)"
        # bounded (the stage block); mke2fs's own lines stay quiet inside the stage, the stage's are shown
        stage_run format 900 sh -c 'exec "$@" >/dev/null' sh $mk -t ext4 -F -b 4096 -m 0 -i "$i" -L f50sd "$1"
    elif [ "$label" != f50sd ]; then
        say "no f50sd filesystem on $1 (run with FORMAT=1)"; return 1
    fi
}
# A device with an internal installation as well: tell its init that the card is what should boot (it then waits
# for the card instead of taking the internal system at once). Best effort - the card boots without it whenever
# the SD host is quick enough - so it always returns 0, also under set -e.
sd_mark_internal() {
    [ "${SD_MODE:-0}" = 1 ] && [ -n "${OFF:-}" ] && [ "${INTERNAL_EXISTS:-0}" = 1 ] || return 0
    I=$T/f50root-internal
    F50_OFF=$OFF F50_SIZE=$SIZE sh $T/android-mount-f50root.sh $I >/dev/null 2>&1 || return 0
    # true, not ':': a failed redirection on a special builtin like ':' ends a POSIX shell (dash) outright
    if mkdir -p $I/.f50 2>/dev/null && true > $I/.f50/root-on-sd 2>/dev/null; then
        say "marked the internal installation: the SD card boots first"
    fi
    sync; sh $T/android-mount-f50root.sh -u $I >/dev/null 2>&1 || true
}
# An installation into the internal filesystem: a root-on-sd left there by an earlier card installation no longer
# says what should boot (init would wait for a card first), so it goes.
sd_unmark() {  # sd_unmark ROOT
    [ "${SD_MODE:-0}" = 1 ] || rm -f "$1/.f50/root-on-sd"
}
# The EXIT trap of SD mode: flush and unmount - unless a stage timed out (STALLED): a sync and an unmount would
# both wait on the request that is stuck, and the installer would hang where it was meant to stop.
sd_exit() {
    [ -n "$STALLED" ] || { sync; sh $T/android-mount-f50root.sh -u $M >/dev/null 2>&1; }
    true
}
# --- sd end

stall_check || exit 1
if [ "${SD_MODE:-0}" = 1 ]; then
    [ -b "${SD_DEV:?SD_DEV is not set}" ] || { say "no block device $SD_DEV (is the SD card inserted?)"; exit 1; }
    sd_check "$SD_DEV" || exit 1
    STAGE_DISK=$(sd_disk "$SD_DEV"); STAGE_DISK=${STAGE_DISK##*/}
    sd_release "$SD_DEV" || exit 1
    sd_prepare "$SD_DEV" "$FORMAT" || exit 1
    F50_SD_DEV=$SD_DEV sh $T/android-mount-f50root.sh $M
    trap sd_exit EXIT
else
# --- region-guard begin
# --- the region must not overlap any partition (checked again here, on the device itself), unless it lies wholly
# inside the one GPT partition named f50root: another installer's place for this filesystem (installer/lib/storage.sh's
# region_in_bounds, FINDINGS 179).
end=0; pn=0; ps=0; pz=0
for p in /sys/block/$IDISK/$IPART*; do
    e=$(( $(cat $p/start) + $(cat $p/size) ))
    [ $e -gt $end ] && end=$e
    grep -qx PARTNAME=f50root $p/uevent 2>/dev/null && { pn=$((pn + 1)); ps=$(cat $p/start); pz=$(cat $p/size); }
done
pname=f50root
disk=$(cat /sys/block/$IDISK/size)
if [ "$OFF_S" -ge $end ] && [ $((OFF_S + SIZE_S)) -le $((disk - GPT_TAIL - 1)) ]; then :
elif [ $pn = 1 ] && [ "$OFF_S" -ge $ps ] && [ $((OFF_S + SIZE_S)) -le $((ps + pz)) ]; then
    say "the Linux filesystem goes into the GPT partition named $pname (sectors $ps + $pz)"
else
    say "region overlaps partitions or the backup GPT"; exit 1
fi
# --- region-guard end

attach() {
    for o in /sys/block/loop*/loop/offset; do
        [ "$(cat $o 2>/dev/null)" = "$OFF" ] && { say "region already attached (${o%/loop/offset})"; exit 1; }
    done
    f=$(losetup -f 2>&1 | grep -o '/dev/block/loop[0-9]*' | head -1); n=${f##*loop}
    L=/dev/block/loop$n
    [ -b "$L" ] || mknod "$L" b $(cut -d: -f1 /sys/block/loop$n/dev) $(cut -d: -f2 /sys/block/loop$n/dev) 2>/dev/null || [ -b "$L" ]
    losetup -o $OFF -S $SIZE "$L" /dev/block/$IDISK
    [ "$(cat /sys/block/loop$n/loop/offset)" = "$OFF" ] && [ "$(blockdev --getsize64 $L)" = "$SIZE" ] || { losetup -d $L; say "loop setup mismatch"; exit 1; }
}

sb() { dd if=/dev/block/$IDISK bs=512 skip=$OFF_S count=4 2>/dev/null | dd bs=1 skip=$1 count=$2 2>/dev/null; }
magic=$(sb 1080 2 | od -An -tx1 | tr -d ' \n')
label=$(sb 1144 16 | tr -d '\000')
if [ "$FORMAT" = 1 ]; then
    if [ "$magic" = 53ef ] && [ "$label" != f50root ]; then say "refusing to format: foreign ext4 ($label) in the region"; exit 1; fi
    attach
    say "creating ext4 f50root on $L ($((SIZE_S / 2048)) MiB)"
    mke2fs -t ext4 -L f50root -F "$L" >/dev/null
    losetup -d "$L"
elif [ "$magic" != 53ef ] || [ "$label" != f50root ]; then
    say "no f50root filesystem in the region (run with FORMAT=1)"; exit 1
fi

F50_OFF=$OFF F50_SIZE=$SIZE sh $T/android-mount-f50root.sh $M
trap 'sync; sh $T/android-mount-f50root.sh -u $M >/dev/null 2>&1; true' EXIT
fi

# --- downgrade begin
# Never back to an older release by itself, as f50-update and the Magisk installer: the new image's updater does
# the carrying over, and an older one may not carry everything of the user's (one from before the first release with
# a four-field tag keeps only a list of places). Checked before anything is written. The release of the new image is
# its etc/f50/image-version, read from the tarball ; a system or an image that does not say which release it is
# (a build of the checkout: "dev") is not held back (newer: the release-tag block). DOWNGRADE=1 in the env file
# allows it: install.sh with F50_DOWNGRADE=yes, the Magisk installer after its own check (F50_DOWNGRADE=yes).
image_release() {  # image_release TARBALL: the release the image says it is (nothing when it does not say)
    { tar -xzOf "$1" ./etc/f50/image-version 2>/dev/null || tar -xzOf "$1" etc/f50/image-version 2>/dev/null; } |
        head -n1 || true
}
downgrade_check() {
    [ "${UPDATE:-0}" = 1 ] || return 0
    for _dg in $OSES; do
        [ -d "$M/$_dg" ] || continue
        _dh=$(head -n1 "$M/$_dg/etc/f50/image-version" 2>/dev/null) || _dh=
        _dn=$(image_release "$T/f50-$_dg.tar.gz")
        newer "$_dh" "$_dn" || continue
        if [ "${DOWNGRADE:-0}" = 1 ]; then
            say "$_dg: going back from $_dh to $_dn, an older release (F50_DOWNGRADE=yes)"
            continue
        fi
        say "the installed $_dg is $_dh, and this installation is $_dn, an older release. Going back to an older release is not done by itself: the older release may not carry over everything you added to the system."
        say "nothing was changed. To install it all the same: F50_DOWNGRADE=yes in front of the installer (a release before the first release with a four-field tag then keeps only settings, users and home directories), or wipe to start fresh"
        return 1
    done
}
# --- downgrade end
downgrade_check || exit 1

# --- delta begin
# An update writes only what changed (docs/research/FINDINGS.md 131; f50-update's delta block): the files of the new image,
# and of the device's own files the installers pack (f50-vendor-OS.tar.gz), that the installed system has byte for
# byte become second names of its own instead of being written again, and the room that takes is counted - and
# checked, for every system - before anything is written. The new image's own parts do it (its f50-update, its
# f50-carry, its static busybox), taken out of its tarball into a directory of the Linux filesystem that only root
# can enter; Android's tar unpacks, as before. An image without those parts is unpacked whole, as before.
DELTA_PARTS="./opt/f50/bin/f50-carry ./opt/f50/bin/f50-update ./usr/bin/busybox ./opt/f50/bin/busybox ./etc/f50/image-manifest.gz ./etc/f50/image-version"
# what an installation that was cut off left goes first, so that it is not counted against the room: a half-unpacked
# <os>.new held 98 % of an F50's region until it was removed by hand
for x in openwrt; do
    for d in $M/$x.new $M/.f50-delta-$x $M/$x.new.carry-log; do
        if [ -e $d ]; then
            stage_run "clean $x" 900 rm -rf $d || exit 1
            say "$x: removed ${d##*/}, left by an installation that did not finish"
        fi
    done
done
need=0 whole=
for os in $OSES; do
    [ -f $T/f50-$os.tar.gz ] || continue
    w=$M/.f50-delta-$os
    ( umask 077; mkdir -p $w/root )
    stage_run "look into $os" 1800 sh -c 'tar -xzf "$1" -C "$2" $3 2>/dev/null; true' sh $T/f50-$os.tar.gz $w/root "$DELTA_PARTS" || exit 1
    bb=
    for b in $w/root/usr/bin/busybox $w/root/opt/f50/bin/busybox; do
        if [ -z "$bb" ] && [ -f $b ] && [ ! -L $b ] && [ -x $b ] && $b true 2>/dev/null; then bb=$b; fi
    done
    vt=; if [ -f $T/f50-vendor-$os.tar.gz ]; then vt=$T/f50-vendor-$os.tar.gz; fi
    echo none > $w/room
    # (F50_DELTA=0 in the environment: the whole image is unpacked; the room for that is counted all the same)
    if [ -n "$bb" ] && [ -f "$w/root/opt/f50/bin/f50-update" ]; then
        stage_run "compare $os" 3600 sh -c 'F50_LIB= "$1" sh "$2/root/opt/f50/bin/f50-update" delta-room "$3" "$2" "$4" "$5" > "$2/room.new" && mv "$2/room.new" "$2/room"; true' \
            sh $bb $w $M/$os $os "$vt" || exit 1
    fi
    set -- $(cat $w/room)
    case ${1:-none} in
        delta|full)
            echo $1 > $w/mode
            if [ $1 = full ]; then whole=1; fi
            n=$(( $2 + $2 / 50 + 16384 + ${4:-0} + 8192 ))
            need=$((need + n))
            if [ $1 = delta ]; then
                say "$os: $(( ($3 + 1023) / 1024 )) MB of the new release are in the installed $os as they are and are not written again; $(( ($2 + ${4:-0}) / 1024 )) MB are written"
            fi ;;
        *) echo none > $w/mode ;;
    esac
done
set --
free=$(df -Pk $M | awk 'NR == 2 { print $4 }')
if [ "${free:-0}" -lt $need ]; then
    for os in $OSES; do rm -rf $M/.f50-delta-$os; done
    what="the new release's files that are not in the installed system as they are"
    [ -z "$whole" ] || what="the whole new release, unpacked beside what is installed"
    say "not enough room on the Linux partition: $((need / 1024)) MB needed for $OSES ($what), $(( ${free:-0} / 1024 )) MB free"
    say "nothing was changed"
    exit 1
fi
# unpack_failed: the new system half made goes, and the run stops (nothing of the installed one was changed). After a
# stage that timed out on the SD card nothing more is written (the stage block).
unpack_failed() {
    [ -n "$STALLED" ] || rm -rf $M/$os.new $M/.f50-delta-$os
    say "$os: unpacking the new system failed; $os was not replaced, and nothing of it was changed"
    exit 1
}
# delta_unpack TARBALL LIST: the image (LIST: the files f50-update's delta-room chose) or the device's files ("*":
# every binary the installed system has byte for byte) into $M/$os.new, the files left out made as second names
delta_unpack() {
    _w=$M/.f50-delta-$os
    rm -f $_w/log
    stage_run "unpack $os" 3600 sh -c '"$1/busybox" gzip -dc < "$2" | "$1/carry" tarfilter "$3" "$4" "$1/log" | tar -xpf - -C "$5"' \
        sh $_w $1 $M/$os "$2" $M/$os.new || return 1
    grep -q '^E ' $_w/log && $_w/carry tarlink $M/$os $M/$os.new < $_w/log > $_w/linked
}
# --- delta end

# --- bootchain begin
# The Linux slot's uboot, sml, teecfg, trustos and vbmeta become the ones of Android's slot (android-bootchain.sh),
# before anything else is installed: a failure stops here, and the installers then write no boot image. The images
# the Linux slot held go onto the Linux filesystem first (.f50/bootchain/<date>), and for install.sh
# also to $T/f50-bootchain-backup, which they pull into their work directory.
if [ "${SYNC_BOOTCHAIN:-0}" = 1 ]; then
    bcdir=$M/.f50/bootchain/$(date +%Y%m%d-%H%M%S)
    rm -rf $T/f50-bootchain-backup
    bcout=$(sh $T/android-bootchain.sh sync "$bcdir") && bcrc=0 || bcrc=$?
    echo "$bcout" | grep -v '^BOOTCHAIN-' | sed 's/^/[device] /'
    if [ $bcrc != 0 ]; then
        say "$(echo "$bcout" | sed -n 's/^BOOTCHAIN-[A-Z]*: //p')"
        say "the boot chain was not synced; no system was installed (F50_SYNC_BOOTCHAIN=0 skips this step)"
        exit 1
    fi
    if [ -z "${F50_DEVICE_WORK:-}" ] && [ -d "$bcdir" ]; then
        cp -R "$bcdir" $T/f50-bootchain-backup && chmod -R a+rX $T/f50-bootchain-backup || true
    fi
fi
# --- bootchain end

ssid=; psk=
if [ "$IMPORT_HOTSPOT" = 1 ]; then
    X=/data/misc/apexdata/com.android.wifi/WifiConfigStoreSoftAp.xml
    ssid=$(sed -n 's/.*<string name="WifiSsid">&quot;\(.*\)&quot;<\/string>.*/\1/p; s/.*<string name="WifiSsid">\([^&<]*\)<\/string>.*/\1/p' $X 2>/dev/null | head -1)
    psk=$(sed -n 's/.*<string name="Passphrase">\(.*\)<\/string>.*/\1/p' $X 2>/dev/null | head -1 | sed "s/&amp;/\&/g; s/&lt;/</g; s/&gt;/>/g; s/&quot;/\"/g; s/&apos;/'/g")
    [ -n "$ssid" ] && [ ${#psk} -ge 8 ] || { say "no usable Android hotspot config, a random password will be generated"; ssid=; psk=; }
fi

# --- user-packages begin
# Packages installed on the device are not in the release's image, and an update puts the image in place while it
# keeps /etc/config (OpenWrt) and /etc/f50: their programs were gone and their settings stayed - OpenClash's left
# the device without DNS (#116). So before the switch the update writes down what was added - the packages asked for
# by hand (OpenWrt: /etc/apk/world; Ubuntu: dpkg's packages that apt did not pull in by itself) less the image's
# own set (etc/f50/image-packages, written when the image is built) - into etc/f50/user-packages/ of the new
# system, and sets the UCI files of those packages aside (etc/f50/orphaned-config/): a package that is not there
# leaves no settings behind. At its first boot the new system installs them again (f50-user-packages, a service)
# and puts their settings back. installer/android/android-install.sh (the installers' update) has a copy of this block; tests
# hold the two to one text. It runs on Android too: sh, awk, sed, grep, sort - nothing else, and safe under set -e.
UPK_DIR=etc/f50/user-packages
UPK_ORPH=etc/f50/orphaned-config
UPK_APKS=etc/f50/user-apks
# what is never installed again: kernel modules (the feed's are for another kernel), and what the images take out on
# purpose (sysupgrade would flash the eMMC; ujail crash-loops services on 5.4)
upk_skip() {
    case $1 in
        kmod-*|kernel|procd-ujail|procd-seccomp|luci-app-attendedsysupgrade|attendedsysupgrade-common|owut) return 0 ;;
        linux-image-*|linux-modules-*|linux-headers-*|linux-generic*|linux-firmware) return 0 ;;
    esac
    return 1
}
upk_kind() { case $1 in ubuntu) echo ubuntu ;; *) echo openwrt ;; esac; }
# upk_dpkg ROOT [manual]: dpkg's installed packages in ROOT; with manual, less those apt installed as dependencies
# (what apt-mark showmanual says, read from the files: ROOT need not be the running system)
upk_dpkg() {
    [ -f "$1/var/lib/dpkg/status" ] || return 0
    { [ "${2:-}" != manual ] || cat "$1/var/lib/apt/extended_states" 2>/dev/null || :; echo '@dpkg'; cat "$1/var/lib/dpkg/status"; } |
        awk '$0 == "@dpkg" { s = 1; next }
             $1 == "Package:" { p = $2; next }
             !s && $1 == "Auto-Installed:" && $2 == 1 { auto[p] = 1; next }
             s && $1 == "Status:" && $3 == "ok" && $4 == "installed" { inst[p] = 1 }
             END { for (p in inst) if (!(p in auto)) print p }' | sort
}
# upk_manual ROOT KIND: the packages asked for by name (OpenWrt: the world file, versions and pins taken off)
upk_manual() {
    if [ "$2" = ubuntu ]; then upk_dpkg "$1" manual; return 0; fi
    [ -f "$1/etc/apk/world" ] || return 0
    sed 's/[<>=~@ ].*//' "$1/etc/apk/world" | awk 'NF' | sort -u
}
# upk_installed ROOT KIND: every installed package
upk_installed() {
    if [ "$2" = ubuntu ]; then upk_dpkg "$1"; return 0; fi
    [ -f "$1/lib/apk/db/installed" ] || return 0
    sed -n 's/^P://p' "$1/lib/apk/db/installed" | sort -u
}
# upk_txt_names FILE: the names in etc/f50/packages.txt ("apk list --installed": name-version-rN arch ...)
upk_txt_names() {
    [ -f "$1" ] || return 0
    awk '{print $1}' "$1" | sed 's/-r[0-9][0-9]*$//; s/-[^-]*$//'
}
# upk_pending DIR: the packages of a record not installed again yet (nor given up on)
upk_pending() {
    [ -f "$1/wanted" ] || return 0
    { for _uf in done failed; do [ ! -f "$1/$_uf" ] || sed 's/^/x /' "$1/$_uf"; done; sed 's/^/w /' "$1/wanted"; } |
        awk '$1 == "x" { d[$2] = 1; next } NF > 1 && !($2 in d) { print $2 }'
}
# What belongs to the new image, not to the device, in the directories an update keeps: its version and package
# lists and its manifest in etc/f50 (the old ones, carried over, made every update report the version it replaced), and
# the names of its own files in etc/config (never set aside). image_own_save NEW before the settings are copied,
# image_own_restore NEW after; upk_record removes what they keep in NEW/.f50-image.
image_own_save() {
    rm -rf "$1/.f50-image"; mkdir -p "$1/.f50-image" || return 1
    for _uf in image-version image-packages packages.txt image-manifest.gz; do
        [ ! -f "$1/etc/f50/$_uf" ] || cp "$1/etc/f50/$_uf" "$1/.f50-image/$_uf" || return 1
    done
    for _uf in "$1"/etc/config/*; do [ ! -e "$_uf" ] || echo "${_uf##*/}"; done > "$1/.f50-image/config"
}
image_own_restore() {
    [ -d "$1/.f50-image" ] || return 0
    mkdir -p "$1/etc/f50" || return 1
    for _uf in image-version image-packages packages.txt image-manifest.gz; do
        if [ -f "$1/.f50-image/$_uf" ]; then cp "$1/.f50-image/$_uf" "$1/etc/f50/$_uf" || return 1
        # a list of the old image would describe the wrong one (the version stays: "unknown" says less)
        elif [ "$_uf" != image-version ]; then rm -f "$1/etc/f50/$_uf"; fi
    done
}
# upk_os_codename ROOT: Ubuntu's release name (noble, ...)
upk_os_codename() {
    sed -n 's/^VERSION_CODENAME=//p' "$1/usr/lib/os-release" 2>/dev/null | tr -d '"' | head -n 1
}
# upk_apt_keep OLD NEW [checked]: Ubuntu keeps only parts of /etc, and apt's sources were not among them: a PPA's
# packages could not come back. The source files added in etc/apt/sources.list.d are copied where the new image
# has no file of that name (the image's own are never replaced), with the keys they need: one in etc/apt/keyrings
# or usr/share/keyrings that a kept source names, and etc/apt/trusted.gpg.d's - each only where the new image has
# no file of that name. checked: the first boot tries apt-get update and disables a kept source that makes it fail
# (f50-user-packages); without it (nothing to install again) a source of another Ubuntu release (24.04 -> 26.04)
# is copied disabled (NAME.disabled, which apt skips) and said so. NEW/.f50-image/apt-kept: the sources kept on.
upk_apt_keep() {
    _ua=$2/.f50-image/apt-kept _ul=
    : > "$_ua" || return 1
    _uoc=$(upk_os_codename "$1") _unc=$(upk_os_codename "$2")
    for _uf in "$1"/etc/apt/sources.list.d/*.list "$1"/etc/apt/sources.list.d/*.sources; do
        [ -f "$_uf" ] || continue
        _ur=etc/apt/sources.list.d/${_uf##*/}
        { [ ! -e "$2/$_ur" ] && [ ! -L "$2/$_ur" ]; } || continue
        mkdir -p "$2/etc/apt/sources.list.d" || return 1
        if [ -z "${3:-}" ] && [ "$_uoc" != "$_unc" ]; then
            cp -p "$_uf" "$2/$_ur.disabled" || return 1
            echo "  apt: /$_ur is for Ubuntu $_uoc, this is $_unc: kept disabled (/$_ur.disabled)"
        else
            cp -p "$_uf" "$2/$_ur" || return 1
            echo "$_ur" >> "$_ua"
            echo "  apt: /$_ur kept"
        fi
        _ul="$_ul $_uf"
    done
    [ -n "$_ul" ] || return 0
    # their pinning (etc/apt/preferences.d), where the new image has no file of that name; a pin whose source is
    # turned off later does nothing on its own (f50-user-packages status names it). NEW/.f50-image/apt-prefs.
    : > "$2/.f50-image/apt-prefs" || return 1
    for _uf in "$1"/etc/apt/preferences.d/*; do
        [ -f "$_uf" ] || continue
        _ur=etc/apt/preferences.d/${_uf##*/}
        { [ ! -e "$2/$_ur" ] && [ ! -L "$2/$_ur" ]; } || continue
        mkdir -p "$2/etc/apt/preferences.d" && cp -p "$_uf" "$2/$_ur" || return 1
        echo "$_ur" >> "$2/.f50-image/apt-prefs"
        echo "  apt: /$_ur kept"
    done
    # shellcheck disable=SC2086
    _us=$(cat $_ul 2>/dev/null)
    for _uf in "$1"/etc/apt/keyrings/* "$1"/usr/share/keyrings/* "$1"/etc/apt/trusted.gpg.d/*; do
        [ -f "$_uf" ] || continue
        _ur=${_uf#"$1"/}
        { [ ! -e "$2/$_ur" ] && [ ! -L "$2/$_ur" ]; } || continue
        case $_ur in
            etc/apt/trusted.gpg.d/*) ;;
            *) printf '%s\n' "$_us" | grep -qF "/$_ur" || continue ;;
        esac
        mkdir -p "$2/${_ur%/*}" && cp -p "$_uf" "$2/$_ur" || return 1
    done
}
# upk_units OLD: "PACKAGE UNIT enabled|disabled|masked" for the systemd units the packages named on stdin have in
# OLD (dpkg's file lists), as OLD had them - f50-user-packages puts that back after it installs them again
upk_units() {
    while read -r _up; do
        for _ul in "$1/var/lib/dpkg/info/$_up.list" "$1/var/lib/dpkg/info/$_up":*.list; do
            [ -f "$_ul" ] || continue
            grep -E '^/(usr/)?lib/systemd/system/[A-Za-z0-9_.:-]+\.(service|socket|timer|path)$' "$_ul" |
                while read -r _uu; do
                    # an alias (a link to the unit, smartd.service) follows its unit
                    [ ! -L "$1$_uu" ] || continue
                    _uu=${_uu##*/}
                    case $_uu in *@*) continue ;; esac
                    _us=disabled
                    for _uw in "$1"/etc/systemd/system/*.wants/"$_uu" "$1"/etc/systemd/system/*.requires/"$_uu"; do
                        [ ! -L "$_uw" ] || _us=enabled
                    done
                    [ "$(readlink "$1/etc/systemd/system/$_uu" 2>/dev/null)" != /dev/null ] || _us=masked
                    echo "$_up $_uu $_us"
                done
        done
    done | sort -u
}
# upk_record OLD NEW OS: after the settings are in NEW, before the switch. Writes NEW/etc/f50/user-packages/
# (wanted: the names; files: "package path" of their files in etc/; rc.d: the services enabled in OLD; from; state)
# and moves their etc/config files out of the way. F50_KEEP_PACKAGES=0: they are not installed again (state
# skipped; f50-user-packages retry does it later), their settings are set aside all the same.
upk_record() {
    _uo=$1 _un=$2 _uk=$(upk_kind "$3")
    _ut=$2/.f50-image _ud=$2/$UPK_DIR
    mkdir -p "$_ut" || return 1
    # the old image's own set; a system from before that list: the new image's and the one packages.txt names
    if [ -s "$_uo/etc/f50/image-packages" ]; then cat "$_uo/etc/f50/image-packages"
    else
        cat "$_ut/image-packages" 2>/dev/null || :
        upk_manual "$_un" "$_uk"
        upk_txt_names "$_uo/etc/f50/packages.txt"
    fi > "$_ut/not"
    upk_installed "$_un" "$_uk" >> "$_ut/not"
    # what an earlier update recorded and the device never got back (offline since, or skipped) stays wanted
    { upk_manual "$_uo" "$_uk"; upk_pending "$_uo/$UPK_DIR"; } > "$_ut/had"
    { sed 's/^/x /' "$_ut/not"; sed 's/^/w /' "$_ut/had"; } |
        awk '$1 == "x" { n[$2] = 1; next } NF > 1 && !($2 in n) { print $2 }' | sort -u > "$_ut/added"
    rm -rf "$_ud"
    : > "$_ut/wanted"
    while read -r _up; do upk_skip "$_up" || echo "$_up" >> "$_ut/wanted"; done < "$_ut/added"
    # Ubuntu: the apt sources added on the device (a PPA's packages cannot come back without it), see upk_apt_keep
    if [ "$_uk" = ubuntu ]; then
        upk_apt_keep "$_uo" "$_un" "$([ -s "$_ut/wanted" ] && echo checked)" || return 1
    fi
    if [ ! -s "$_ut/wanted" ]; then
        echo "  none: everything installed is part of the new image"
        rm -rf "$_ut"
        return 0
    fi
    mkdir -p "$_ud" || return 1
    mv "$_ut/wanted" "$_ud/wanted"
    [ ! -s "$_ut/apt-kept" ] || mv "$_ut/apt-kept" "$_ud/apt-sources"
    [ ! -s "$_ut/apt-prefs" ] || mv "$_ut/apt-prefs" "$_ud/apt-prefs"
    : > "$_ud/done"; : > "$_ud/failed"
    cat "$_uo/etc/f50/image-version" > "$_ud/from" 2>/dev/null || echo unknown > "$_ud/from"
    # their files in etc/, from the package database of OLD (and of the record a pending one came from)
    if [ "$_uk" = openwrt ]; then
        { cat "$_uo/$UPK_DIR/files" 2>/dev/null || :
          [ ! -f "$_uo/lib/apk/db/installed" ] || awk '
            $0 ~ /^P:/ { p = substr($0, 3); next }
            $0 ~ /^F:/ { d = substr($0, 3); next }
            $0 ~ /^R:/ && (d == "etc" || d ~ /^etc\//) { print p " " d "/" substr($0, 3) }' "$_uo/lib/apk/db/installed"
        } | { sed 's/^/w /' "$_ud/wanted"; sed 's/^/f /'; } |
            awk '$1 == "w" { w[$2] = 1; next } ($2 in w) && NF > 2 { print substr($0, 3) }' | sort -u > "$_ud/files"
        for _uf in "$_uo"/etc/rc.d/*; do { [ -L "$_uf" ] || [ -e "$_uf" ]; } && echo "${_uf##*/}"; done > "$_ud/rc.d" || :
    else
        : > "$_ud/files"; : > "$_ud/rc.d"
        upk_units "$_uo" < "$_ud/wanted" > "$_ud/units"
    fi
    if [ "${F50_KEEP_PACKAGES:-1}" = 0 ]; then echo skipped > "$_ud/state"; else echo pending > "$_ud/state"; fi
    echo "  $(tr '\n' ' ' < "$_ud/wanted")"
    # a package the feeds do not have, coming back from the file kept for it before the update (f50-user-packages keep)
    [ ! -f "$_uo/$UPK_APKS/index" ] || while read -r _kp _kv _kf; do
        [ ! -f "$_uo/$UPK_APKS/$_kf" ] || ! grep -qxF "$_kp" "$_ud/wanted" || echo "  from a kept file: /$UPK_APKS/$_kf"
    done < "$_uo/$UPK_APKS/index"
    if [ "$(cat "$_ud/state")" = skipped ]; then
        echo "  The new system does not have them, and they are not installed again (F50_KEEP_PACKAGES=0 or"
        echo "  --no-reinstall; later: f50-user-packages retry)."
    else
        echo "  The new system does not have them: its first boot installs them again once the device is online"
        echo "  (the result: f50-user-packages status)."
    fi
    # their UCI files: a package that is not there must leave nothing behind that acts on the network. Only a file
    # its package owns, in etc/config, that the new image does not have itself (dhcp, firewall, luci: never).
    _um=
    while read -r _up _uf; do
        case $_uf in etc/config/?*) ;; *) continue ;; esac
        { [ -f "$_un/$_uf" ] && [ ! -L "$_un/$_uf" ]; } || continue
        ! grep -qxF "${_uf#etc/config/}" "$_ut/config" 2>/dev/null || continue
        mkdir -p "$_un/$UPK_ORPH/etc/config" && mv -f "$_un/$_uf" "$_un/$UPK_ORPH/$_uf" || return 1
        [ -n "$_um" ] || echo "  Their settings are set aside until they are back, so that they cannot act without them:"
        echo "    /$_uf -> /$UPK_ORPH/$_uf ($_up)"
        _um=1
    done < "$_ud/files"
    rm -rf "$_ut"
}
# stale_config_clean NEW: what the images of v2026.10.18 left in etc/config and an update then copied onto every
# device (the config files of the image are added to the kept ones): attendedsysupgrade, of a package the images
# remove on purpose, and the .apk-new copies apk leaves beside a changed config. Gone from NEW's etc/config unless
# the package is installed there (attendedsysupgrade stays with attendedsysupgrade-common or its LuCI app).
stale_config_clean() {
    for _uf in "$1"/etc/config/*.apk-new; do [ ! -e "$_uf" ] || rm -f "$_uf"; done
    [ -e "$1/etc/config/attendedsysupgrade" ] || return 0
    upk_installed "$1" openwrt | grep -qx 'attendedsysupgrade-common\|luci-app-attendedsysupgrade' && return 0
    rm -f "$1/etc/config/attendedsysupgrade"
}
# --- user-packages end
# --- install-os begin
# Accounts are settings too, and etc/shadow is not in keep: an update carries them over the way f50-update does,
# with its own function (copied here: this runs on Android and cannot source the new system's f50-update; tests
# keep the two the same). The old entry of every account both systems have (hashes, group memberships merged), and
# the users and groups the old system added (ids 1000-59999). A new password (PWHASH) is put in afterwards.
ACCOUNTS_MARK=/etc/.f50-accounts-from-image
merge_accounts() {  # merge_accounts OLD_ROOT NEW_ROOT   ("" is the running system)
    o=$1 n=$2
    [ -f "$o/etc/shadow" ] && [ -f "$n/etc/shadow" ] || return 0
    for f in passwd group shadow gshadow; do
        [ -f "$o/etc/$f" ] && [ -f "$n/etc/$f" ] || continue
        awk -F: -v OFS=: -v f="$f" -v op="$o/etc/passwd" -v og="$o/etc/group" '
            BEGIN {
                while ((getline l < op) > 0) { split(l, a, ":"); if (a[3] >= 1000 && a[3] < 60000) lu[a[1]] = 1 }
                while ((getline l < og) > 0) { split(l, a, ":"); if (a[3] >= 1000 && a[3] < 60000) lg[a[1]] = 1 }
            }
            NR == FNR { old[$1] = $0; next }
            {
                seen[$1] = 1
                if ((f == "shadow" || f == "gshadow") && ($1 in old)) { print old[$1]; next }
                if (f == "group" && ($1 in old)) {
                    split(old[$1], og4, ":"); m = $4; k = split(og4[4], om, ",")
                    for (i = 1; i <= k; i++)
                        if (om[i] != "" && index("," m ",", "," om[i] ",") == 0) m = (m == "" ? om[i] : m "," om[i])
                    $4 = m
                }
                print
            }
            END {
                for (x in old) if (!(x in seen)) {
                    if ((f == "passwd" || f == "shadow") && (x in lu)) print old[x]
                    if ((f == "group" || f == "gshadow") && (x in lg)) print old[x]
                }
            }' "$o/etc/$f" "$n/etc/$f" > "$n/etc/$f.f50" || { rm -f "$n/etc/$f.f50"; return 1; }
        # renamed into place, never rewritten: a crash (or a reader at boot) never sees a half-written passwd.
        # The copy made with cp -p keeps the file's owner and mode (shadow is root:shadow 0640; no stat on OpenWrt).
        if [ -s "$n/etc/$f.f50" ]; then
            cp -p "$n/etc/$f" "$n/etc/$f.f50-new" && cat "$n/etc/$f.f50" > "$n/etc/$f.f50-new" && sync &&
                mv -f "$n/etc/$f.f50-new" "$n/etc/$f" || { rm -f "$n/etc/$f.f50" "$n/etc/$f.f50-new"; return 1; }
        fi
        rm -f "$n/etc/$f.f50"
    done
    rm -f "$n$ACCOUNTS_MARK"
}
for os in $OSES; do
    tarball=$T/f50-$os.tar.gz
    [ -f $tarball ] || { say "missing $tarball"; exit 1; }
    say "installing $os"
    # an earlier run that died between moving the old system aside and putting the new one in place: the old one
    # is the installed system again (its files may be the only names of the user's data)
    if [ ! -d $M/$os ] && [ -d $M/$os.broken ]; then
        mv $M/$os.broken $M/$os
        say "$os: an earlier installation was cut off; the system it was replacing is back in place"
    fi
    # the writes to the card run bounded (the stage block); on the eMMC as they are
    stage_run "clean $os" 900 rm -rf $M/$os.new
    mkdir $M/$os.new
    # only what changed is written where the delta block could compare (its mode file says)
    dmode=$(cat $M/.f50-delta-$os/mode 2>/dev/null || echo none)
    if [ "$dmode" = delta ]; then
        delta_unpack $tarball $M/.f50-delta-$os/link || unpack_failed
    else
        stage_run "unpack $os" 3600 tar -xzpf $tarball -C $M/$os.new || unpack_failed
    fi
    # the account the installer gives a password (OpenWrt's root)
    pwu=root
    # KEEP_PW_OSES: the systems whose accounts and passwords this update keeps as they are, whatever PWHASH says (the
    # user chose to keep them and confirmed the current password, or a Magisk zip was told nothing): only with a
    # previous system to keep them from
    pwkeep=0
    case " ${KEEP_PW_OSES:-} " in *" $os "*) if [ "${UPDATE:-0}" = 1 ] && [ -f "$M/$os/etc/shadow" ]; then pwkeep=1; fi ;; esac
    # firmware and Android userspace of this device, when an installer pushed them (the Magisk installer's
    # vendor_overlay, android-boot-image.sh); a locally built image has them inside
    if [ -f $T/f50-vendor-$os.tar.gz ]; then
        if [ "$dmode" = delta ]; then
            delta_unpack $T/f50-vendor-$os.tar.gz '*' || unpack_failed
        else
            stage_run "unpack $os vendor" 900 tar -xzpf $T/f50-vendor-$os.tar.gz -C $M/$os.new || unpack_failed
        fi
        rm -f $T/f50-vendor-$os.tar.gz
    fi
    rm -rf $M/.f50-delta-$os
    # update: everything of the user's in the previous installation goes into the new system. The new image brings
    # what does it (f50-update's carry block, f50-carry, a static busybox: docs/research/FINDINGS.md 85) and it is run from
    # there, by that busybox - one implementation for f50-update apply, install.sh and the Magisk zip, with nothing
    # of it copied here and none of Android's own tools in it (toybox's cp breaks hard links and makes sparse files
    # dense). By link: the old system is not kept on this path, so its files become the new system's without a copy,
    # which needs no room. F50_UPDATE_KEEP=list (environment), or an image without those parts: the update as it
    # was, keeping the list of places below.
    if [ "${UPDATE:-0}" = 1 ] && [ -d $M/$os ]; then
        carried=0
        cbb=
        for b in $M/$os.new/usr/bin/busybox $M/$os.new/opt/f50/bin/busybox; do
            if [ -z "$cbb" ] && [ -f $b ] && [ ! -L $b ] && [ -x $b ]; then cbb=$b; fi
        done
        if [ "${F50_UPDATE_KEEP:-all}" != list ] && [ -n "$cbb" ] && [ -x $M/$os.new/opt/f50/bin/f50-carry ] &&
            grep -q '^# --- carry begin' $M/$os.new/opt/f50/bin/f50-update 2>/dev/null && $cbb true 2>/dev/null; then
            say "carrying everything of yours in the previous $os over"
            crc=0
            rm -f $M/$os.new/.f50-carried
            # its output names the user's files: kept on the Linux filesystem, which is root's, for the moment it takes
            # to pass it on - never in $T (/data/local/tmp belongs to the shell user, who could plant a link there)
            clog=$M/$os.new.carry-log
            rm -rf $clog
            ( umask 077; F50_LIB='' $cbb sh $M/$os.new/opt/f50/bin/f50-update carry $M/$os $M/$os.new $os link > $clog 2>&1 ) || crc=$?
            while IFS= read -r l; do say "$l"; done < $clog
            rm -f $clog
            # done is what the block says at its very end, with this file: a run without it did not do the work
            if [ $crc = 0 ] && [ ! -e $M/$os.new/.f50-carried ]; then crc=1; fi
            rm -f $M/$os.new/.f50-carried
            if [ $crc != 0 ]; then
                rm -rf $M/$os.new
                say "$os: carrying your files over failed; $os was not replaced, and nothing of it was changed"
                exit 1
            fi
            carried=1
        elif [ "${F50_UPDATE_KEEP:-all}" != list ]; then
            say "this image does not bring what carries everything over (f50-carry): keeping the settings an update always kept"
        fi
        extra=
        if [ $carried = 0 ]; then
            # the new image's version and package lists, and the names of its own etc/config files (upk_record)
            image_own_save "$M/$os.new"
            keep="etc/config etc/f50 etc/unisoc-modem etc/dropbear etc/rc.local etc/crontabs root"
            kept=
            for k in $keep; do
                ks=$k
                [ -e "$M/$os/$ks" ] || continue
                mkdir -p "$M/$os.new/$(dirname $k)"
                rm -rf "$M/$os.new/$k.image"
                [ "$k" = etc/config ] && [ -d "$M/$os.new/$k" ] && mv "$M/$os.new/$k" "$M/$os.new/$k.image"
                rm -rf "$M/$os.new/$k"
                cp -a "$M/$os/$ks" "$M/$os.new/$k" && kept="$kept $ks"
                # OpenWrt: the old settings, plus the config files of the image's packages the old system did not have
                # (as f50-update does; the old etc/config alone dropped e.g. /etc/config/sqm)
                if [ -d "$M/$os.new/$k.image" ]; then
                    for f in "$M/$os.new/$k.image"/* "$M/$os.new/$k.image"/.[!.]*; do
                        [ -e "$f" ] || [ -L "$f" ] || continue
                        [ -e "$M/$os.new/$k/${f##*/}" ] || [ -L "$M/$os.new/$k/${f##*/}" ] || cp -a "$f" "$M/$os.new/$k/"
                    done
                    rm -rf "$M/$os.new/$k.image"
                    # the stray files of v2026.10.18 (the old system's, or that image's own) do not stay
                    stale_config_clean "$M/$os.new"
                fi
            done
            # services the user enabled or disabled themselves: copy the extra symlinks over, but only when the
            # service they point at exists in the new system (stale ones from an older release must not come back)
            for l in $M/$os/etc/rc.d/*; do
                [ -L "$l" ] || [ -e "$l" ] || continue
                u=${l##*/}
                if [ -L "$M/$os.new/etc/rc.d/$u" ]; then continue; fi
                [ -x "$M/$os.new/etc/init.d/${u#S??}" ] || [ -x "$M/$os.new/etc/init.d/${u#K??}" ] || continue
                cp -a "$l" "$M/$os.new/etc/rc.d/$u" && extra="$extra $u"
            done
            say "kept from the previous $os:$kept"
            image_own_restore "$M/$os.new"
            # the packages installed on the device: written down, their settings set aside, put back at the first boot
            # (f50-user-packages), as f50-update apply does
            say "packages installed on the device in the previous $os:"
            upk=$(upk_record "$M/$os" "$M/$os.new" "$os") ||
                { rm -rf $M/$os.new; say "$upk"; say "$os: could not write down the packages installed on the device; $os was not replaced"; exit 1; }
            printf '%s\n' "$upk" | while IFS= read -r l; do say "$l"; done
            rm -rf "$M/$os.new/.f50-image"
        fi
        if [ -f "$M/$os/etc/shadow" ]; then
            # (carried over by the carry block already, when that ran)
            if [ $carried = 0 ]; then
                merge_accounts "$M/$os" "$M/$os.new" || { say "could not carry the accounts of $os over; nothing was replaced"; exit 1; }
            fi
            # the log line says what the shadow will hold: with a hash, or PW_SET=1 (the Magisk installer's
            # F50_PASSWORD_RESET=yes, or a system that still had the images' accounts), the system's own account
            # gets PWHASH below, and only the other accounts keep their passwords
            if [ $pwkeep = 1 ]; then
                say "kept the accounts and passwords of the previous $os"
            elif [ -n "$PWHASH" ]; then
                say "kept the other accounts of the previous $os; $pwu's password is the one the installer chose (the images' own, or F50_PASSWORD)"
            elif [ "${PW_SET:-0}" = 1 ]; then
                say "kept the other accounts of the previous $os; $pwu's password is the images' own again: none, until one is set"
            else
                say "kept the accounts and passwords of the previous $os"
            fi
        fi
        [ -n "$extra" ] && say "kept enabled services:$extra"
    fi
    # The password, before the new system replaces the old one: the hash just chosen (PWHASH; with PW_SET=1 also an
    # empty one, OpenWrt's root without a password), else the accounts carried over, else the image's own (OpenWrt's
    # root without a password: the well-known one, issue #167). The accounts are this system's now, whichever they
    # are: never the mark that has f50-update carry some <os>.old's over at boot.
    if [ $pwkeep = 0 ] && { [ -n "$PWHASH" ] || [ "${PW_SET:-0}" = 1 ]; }; then
        sed -i "s|^$pwu:[^:]*:|$pwu:$PWHASH:|" $M/$os.new/etc/shadow
    fi
    rm -f $M/$os.new$ACCOUNTS_MARK
    # The old system goes aside under a name of its own and is removed when the new one is in place: files carried
    # by link are the same files in both, and a run that died between these steps must not leave a state in which
    # the next one would delete them (see the top of this loop). <os>.broken: the name f50-update removes too.
    stage_run "replace $os" 900 rm -rf $M/$os.broken
    if [ -d $M/$os ]; then mv $M/$os $M/$os.broken; fi
    mv $M/$os.new $M/$os
    stage_run "replace $os" 900 rm -rf $M/$os.broken
    R=$M/$os
    mkdir -p $R/etc/f50
    if [ -n "$ssid" ] && ! { [ "${UPDATE:-0}" = 1 ] && [ -s $R/etc/f50/hotspot.conf ]; }; then
        umask 077
        printf 'SSID=%s\nPSK=%s\nBAND=5\nCHANNEL=auto\nCOUNTRY=TR\n' "$ssid" "$psk" > $R/etc/f50/hotspot.conf
        chmod 600 $R/etc/f50/hotspot.conf
        umask 022
    fi
    if [ "$DEFAULT_LINUX" = 1 ]; then echo linux > $R/etc/f50/default-boot; else rm -f $R/etc/f50/default-boot; fi
    # adb over USB to Linux for the installing computer's key only (whoever holds it gets a root shell); ADB=0
    # changes nothing (an update keeps etc/f50 and with it the setting and the keys), and a system with nothing
    # set has adb on for every computer
    if [ "${ADB:-0}" = 1 ] && [ -n "${ADB_KEY:-}" ]; then
        # keys first: with only one of the two written, the system reads keys either way (f50-adbd, auth_read)
        echo keys > $R/etc/f50/adb_auth
        echo on > $R/etc/f50/adb
        umask 077
        grep -qF "${ADB_KEY%% *}" $R/etc/f50/adb_keys 2>/dev/null || echo "$ADB_KEY" >> $R/etc/f50/adb_keys
        umask 022
        chmod 600 $R/etc/f50/adb_keys
        say "$os: adb over USB on, only for ${ADB_KEY#* }"
    fi
    rm -f $tarball
done
mkdir -p $M/.f50
sd_unmark $M
echo "$BOOT_OS" > $M/.f50/boot-os
# --- install-os end
case ${BOOT_ATTEMPTS:-} in [1-6]) echo "$BOOT_ATTEMPTS" > $M/.f50/boot-attempts ;; esac
# the vendor 5.4 kernel, the only one (checked at the start)
mkdir -p $M/boot; echo 5.4 > $M/boot/kernel; echo 5.4 > $M/boot/installed.kernel
# the boot image is the installer's now: a release tag left by an earlier f50-update would describe another one
rm -f $M/boot/installed.tag
[ -n "$ssid" ] && say "hotspot: SSID $ssid imported (passphrase ${#psk} chars)"
say "installed: $(ls -d $M/openwrt 2>/dev/null | sed "s|$M/||g" | tr '\n' ' ')boot-os=$BOOT_OS default-linux=$DEFAULT_LINUX"
sd_mark_internal
# the card's dirty pages go out now, bounded like the other card stages (the unmount in sd_exit then has little
# left to wait for); a timeout here is a failure like any other: the OK line never comes, nothing is armed
[ "${SD_MODE:-0}" != 1 ] || stage_run flush 1800 sync
stall_clear
rm -f $T/f50-install.env
echo F50-INSTALL-OK   # install.sh checks for this line (set -e stops before it on any failure)
