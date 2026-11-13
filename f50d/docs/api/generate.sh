#!/bin/sh
# Regenerates the OpenAPI document from a device running f50d: `f50d openapi` reads the method table and samples the
# read methods for the response shapes (types only, never values). Writes f50d/docs/api/openapi.json and the copy the
# web console serves with Swagger UI (f50-web/public/api-docs/openapi.json).
#
#   f50d/docs/api/generate.sh            over adb (the default)
#   SSH=root@192.168.77.1 f50d/docs/api/generate.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
out="$here/openapi.json"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

if [ -n "${SSH:-}" ]; then
    ssh "$SSH" f50d openapi > "$tmp"
else
    adb shell f50d openapi > "$tmp"
fi
# pretty, stable, UTF-8 as it is
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); json.dump(d, open(sys.argv[2],"w"), indent=2, ensure_ascii=False); open(sys.argv[2],"a").write("\n")' "$tmp" "$out"
mkdir -p "$root/f50-web/public/api-docs"
cp "$out" "$root/f50-web/public/api-docs/openapi.json"
echo "wrote $out ($(grep -c '"operationId"' "$out") operations) and f50-web/public/api-docs/openapi.json"
