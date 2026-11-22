#!/usr/bin/env bash
# Build the Snipt Linux binary -> dist/Snipt-linux-x86_64 (+ SHA256SUMS).
# Assumes `npm ci` was already run. Memory-safe: see docs/INCIDENT-20261122-OOM.md.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"

for c in node npm cargo; do
  command -v "$c" >/dev/null || { echo "missing: $c" >&2; exit 1; }
done
[ -d node_modules ] || { echo "node_modules missing: run 'npm ci' first" >&2; exit 1; }
# The artifact is published as x86-64: refuse other hosts (no cross build here).
[ "$(uname -m)" = x86_64 ] || { echo "host is $(uname -m); this script only builds the x86-64 artifact" >&2; exit 1; }
command -v file >/dev/null || { echo "missing: file (needed for the artifact architecture check)" >&2; exit 1; }

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
# The frontend is built once, by Tauri's beforeBuildCommand (npm run build).
echo "== tauri build --no-bundle (CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS)"
npx tauri build --no-bundle >"$LOG" 2>&1 || { tail -40 "$LOG"; exit 1; }
tail -3 "$LOG"

# Shared workspace target dir, not src-tauri/target.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')"
BIN="$TARGET_DIR/release/snipt"
[ -x "$BIN" ] || { echo "binary not found: $BIN" >&2; exit 1; }

# Verify the architecture before publishing under the x86-64 name.
case "$(file -b "$BIN")" in
  *"ELF 64-bit"*x86-64*) ;;
  *) echo "binary is not an x86-64 ELF: $(file -b "$BIN" | cut -c1-120)" >&2; exit 1 ;;
esac

mkdir -p dist
OUT=dist/Snipt-linux-x86_64
cp -f "$BIN" "$OUT.tmp"
mv -f "$OUT.tmp" "$OUT"
# Release profile may already strip; strip is a no-op safety net.
command -v strip >/dev/null && strip --strip-unneeded "$OUT" 2>/dev/null || true
chmod 0755 "$OUT"
( cd dist && sha256sum Snipt-linux-x86_64 > SHA256SUMS && cat SHA256SUMS )
echo "size: $(du -h "$OUT" | cut -f1) ($(stat -c %s "$OUT") bytes)"
file "$OUT" | cut -c1-160
