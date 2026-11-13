#!/bin/sh
# Build the Magisk module zip: android/magisk/build.sh [OUT.zip]
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
SRC=$HERE/f50-linux-switch
OUT=${1:-$HERE/f50-linux-switch.zip}
rm -f "$OUT"
(cd "$SRC" && zip -qr "$OUT" . -x '.*')
echo "built $OUT"
unzip -l "$OUT"
