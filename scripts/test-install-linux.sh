#!/usr/bin/env bash
# Regression test for scripts/install-linux.sh (R2 #8) in throwaway HOMEs with
# a fake source tree: destination preflight, exact-file install (-T), absolute
# Exec= in the desktop entry, uninstall leaving shared caches alone.
# Usage: bash scripts/test-install-linux.sh   (no network, no real HOME)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAKE="$WORK/tree"
mkdir -p "$FAKE/scripts" "$FAKE/dist" "$FAKE/packaging" "$FAKE/assets/icon"
cp "$ROOT/scripts/install-linux.sh" "$FAKE/scripts/"
cp "$ROOT/packaging/snipt.desktop" "$FAKE/packaging/"
printf '#!/bin/sh\necho fake\n' >"$FAKE/dist/Snipt-linux-x86_64"
for f in snipt-256.png snipt-512.png snipt.svg; do echo "$f" >"$FAKE/assets/icon/$f"; done

fail() { echo "FAIL: $*" >&2; exit 1; }
inst() { env -u XDG_DATA_HOME -u XDG_CONFIG_HOME -u XDG_CACHE_HOME HOME="$1" bash "$FAKE/scripts/install-linux.sh" "${@:2}"; }

# 1. A directory where the binary goes: refused before anything is written.
H1="$WORK/h1"; mkdir -p "$H1/.local/bin/snipt"
if inst "$H1" >/dev/null 2>&1; then fail "install over a directory succeeded"; fi
[ -z "$(ls -A "$H1/.local/bin/snipt")" ] || fail "something was copied into the directory"
[ ! -e "$H1/.local/share/applications/snipt.desktop" ] || fail "partial install after preflight failure"

# 2. A symlink destination is refused too (never followed or replaced).
H2="$WORK/h2"; mkdir -p "$H2/.local/share/applications"; ln -s /etc/hostname "$H2/.local/share/applications/snipt.desktop"
if inst "$H2" >/dev/null 2>&1; then fail "install over a symlink succeeded"; fi

# 3. Normal install: exact files, absolute Exec=, shared caches survive uninstall.
H3="$WORK/h3 space"; mkdir -p "$H3/.local/share/applications"
echo "keep" >"$H3/.local/share/applications/mimeinfo.cache"
echo "keep" >"$H3/.local/share/applications/intent.cache"
inst "$H3" >/dev/null
[ -x "$H3/.local/bin/snipt" ] || fail "binary missing"
[ -f "$H3/.local/share/icons/hicolor/scalable/apps/app.snipt.sms.svg" ] || fail "icon missing"
grep -qx "Exec=\"$H3/.local/bin/snipt\"" "$H3/.local/share/applications/snipt.desktop" || fail "Exec= not absolute: $(grep '^Exec=' "$H3/.local/share/applications/snipt.desktop")"
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$H3/.local/share/applications/snipt.desktop" 2>&1 | grep -v 'hint:' | grep . && fail "desktop-file-validate errors" || true
fi
inst "$H3" --uninstall >/dev/null
[ ! -e "$H3/.local/bin/snipt" ] || fail "binary not removed"
[ ! -e "$H3/.local/share/applications/snipt.desktop" ] || fail "desktop entry not removed"
[ -f "$H3/.local/share/applications/mimeinfo.cache" ] || fail "shared mimeinfo.cache removed"
[ -f "$H3/.local/share/applications/intent.cache" ] || fail "shared intent.cache removed"

# 4. R4: a literal % in the install path is encoded as %% in Exec=.
H4="$WORK/50%test"; mkdir -p "$H4"
inst "$H4" >/dev/null
grep -qx "Exec=\"$WORK/50%%test/.local/bin/snipt\"" "$H4/.local/share/applications/snipt.desktop" \
  || fail "literal % not encoded: $(grep '^Exec=' "$H4/.local/share/applications/snipt.desktop")"
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$H4/.local/share/applications/snipt.desktop" 2>&1 | grep -v 'hint:' | grep . && fail "desktop-file-validate errors (% path)" || true
fi

# 5. R4: a dry run needs no writable temp directory and writes nothing.
H5="$WORK/h5"; mkdir -p "$H5"
TMPDIR=/nonexistent-snipt-tmp inst "$H5" --dry-run >"$WORK/dry.txt" 2>&1 || fail "dry run failed: $(cat "$WORK/dry.txt")"
grep -q '^\[dry-run\] desktop entry: Exec=' "$WORK/dry.txt" || fail "dry run did not report the launcher"
[ -z "$(ls -A "$H5")" ] || fail "dry run wrote files"

echo "install-linux.sh regression: OK"
