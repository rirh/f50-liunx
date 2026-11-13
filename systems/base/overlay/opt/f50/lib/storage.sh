# The device's internal storage - an eMMC (mmcblkN) or a UFS (sdX) - and its GPT partitions, found by what they are,
# never by number or name alone: the one lookup that boot/init (as /etc/f50-storage.sh in the ramdisk), the
# installers and android-vendor-start use (f50-update carries a copy, tests/test_storage_lib.py keeps it the same).
# After kanoqwq's storage.sh (his fe52afe3, issue #396).
#
# The eMMC: the disk whose card is of type MMC, else the first mmcblk disk that is not a card - the host index
# follows probe order, and under mainline the card slot once took mmc0 and left mmcblk1 to the eMMC (FINDINGS 31l).
# The UFS: a SCSI disk below the UFS host (".../22000000.ufs/host0/..."), the logical unit that holds misc - its
# other units are 4 MiB boot areas. Never a card, a USB disk or anything else: they can carry the same GPT names (a
# raw clone of a device backup), and what is found here is written to (the BCB in misc, a kernel in boot_<slot>).
# sysfs counts start and size in 512-byte sectors on both; only a GPT is in logical blocks (4096 on a UFS).
# POSIX sh for dash, bash, busybox ash and Android's mksh; set -e safe; F50_SYS is the sysfs root (/sys).
# --- storage begin
f50_storage_mmc() {  # f50_storage_mmc TYPE: the whole mmcblkN disks whose card is of TYPE (MMC, SD)
    for _f50sb in "${F50_SYS:-/sys}"/block/mmcblk*; do
        case ${_f50sb##*/mmcblk} in ''|*[!0-9]*) continue ;; esac   # not mmcblk0boot0, mmcblk0rpmb, ...
        [ "$(cat "$_f50sb/device/type" 2>/dev/null)" != "$1" ] || echo "${_f50sb##*/}"
    done
    return 0
}
f50_storage_ufs() {  # f50_storage_ufs DISK: is DISK a disk of the UFS host?
    case ${1##*/} in sd[a-z]|sd[a-z][a-z]) ;; *) return 1 ;; esac
    _f50sp=$(readlink -f "${F50_SYS:-/sys}/block/${1##*/}" 2>/dev/null) || return 1
    case $_f50sp in */*.ufs/host[0-9]*/*) return 0 ;; esac
    return 1
}
f50_storage_prefix() {  # f50_storage_prefix DISK: what its partitions' names are DISK plus a number after
    case ${1##*/} in *[0-9]) echo "${1##*/}p" ;; *) echo "${1##*/}" ;; esac
}
f50_storage_parts() {  # f50_storage_parts DISK: the names of its partitions (mmcblk0p1 ..., sda1 ...)
    _f50sx=$(f50_storage_prefix "$1")
    for _f50sq in "${F50_SYS:-/sys}/block/${1##*/}/$_f50sx"[0-9]*; do
        [ -d "$_f50sq" ] && echo "${_f50sq##*/}"
    done
    return 0
}
f50_storage_part() {  # f50_storage_part NAME [DISK]: the partition named NAME in the GPT of DISK (internal)
    _f50sd=${2:-}
    [ -n "$_f50sd" ] || _f50sd=$(f50_storage_disk) || return 1
    for _f50sq in $(f50_storage_parts "$_f50sd"); do
        grep -qx "PARTNAME=$1" "${F50_SYS:-/sys}/block/${_f50sd##*/}/$_f50sq/uevent" 2>/dev/null || continue
        echo "$_f50sq"
        return 0
    done
    return 1
}
f50_storage_disk() {  # f50_storage_disk: the internal disk's name; status 1 when it is not there (yet)
    for _f50sd in $(f50_storage_mmc MMC); do echo "$_f50sd"; return 0; done
    for _f50sb in "${F50_SYS:-/sys}"/block/sd*; do
        f50_storage_ufs "$_f50sb" && f50_storage_part misc "${_f50sb##*/}" >/dev/null &&
            { echo "${_f50sb##*/}"; return 0; }
    done
    for _f50sb in "${F50_SYS:-/sys}"/block/mmcblk*; do
        case ${_f50sb##*/mmcblk} in ''|*[!0-9]*) continue ;; esac
        [ "$(cat "$_f50sb/device/type" 2>/dev/null)" = SD ] || { echo "${_f50sb##*/}"; return 0; }
    done
    return 1
}
f50_storage_sector_size() {  # f50_storage_sector_size DISK: its logical block size, 512 or 4096
    _f50ss=$(cat "${F50_SYS:-/sys}/block/${1##*/}/queue/logical_block_size" 2>/dev/null) || _f50ss=
    case $_f50ss in 4096) echo 4096 ;; *) echo 512 ;; esac
}
# --- storage end
