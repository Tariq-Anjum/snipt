#!/usr/bin/env bash
# User-scoped Snipt installer (no sudo). Usage: install-linux.sh [--dry-run] [--uninstall]
# Never touches user data in ~/.local/share/app.snipt.sms.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ID=app.snipt.sms
DRY=0; UNINSTALL=0
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    --uninstall) UNINSTALL=1 ;;
    -h|--help) sed -n 2,3p "$0"; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
: "${HOME:?HOME unset}"
# Derived from HOME only (not XDG_DATA_HOME) so a throwaway HOME is fully isolated.
DATA="$HOME/.local/share"
BINDIR="$HOME/.local/bin"
ICONS="$DATA/icons/hicolor"
APPS="$DATA/applications"

# source -> destination (exactly the files this script owns)
SRC_BIN="$ROOT/dist/Snipt-linux-x86_64"
declare -a SRC=("$SRC_BIN" "$ROOT/packaging/snipt.desktop" "$ROOT/assets/icon/snipt-256.png" "$ROOT/assets/icon/snipt-512.png" "$ROOT/assets/icon/snipt.svg")
declare -a DST=("$BINDIR/snipt" "$APPS/snipt.desktop" "$ICONS/256x256/apps/$ID.png" "$ICONS/512x512/apps/$ID.png" "$ICONS/scalable/apps/$ID.svg")
declare -a MODE=(0755 0644 0644 0644 0644)

run() { if [ "$DRY" = 1 ]; then echo "[dry-run] $*"; else "$@"; fi; }

refresh_caches() {
  if command -v update-desktop-database >/dev/null; then run update-desktop-database "$APPS" || true; fi
  if command -v gtk-update-icon-cache >/dev/null && [ -f "$ICONS/index.theme" ]; then
    run gtk-update-icon-cache -f -t "$ICONS" || true
  fi
}

if [ "$UNINSTALL" = 1 ]; then
  for d in "${DST[@]}"; do
    if [ -e "$d" ] || [ -L "$d" ]; then run rm -f "$d"; echo "removed $d"; fi
  done
  # remove only now-empty dirs we may have created (rmdir fails safely if non-empty)
  for d in "$ICONS/256x256/apps" "$ICONS/512x512/apps" "$ICONS/scalable/apps" "$ICONS/256x256" "$ICONS/512x512" "$ICONS/scalable" "$ICONS" "$DATA/icons" "$APPS"; do
    [ "$DRY" = 1 ] || rmdir "$d" 2>/dev/null || true
  done
  [ "$DRY" = 1 ] || rmdir "$BINDIR" 2>/dev/null || true
  refresh_caches
  # update-desktop-database writes cache files; if they are all that is left, drop them.
  if [ "$DRY" != 1 ] && [ -d "$APPS" ]; then
    left="$(ls -A "$APPS" | grep -vxE 'mimeinfo.cache|intent.cache' || true)"
    if [ -z "$left" ]; then rm -f "$APPS/mimeinfo.cache" "$APPS/intent.cache"; rmdir "$APPS" 2>/dev/null || true; fi
  fi
  echo "uninstalled (user data in $DATA/$ID left untouched)"
  exit 0
fi

for s in "${SRC[@]}"; do
  [ -f "$s" ] || { echo "missing source: $s$([ "$s" = "$SRC_BIN" ] && echo ' (run scripts/build-linux.sh first)')" >&2; exit 1; }
done
for i in "${!SRC[@]}"; do
  run install -D -m "${MODE[$i]}" "${SRC[$i]}" "${DST[$i]}"
  echo "installed ${DST[$i]}"
done
refresh_caches
case ":$PATH:" in *":$BINDIR:"*) ;; *) echo "note: $BINDIR is not on PATH" ;; esac
