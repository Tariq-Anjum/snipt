#!/usr/bin/env bash
# Assemble GitHub release assets from already-built artifacts (no build here):
#   dist/release/Snipt-arm64.apk          <- dist/Snipt-arm64-v<version>.apk
#   dist/release/Snipt-linux-x86_64.tar.gz (binary + launcher + icons + installer)
#   dist/release/SHA256SUMS
# Usage: scripts/make-release.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VER="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$ROOT/src-tauri/tauri.conf.json" | head -1)"
[ -n "$VER" ] || { echo "version not found" >&2; exit 1; }
APK="$ROOT/dist/Snipt-arm64-v$VER.apk"
BIN="$ROOT/dist/Snipt-linux-x86_64"
for f in "$APK" "$BIN"; do [ -f "$f" ] || { echo "missing build artifact: $f" >&2; exit 1; }; done

OUT="$ROOT/dist/release"
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$APK" "$OUT/Snipt-arm64.apk"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
B="$STAGE/snipt-linux"
mkdir -p "$B/dist" "$B/packaging" "$B/assets/icon" "$B/scripts"
cp "$BIN" "$B/dist/Snipt-linux-x86_64"
cp "$ROOT/packaging/snipt.desktop" "$B/packaging/"
cp "$ROOT/assets/icon/snipt-256.png" "$ROOT/assets/icon/snipt-512.png" "$ROOT/assets/icon/snipt.svg" "$B/assets/icon/"
cp "$ROOT/scripts/install-linux.sh" "$B/scripts/"
cp "$ROOT/LICENSE" "$B/"
chmod 0755 "$B/dist/Snipt-linux-x86_64" "$B/scripts/install-linux.sh"
tar --owner=0 --group=0 --sort=name -czf "$OUT/Snipt-linux-x86_64.tar.gz" -C "$STAGE" snipt-linux

(cd "$OUT" && sha256sum Snipt-arm64.apk Snipt-linux-x86_64.tar.gz > SHA256SUMS)
echo "Release v$VER assets in $OUT:"
cat "$OUT/SHA256SUMS"
