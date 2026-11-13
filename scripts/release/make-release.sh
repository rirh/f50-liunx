#!/bin/sh
# Maintainer: build the generic release assets used by ./install.sh (prebuilt mode) and optionally publish them.
#   scripts/release/make-release.sh --next-tag      print the tag of the next release: vYYYY.MM.DD.N for today (UTC), N one
#                                          more than the highest N of today's releases and tags (1 for the first)
#   scripts/release/make-release.sh --check-tag TAG  only check TAG as a new release's tag (below); prints it
#   scripts/release/make-release.sh TAG             build into release/TAG and audit the contents
#   scripts/release/make-release.sh TAG --publish   also create/update the GitHub release (gh)
#   scripts/release/make-release.sh TAG --publish --prerelease   create it as a prerelease: never "latest", so f50-update and
#                                          the installers do not offer it until it is promoted on GitHub
#   --any-date                             take a TAG whose day is not today (a build that went on past midnight, a
#                                          retry of a release made another day); its form is checked all the same
# The tag of a new release is vYYYY.MM.DD.N: the real day (UTC) it is built, and N = 1, 2, ... for the releases of
# that day (docs/research/FINDINGS.md 125; the three-field tags up to v2026.10.27 used the day as a counter and are not made
# any more). Every four-field tag is newer than every three-field one, for the updater and the installers alike.
# Needs docker and the kernel outputs in $F50_KERNEL_OUT (default: out/, from kernel/vendor/build-all.sh).
# The assets must never contain proprietary files or anything from a particular device: the audit below fails the
# build if firmware, Android userspace, host keys or local settings end up in an image.
set -eu
USAGE='usage: scripts/release/make-release.sh TAG [--publish [--prerelease]] [--any-date] | --next-tag | --check-tag TAG [--any-date]'
TOP=$(cd "$(dirname "$0")/../.." && pwd)
REPO=${F50_REPO:-dikeckaan/mu300-linux}
# tag_ok, tag_key, newer: the release tags (the same block as f50-update's)
. "$TOP/installer/lib/releases.sh"
# today: the day of a release built now, as its tag writes it (F50_TODAY stands in for the clock in the tests)
today() { echo "${F50_TODAY:-$(date -u +%Y.%m.%d)}"; }
# next_tag: vYYYY.MM.DD.N for today. What counts: GitHub's releases (drafts and prereleases too, as gh lists them to
# the maintainer) and the tags, here and on origin. Without GitHub's answer there is no next tag: a guess could
# take the name of a release that exists.
next_tag() {
    _day=v$(today)
    _rel=$(gh release list -R "$REPO" --limit 200 --json tagName -q '.[].tagName') ||
        { echo "cannot read the releases of $REPO (gh): no tag is guessed" >&2; return 1; }
    _n=0
    for _t in $_rel $(git -C "$TOP" tag -l "$_day.*") \
        $(git -C "$TOP" ls-remote --tags origin "refs/tags/$_day.*" 2>/dev/null | sed -n 's|.*refs/tags/\([^^]*\).*|\1|p'); do
        case $_t in "$_day".*) tag_ok "$_t" || continue; [ "${_t##*.}" -le "$_n" ] || _n=${_t##*.} ;; esac
    done
    [ "$_n" -lt 99 ] || { echo "$_day has 99 releases already" >&2; return 1; }
    echo "$_day.$((_n + 1))"
}
# check_tag TAG: a tag a new release may have - vYYYY.MM.DD.N, and of today unless --any-date
check_tag() {
    case $1 in
        *.*.*.*) tag_ok "$1" || { echo "$1 is not a release tag: vYYYY.MM.DD.N, N = 1..99 without a leading zero (scripts/release/make-release.sh --next-tag)" >&2; return 1; } ;;
        *) echo "$1 is not a tag for a new release: those are vYYYY.MM.DD.N, the day and its N (scripts/release/make-release.sh --next-tag; docs/research/FINDINGS.md 125)" >&2; return 1 ;;
    esac
    [ -n "$ANY_DATE" ] || [ "${1%.*}" = "v$(today)" ] ||
        { echo "$1 is not of today (v$(today)): a release is named after the day it is built (--any-date takes it anyway)" >&2; return 1; }
}
ARG= PUBLISH="" PRERELEASE="" ANY_DATE="" MODE=build
for a in "$@"; do
    case $a in
        --publish) PUBLISH=--publish ;;
        --prerelease) PRERELEASE=--prerelease ;;
        --any-date) ANY_DATE=1 ;;
        --next-tag) MODE=next ;;
        --check-tag) MODE=check ;;
        -*) echo "unknown option $a ($USAGE)" >&2; exit 1 ;;
        *) [ -z "$ARG" ] || { echo "$USAGE" >&2; exit 1; }; ARG=$a ;;
    esac
done
case $MODE in
    next) [ -z "$ARG" ] || { echo "$USAGE" >&2; exit 1; }; next_tag; exit ;;
    check) [ -n "$ARG" ] || { echo "$USAGE" >&2; exit 1; }; check_tag "$ARG" || exit 1; echo "$ARG"; exit 0 ;;
esac
TAG=$ARG
[ -n "$TAG" ] || { echo "$USAGE" >&2; exit 1; }
check_tag "$TAG" || exit 1
KOUT=${F50_KERNEL_OUT:-$TOP/out}
D=$TOP/release/$TAG
IN=$D/inputs
[ -f "$KOUT/Image" ] && ls "$KOUT"/modules/*.ko >/dev/null 2>&1 || { echo "kernel outputs missing in $KOUT (kernel/vendor/build-all.sh)" >&2; exit 1; }
[ -z "$(git -C "$TOP" status --porcelain)" ] || { echo "commit your changes first: the release must match a commit" >&2; exit 1; }
rm -rf "$D" && mkdir -p "$IN/out" "$IN/device-tools/logdw" "$IN/device-tools/bt-init" "$IN/device-tools/gpu"

echo "==> helper binaries"
cp -R "$KOUT/modules" "$IN/out/modules"
cp "$KOUT/modules.builtin" "$KOUT/modules.builtin.modinfo" "$IN/out/"
docker build -q -t f50-kbuild "$TOP/kernel/vendor" >/dev/null
# the tools image (systems/base): its static busybox goes into the kernel bundle and the image
docker build -q -t f50-ubuntu:24.04 "$TOP/systems/base" >/dev/null
docker run --rm f50-ubuntu:24.04 cat /bin/busybox > "$IN/busybox"; chmod +x "$IN/busybox"
docker run --rm -v "$TOP/device-tools":/src:ro -v "$IN/device-tools":/o f50-kbuild sh -c '
  gcc -O2 -static -o /o/logdw/logdw /src/logdw/logdw.c &&
  gcc -O2 -static -o /o/bt-init/f50-bt-init /src/bt-init/f50-bt-init.c &&
  mkdir -p /o/keys && gcc -O2 -static -o /o/keys/f50-keys /src/keys/f50-keys.c &&
  mkdir -p /o/carry && gcc -O2 -static -o /o/carry/f50-carry /src/carry/f50-carry.c &&
  mkdir -p /o/pwcheck && gcc -O2 -static -o /o/pwcheck/f50-pwcheck /src/pwcheck/f50-pwcheck.c -lcrypt'
# the password checker of the installers (installer/android/android-pwcheck.sh: never a program of the Linux system it checks):
# it must know yescrypt, what Ubuntu's passwd writes (FINDINGS 126)
H='$y$j9T$yozuPAQ4fDSQH5fJzaFQd/$Z6UKqnHQMcZqZNJ18QIV5mTrJ7raj2y7MDipRg8bUj.'
[ "$(printf '%s\n%s\n' 'old secret 1' "$H" | docker run --rm -i -v "$IN/device-tools/pwcheck":/p:ro f50-kbuild /p/f50-pwcheck)" = MATCH ] ||
    { echo "f50-pwcheck does not check a yescrypt hash" >&2; exit 1; }
# e2fsck for the boot ramdisk: init runs it on a root filesystem whose superblock records an error (FINDINGS 182)
sh "$TOP/device-tools/e2fsck/build.sh" "$IN/device-tools/e2fsck/e2fsck" >/dev/null
# adbd (adb over USB to Linux): static aarch64, from AOSP's sources (device-tools/adbd/build.sh)
sh "$TOP/device-tools/adbd/build.sh" "$IN/device-tools/adbd/adbd" >/dev/null
# cltest links against Android's libraries at build time only; use a local build when there is one
[ -f "$TOP/device-tools/gpu/cltest" ] && cp "$TOP/device-tools/gpu/cltest" "$IN/device-tools/gpu/cltest"

echo "==> kernel bundle"
K=$D/kernel && mkdir -p "$K"
cp -R "$IN/out/modules" "$K/modules"
# the devices this bundle runs on (the installers check it)
echo "f50" > "$K/devices"
# what it can do that older bundles could not: 5.4 reads the SD card (FINDINGS 31j), so a system on the card may get
# it; its init runs from either slot, so a Linux on slot a may get it (f50-update checks both); the same init finds a
# system in a GPT partition named f50root (gpt-root, FINDINGS 178)
printf 'sdcard\nlinux-slot\ngpt-root\n' > "$K/features"
cp "$KOUT/Image" "$KOUT/modules.builtin" "$KOUT/modules.builtin.modinfo" "$IN/busybox" "$IN/device-tools/logdw/logdw" "$K/"
# beside busybox for the installers' password check (the Magisk zips take both from here; FINDINGS 126)
cp "$IN/device-tools/pwcheck/f50-pwcheck" "$K/"
# beside busybox for every boot ramdisk built from this bundle (install.sh)
cp "$IN/device-tools/e2fsck/e2fsck" "$K/"
# the device-independent part of the boot ramdisk, which f50-update puts behind the device's own ramdisk to update
# the kernel and the boot image without a computer (same builder and file list as install.sh)
python3 "$TOP/boot/build-boot-image.py" --generic-ramdisk --modules "$IN/out/modules" --busybox "$IN/busybox" \
  --logdw "$IN/device-tools/logdw/logdw" --e2fsck "$IN/device-tools/e2fsck/e2fsck" --ueventd-perms "$TOP/android/vendor/ueventd-perms.sh" \
  --out "$K/ramdisk-generic.lz4" >/dev/null
tar -C "$K" -czf "$D/f50-kernel.tar.gz" .
rm -rf "$K"

echo "==> OpenWrt root filesystem (generic)"
F50_INPUTS="$IN" F50_VERSION="$TAG" sh "$TOP/systems/openwrt/build-rootfs.sh" f50-openwrt-release.tar.gz >/dev/null
mv "$TOP/systems/openwrt/f50-openwrt-release.tar.gz" "$D/f50-openwrt-rootfs.tar.gz"

echo "==> audit"
fail=0
list_of_image() { tar -tzf "$D/$1.tar.gz" | sed 's|^\./||'; }
for a in f50-kernel f50-openwrt-rootfs; do
    bad=$(tar -tzf "$D/$a.tar.gz" | sed 's|^\./||' | grep -E \
        -e '(^|/)lib/firmware/(wcnmodem|gnssmodem|wifi_board_config|bt_configure)' \
        -e '^opt/f50/android/.+' -e '__properties__|dev-properties' \
        -e '(^|/)(libmali|libOpenCL|libGLES|libEGL|libvulkan)[^/]*\.so' \
        -e '^etc/ssh/ssh_host_' -e '^etc/uhttpd\.key$' -e '^etc/f50/(hotspot|vpn|toolkit|mobile-data)\.conf$' -e '^etc/f50/update-source$' -e '^etc/dropbear/dropbear_.*_host_key' \
        -e '^opt/f50/bin/(xray|sing-box|hev-socks5-tunnel|mihomo|f50-vpn)$' -e '^opt/f50/lib/vpn(/|$)' \
        -e '^etc/systemd/system/f50-vpn\.service$' -e '^etc/init\.d/f50-vpn$' -e '^etc/f50/vpn\.conf\.example$' \
        | grep -vE '^opt/f50/android/system/?$|^opt/f50/android/system/bin/?$|^opt/f50/android/system/bin/cltest$' || true)
    if [ -n "$bad" ]; then echo "$a contains files that must not be published:"; echo "$bad" | head -20; fail=1; fi
    mid=$(tar -xzOf "$D/$a.tar.gz" ./etc/machine-id 2>/dev/null || true)
    [ -z "$mid" ] || { echo "$a has a machine-id"; fail=1; }
done
# An update carries everything of the user's over with what the NEW image brings (f50-update's carry block): each
# image must have f50-carry, a static busybox and the manifest of its own files - which must be the one this tool makes from the tarball (two implementations,
# held against each other at every release). Without them an update would quietly keep only the list of places.
# The manifest is published beside the image, too (f50-<image>-rootfs.manifest.gz, in SHA256SUMS).
for a in f50-openwrt-rootfs:opt/f50/bin/busybox; do
    bb=${a#*:}; a=${a%%:*}
    l=$(list_of_image "$a")
    for f in opt/f50/bin/f50-carry "$bb" etc/f50/image-manifest.gz; do
        printf '%s\n' "$l" | grep -qx "$f" || { echo "$a lacks $f: an update to it would not carry everything over"; fail=1; }
    done
    python3 "$TOP/scripts/release/image-manifest.py" check "$D/$a.tar.gz" || fail=1
    tar -xzOf "$D/$a.tar.gz" ./etc/f50/image-manifest.gz > "$D/$a.manifest.gz" 2>/dev/null || { echo "$a: no manifest to publish"; fail=1; }
done
# the image: f50d and the web console in it, the LuCI backend of the old shell layer not, and LuCI in its languages
plain=$(tar -tzf "$D/f50-openwrt-rootfs.tar.gz" | sed 's|^\./||')
for f in usr/libexec/rpcd/f50dash; do
    ! printf '%s\n' "$plain" | grep -qx "$f" || { echo "f50-openwrt-rootfs has $f, the backend f50d replaces"; fail=1; }
done
for f in usr/sbin/f50d www/f50/index.html etc/f50/packages.txt usr/lib/lua/luci/i18n/base.tr.lmo usr/lib/lua/luci/i18n/base.zh-cn.lmo; do
    printf '%s\n' "$plain" | grep -qx "$f" || { echo "f50-openwrt-rootfs lacks $f"; fail=1; }
done
# what the package list changed since the last release (F50_PREV_RELEASE: its directory, with the assets)
if [ -n "${F50_PREV_RELEASE:-}" ]; then
    for a in f50-openwrt-rootfs; do
        [ -f "$F50_PREV_RELEASE/$a.tar.gz" ] || { echo "$a: no previous asset in $F50_PREV_RELEASE"; continue; }
        echo "==> $a: etc/f50/packages.txt against the previous release"
        tar -xzOf "$F50_PREV_RELEASE/$a.tar.gz" ./etc/f50/packages.txt > "$D/packages.prev" 2>/dev/null || : > "$D/packages.prev"
        tar -xzOf "$D/$a.tar.gz" ./etc/f50/packages.txt > "$D/packages.new" 2>/dev/null || : > "$D/packages.new"
        diff -u "$D/packages.prev" "$D/packages.new" || true
        rm -f "$D/packages.prev" "$D/packages.new"
    done
fi
# f50-update refuses a kernel bundle without the SD host for a system on the card, and one whose init runs only from
# slot b for a Linux on slot a: the bundle must say it has both
for a in f50-kernel; do
    tar -xzOf "$D/$a.tar.gz" ./features 2>/dev/null | grep -qx sdcard || { echo "$a does not list sdcard in ./features"; fail=1; }
    tar -xzOf "$D/$a.tar.gz" ./features 2>/dev/null | grep -qx linux-slot || { echo "$a does not list linux-slot in ./features"; fail=1; }
    tar -xzOf "$D/$a.tar.gz" ./features 2>/dev/null | grep -qx gpt-root || { echo "$a does not list gpt-root in ./features"; fail=1; }
done
# the generic ramdisk segment brings init's e2fsck: a root filesystem with recorded errors is checked before it is
# mounted (FINDINGS 182)
for a in f50-kernel; do
    tar -xzOf "$D/$a.tar.gz" ./ramdisk-generic.lz4 > "$D/ramdisk.check" 2>/dev/null &&
    python3 -c 'import importlib.util as u, sys
s = u.spec_from_file_location("b", sys.argv[1]); b = u.module_from_spec(s); s.loader.exec_module(b)
sys.exit("sbin/e2fsck" not in b.cpio_names(b.unlz4_legacy(open(sys.argv[2], "rb").read())))' \
        "$TOP/boot/build-boot-image.py" "$D/ramdisk.check" || { echo "$a: its ramdisk-generic.lz4 has no sbin/e2fsck"; fail=1; }
    rm -f "$D/ramdisk.check"
done
# Call audio is not supported: reject leftover drivers, programs and services in release inputs.
# The vendor kernel keeps its native DMA driver.
AUDIO_KO="audio_mem agdsp_access audio_sipc sprd_audcp_boot sprd_audcp_dvfs audio-dsp-dump audio-pipe sprd_apipe
mcdt_hw_r2p0 saudio sprd-dmaengine-pcm sprd-platform-pcm-routing sprd-compr-2stage-dma snd-soc-sprd-dai snd-soc-sprd-vaudio
snd-soc-sprd-vbc-v4 snd-soc-sprd-vbc-fe snd-soc-sprd-codec-ump9620-power snd-soc-sprd-codec-ump9620-power-dev
snd-soc-sprd-codec-ump9620 snd-soc-sprd-pa-ucp1301 snd-soc-sprd-dummy-codec snd-soc-sprd-card"
for a in f50-kernel; do
    l=$(tar -tzf "$D/$a.tar.gz")
    for m in $AUDIO_KO; do
        ! printf '%s\n' "$l" | grep -Eq "/$m\.ko\$" || { echo "$a carries $m.ko, which belongs to the removed call-audio stack"; fail=1; }
    done
done
for m in f50-openwrt-rootfs; do
    [ -f "$D/$m.manifest.gz" ] || continue
    ! gzip -dc "$D/$m.manifest.gz" | grep -Eq '(opt/f50/bin/(f50-audio-dsp|f50-voice|pcmhost|ctlset|f50-hfp)|systemd/system/f50-(audio|voice)\.service|init\.d/f50-(audio|voice)|modprobe\.d/f50-audio\.conf)( |$)' ||
        { echo "$m carries removed call-audio programs or services"; fail=1; }
done
[ $fail = 0 ] || { echo "audit failed, nothing published" >&2; exit 1; }
rm -rf "$IN"
# the diffs from the releases before this one (scripts/release/make-diffs.sh): listed in SHA256SUMS below, so signed with the rest
sh "$TOP/scripts/release/make-diffs.sh" "$D" "$TAG"
# the updater itself: an older f50-update fetches this one and continues with it
cp "$TOP/systems/base/overlay/opt/f50/bin/f50-update" "$D/f50-update"
# f50-release: the tag, as a file, so that SHA256SUMS - and with it the signature - says which release these
# checksums are of (f50-update's tag_sha: a signed SHA256SUMS cannot be handed out as another release's)
echo "$TAG" > "$D/f50-release"
(cd "$D" && { shasum -a 256 *.tar.gz *.manifest.gz f50-update f50-release 2>/dev/null || sha256sum *.tar.gz *.manifest.gz f50-update f50-release; } > SHA256SUMS)
# The signature of SHA256SUMS (scripts/release/sign-release.sh; f50-update checks it where the system has the public key,
# docs/research/FINDINGS.md 86): made when the release key is here - F50_RELEASE_KEY, which the Release workflow takes from the
# repository secret of that name, or F50_RELEASE_KEY_FILE. The images of this release carry the public key when it
# is in the tree; devices with it refuse a newer release that is not signed, so such a release is never published
# without its signature.
PUB=$TOP/systems/base/overlay/opt/f50/lib/release.pub
SIG=
if [ -n "${F50_RELEASE_KEY:-}${F50_RELEASE_KEY_FILE:-}" ]; then
    if [ -s "$PUB" ]; then sh "$TOP/scripts/release/sign-release.sh" "$D/SHA256SUMS" "$PUB"
    else
        echo "signing without $PUB in the tree: no device checks this signature until the public key is committed" >&2
        sh "$TOP/scripts/release/sign-release.sh" "$D/SHA256SUMS"
    fi
    SIG=$D/SHA256SUMS.sig
elif [ -s "$PUB" ]; then
    echo "the images carry the release key ($PUB) and there is no F50_RELEASE_KEY to sign with:" >&2
    echo "devices would refuse this release. Nothing is published without its signature." >&2
    [ "$PUBLISH" != --publish ] || exit 1
fi
ls -la "$D"

[ "$PUBLISH" = --publish ] || { echo "built release/$TAG (run again with --publish to upload)"; exit 0; }
commit=$(git -C "$TOP" rev-parse HEAD)
kernel_rev=$(sed -n 's/^KERNEL_REV=//p' "$TOP/kernel/vendor/build-all.sh")
modules_rev=$(sed -n 's/^MODULES_REV=//p' "$TOP/kernel/vendor/build-all.sh")
notes=$(mktemp "${TMPDIR:-/tmp}/f50-notes.XXXXXX")
cat > "$notes" <<EOF
Prebuilt images for \`./install.sh\` (ZTE F50: OpenWrt with f50d and the web console). Check your device
first with \`./install.sh --check\`.

| file | contents |
|---|---|
| f50-kernel.tar.gz | Linux 5.4.254 \`Image\` and modules, with the patches of \`kernel/vendor/patches\` (among them: no panic when a network namespace goes away, #103, and the Wi-Fi driver's routine lines no longer at error level, #140), static busybox, logdw and e2fsck for the boot image, the installers' static password checker \`f50-pwcheck\`, and the generic boot ramdisk segment \`f50-update\` uses |
| f50-openwrt-rootfs.tar.gz | OpenWrt 25.12.6 root filesystem with f50d and the web console (/f50/) |
| f50-update | the on-device updater of this release (\`f50-update apply\` switches to it before it changes anything) |
| f50-*-rootfs.diff-<tag>.tar.gz | an image without the files the image of release <tag> (one of the five before this one) had as they are: \`f50-update apply\` on a device that runs <tag> downloads it instead of the whole image, takes the rest from the installed system, and installs it only when the result is, file for file, the image this release's signed manifest describes (else it downloads the whole image) |
| f50-*-rootfs.manifest.gz | the list of each image's files (type, mode, owner, size, sha256), which the image also carries: an update reads it to tell the image's files from yours, and carries yours over |
| f50-magisk-*.zip | attached a few minutes later by the Magisk installers workflow, one per system and kernel. Two ways to use one: install it in **Magisk** on the device (no computer), or **unpack it on a computer** and run \`start.cmd\` / \`sh start.sh\` (\`start-gui\` for the window): the project's installer, with that system and kernel, offline (\`START-HERE.md\` in the zip; adb from Google's platform-tools is not included) |
| (the VPN) | not part of this release: the VPN toolkit (\`f50-vpn\`) is the module of https://github.com/dikeckaan/mu300-linux-vpn |

The images contain **no proprietary files**: the installer pulls the Wi-Fi/Bluetooth firmware and the Android
modem/GPU userspace from your own device and adds them during installation. They carry a static adbd (AOSP
platform-tools, built by \`device-tools/adbd/build.sh\`) for adb to Linux over USB, which is **on by default and lets every
computer in**: whoever can plug a cable into the device gets a root shell without a password. To close it:
the web console's USB page (adb: off, or only authorised keys). A device where adb was turned off, or set up with a key, before this release stays as it was.

Built from $REPO@$commit with \`kernel/vendor/build-all.sh\` and \`scripts/release/make-release.sh\`.
Corresponding source (GPL): kernel https://github.com/Enceka/android_kernel_zte_ums9620_mifi_u30air/tree/$kernel_rev ,
Wi-Fi/Bluetooth/Mali modules https://github.com/realme-kernel-opensource/realme_C51_C53_Narzo-N53-AndroidT-kernel-source/tree/$modules_rev ,
patches in \`kernel/vendor/patches\`. OpenWrt and busybox packages come from their distributions' archives.
News and releases: the Telegram channel https://t.me/f50project
EOF
if gh release view "$TAG" -R "$REPO" >/dev/null 2>&1; then
    # shellcheck disable=SC2086
    gh release upload "$TAG" -R "$REPO" --clobber "$D"/*.tar.gz "$D"/*.manifest.gz "$D/f50-update" "$D/f50-release" "$D/SHA256SUMS" $SIG
else
    # shellcheck disable=SC2086
    gh release create "$TAG" -R "$REPO" --target "$commit" --title "F50 Linux $TAG" --notes-file "$notes" $PRERELEASE \
      "$D"/*.tar.gz "$D"/*.manifest.gz "$D/f50-update" "$D/f50-release" "$D/SHA256SUMS" $SIG
fi
rm -f "$notes"
