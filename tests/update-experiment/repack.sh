#!/bin/sh
# A release to update TO, made without a release build: the images of a published release with this checkout's
# updater, the carry helper and the manifests put into them the way the image builds do it (systems/base/assemble.sh,
# systems/openwrt/build-rootfs.sh). Inside the container:
#   repack.sh RELEASE_DIR OUT_DIR TAG [ASSET...]      (ASSET: f50-openwrt-rootfs, ...; default: all four)
# OUT_DIR gets the four images as release TAG (etc/f50/image-version, their own manifest), this checkout's
# f50-update, the kernel bundles of RELEASE_DIR (links) and SHA256SUMS: what lab.sh serves to an installed updater.
# What it does not have is everything else a release build would change (other overlay files, packages).
set -eu
SRC=$(cd "$(dirname "$0")/../.." && pwd)
rel=$1 out=$2 tag=$3
shift 3
only=" $* "
W=${F50_LAB_WORK:-/w/lab}/repack
mkdir -p "$out" "$W"
gcc -O2 -static -o "$W/f50-carry" "$SRC/device-tools/carry/f50-carry.c"
for asset in f50-ubuntu-rootfs:ubuntu-24.04 f50-ubuntu-26.04-rootfs:ubuntu-26.04 f50-openwrt-rootfs:openwrt \
             f50-openwrt-luci-rootfs:openwrt-luci; do
    image=${asset#*:}; asset=${asset%%:*}
    case $only in "  "|*" $asset "*) ;; *) continue ;; esac
    R=$W/root
    rm -rf "$R"; mkdir -p "$R"
    tar --numeric-owner -xzpf "$rel/$asset.tar.gz" -C "$R"
    install -m 755 "$SRC/systems/base/overlay/opt/f50/bin/f50-update" "$R/opt/f50/bin/f50-update"
    install -m 755 "$W/f50-carry" "$R/opt/f50/bin/f50-carry"
    # the manifests of the releases before this one: an image has those of its own system
    mkdir -p "$R/opt/f50/lib/image-manifests"
    case $image in
        ubuntu-*) for m in ubuntu-24.04 ubuntu-26.04; do install -m 644 "$SRC/systems/base/overlay/opt/f50/lib/image-manifests/$m.gz" "$R/opt/f50/lib/image-manifests/"; done ;;
        *) install -m 644 "$SRC/systems/base/overlay/opt/f50/lib/image-manifests/$image.gz" "$R/opt/f50/lib/image-manifests/"
           install -m 755 "$SRC/systems/openwrt/overlay/etc/profile.d/f50-update.sh" "$R/etc/profile.d/f50-update.sh" ;;
    esac
    echo "$tag" > "$R/etc/f50/image-version"
    "$W/f50-carry" manifest "$R" "$image" "$tag" | gzip -9n > "$W/manifest.gz"
    install -m 644 "$W/manifest.gz" "$R/etc/f50/image-manifest.gz"
    ( cd "$R" && tar --numeric-owner -czf "$out/$asset.tar.gz" . )
    python3 "$SRC/scripts/release/image-manifest.py" check "$out/$asset.tar.gz"
    rm -rf "$R"
done
for f in "$rel"/f50-kernel*.tar.gz; do ln -sf "$f" "$out/"; done
cp "$SRC/systems/base/overlay/opt/f50/bin/f50-update" "$out/f50-update"
( cd "$out" && sha256sum *.tar.gz f50-update > SHA256SUMS )
ls -la "$out"
