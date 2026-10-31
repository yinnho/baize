#!/bin/sh
# 铸 baize 包：pkg.toml + SKILL.md + files/bin/五腿+路由 + files/seed/
# 产出 .local/release/<tag>/baize-v<ver>-4pc.tar + .sha256
# 用法: scripts/build-pkg.sh <版本> <tag>   例: scripts/build-pkg.sh 0.1.0 v0.0.3
set -eu
cd "$(dirname "$0")/.."
VER="${1:?用法: build-pkg.sh <版本> <tag>}"
TAG="${2:?用法: build-pkg.sh <版本> <tag>}"
OUT=".local/release/$TAG"
STAGE=$(mktemp -d "${TMPDIR:-/tmp}/baize-pkg.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/files/bin" "$STAGE/files/seed"

cp pkg/baize/pkg.toml pkg/baize/SKILL.md "$STAGE/"
cp bin/baize bin/baize-day bin/baize-night bin/baize-harvest bin/baize-k bin/baize-adopt bin/baize-gate "$STAGE/files/bin/"
cp -R seed/. "$STAGE/files/seed/"
find "$STAGE/files/seed" -name '.DS_Store' -delete

mkdir -p "$OUT"
TAR="$OUT/baize-v$VER-4pc.tar"
tar -cf "$TAR" -C "$STAGE" pkg.toml SKILL.md files
shasum -a 256 "$TAR" | sed 's/  .*//' > "$TAR.sha256"
echo "built: $TAR"
echo "sha256: $(cat "$TAR.sha256")"
tar -tf "$TAR"
