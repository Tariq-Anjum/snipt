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
[ "$(uname -m)" = x86_64 ] || echo "warning: host is $(uname -m), artifact name says x86_64" >&2

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT
echo "== npm run build"
npm run build >"$LOG" 2>&1 || { tail -30 "$LOG"; exit 1; }
tail -3 "$LOG"
echo "== tauri build --no-bundle (CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS)"
npx tauri build --no-bundle >"$LOG" 2>&1 || { tail -40 "$LOG"; exit 1; }
tail -3 "$LOG"

# Shared workspace target dir, not src-tauri/target.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')"
BIN="$TARGET_DIR/release/snipt"
[ -x "$BIN" ] || { echo "binary not found: $BIN" >&2; exit 1; }

mkdir -p dist
OUT=dist/Snipt-linux-x86_64
cp -f "$BIN" "$OUT"
# Release profile may already strip; strip is a no-op safety net.
command -v strip >/dev/null && strip --strip-unneeded "$OUT" 2>/dev/null || true
chmod 0755 "$OUT"
( cd dist && sha256sum Snipt-linux-x86_64 > SHA256SUMS && cat SHA256SUMS )
echo "size: $(du -h "$OUT" | cut -f1) ($(stat -c %s "$OUT") bytes)"
file "$OUT" | cut -c1-160
