#!/usr/bin/env bash
# User-scoped Snipt installer (no sudo). Usage: install-linux.sh [--dry-run] [--uninstall]
# Never touches user data in ~/.local/share/app.snipt.sms or shared desktop/icon caches.
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

# source -> destination (exactly the files this script owns). The desktop
# entry is generated at install time with the absolute binary path in Exec=,
# so the launcher does not depend on the graphical session's PATH.
SRC_BIN="$ROOT/dist/Snipt-linux-x86_64"
SRC_DESKTOP="$ROOT/packaging/snipt.desktop"
declare -a SRC=("$SRC_BIN" "$SRC_DESKTOP" "$ROOT/assets/icon/snipt-256.png" "$ROOT/assets/icon/snipt-512.png" "$ROOT/assets/icon/snipt.svg")
declare -a DST=("$BINDIR/snipt" "$APPS/snipt.desktop" "$ICONS/256x256/apps/$ID.png" "$ICONS/512x512/apps/$ID.png" "$ICONS/scalable/apps/$ID.svg")
declare -a MODE=(0755 0644 0644 0644 0644)

run() { if [ "$DRY" = 1 ]; then echo "[dry-run] $*"; else "$@"; fi; }
die() { echo "error: $*" >&2; exit 1; }

# A destination must be absent or a regular file (never a directory, symlink,
# fifo, ...), and every existing ancestor up to $HOME must be a real directory.
# Checked for all files before anything is written or removed.
preflight() {
  local d p
  for d in "${DST[@]}"; do
    if [ -L "$d" ]; then die "$d is a symlink; refusing to replace it"; fi
    if [ -e "$d" ] && [ ! -f "$d" ]; then die "$d exists and is not a regular file"; fi
    p="$(dirname "$d")"
    while [ "$p" != "$HOME" ] && [ "$p" != "/" ]; do
      if [ -L "$p" ] || { [ -e "$p" ] && [ ! -d "$p" ]; }; then
        die "$p exists and is not a plain directory"
      fi
      p="$(dirname "$p")"
    done
  done
}

refresh_caches() {
  # The cache tools own their cache files; this script never deletes them.
  if command -v update-desktop-database >/dev/null && [ -d "$APPS" ]; then run update-desktop-database "$APPS" || true; fi
  if command -v gtk-update-icon-cache >/dev/null && [ -f "$ICONS/index.theme" ]; then
    run gtk-update-icon-cache -f -t "$ICONS" || true
  fi
}

# Desktop Entry spec quoting for an Exec argument: a literal % is written
# %% (otherwise "%t" in a path is a field code), then the quoted-argument
# escapes for \ " ` $.
exec_quote() {
  local s="$1"
  s="${s//%/%%}"
  s="${s//\\/\\\\}"; s="${s//\"/\\\"}"; s="${s//\`/\\\`}"; s="${s//\$/\\\$}"
  printf '"%s"' "$s"
}

preflight

if [ "$UNINSTALL" = 1 ]; then
  for d in "${DST[@]}"; do
    if [ -f "$d" ]; then run rm -f -- "$d"; echo "removed $d"; fi
  done
  # Remove only now-empty dirs we may have created (rmdir fails safely if non-empty).
  for d in "$ICONS/256x256/apps" "$ICONS/512x512/apps" "$ICONS/scalable/apps" "$ICONS/256x256" "$ICONS/512x512" "$ICONS/scalable"; do
    [ "$DRY" = 1 ] || rmdir "$d" 2>/dev/null || true
  done
  refresh_caches
  echo "uninstalled (user data in $DATA/$ID and shared caches left untouched)"
  exit 0
fi

for s in "${SRC[@]}"; do
  [ -f "$s" ] || { echo "missing source: $s$([ "$s" = "$SRC_BIN" ] && echo ' (run scripts/build-linux.sh first)')" >&2; exit 1; }
done

EXEC_LINE="Exec=$(exec_quote "$BINDIR/snipt")"
grep -q '^Exec=' "$SRC_DESKTOP" || die "$SRC_DESKTOP has no Exec= line"
render_desktop() {
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      Exec=*) printf '%s\n' "$EXEC_LINE" ;;
      TryExec=*) ;;
      *) printf '%s\n' "$line" ;;
    esac
  done <"$SRC_DESKTOP"
}
# The rendered launcher needs a temporary file only for a real install; a
# dry run writes nothing (it works without a writable temp directory).
TMP_DESKTOP="(rendered $SRC_DESKTOP)"
if [ "$DRY" != 1 ]; then
  TMP_DESKTOP="$(mktemp)"
  trap 'rm -f "$TMP_DESKTOP"' EXIT
  render_desktop >"$TMP_DESKTOP"
fi

for i in "${!SRC[@]}"; do
  src="${SRC[$i]}"
  [ "$src" = "$SRC_DESKTOP" ] && src="$TMP_DESKTOP"
  # -T: the destination is the file itself, never "a directory to copy into".
  run install -D -T -m "${MODE[$i]}" "$src" "${DST[$i]}"
  echo "installed ${DST[$i]}"
done
[ "$DRY" = 1 ] && echo "[dry-run] desktop entry: $EXEC_LINE"
refresh_caches
case ":$PATH:" in *":$BINDIR:"*) ;; *) echo "note: $BINDIR is not on PATH (the desktop launcher uses the absolute path)" ;; esac
