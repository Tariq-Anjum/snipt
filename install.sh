#!/usr/bin/env bash
# Snipt one-click installer (downloads the official release, verifies SHA-256).
#
#   Linux desktop (x86_64, no sudo):
#     curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash
#   Android phone connected over USB (needs adb + USB debugging):
#     curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash -s -- --android
#   Remove the Linux app (keeps your data):
#     curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash -s -- --uninstall
#
# Options: --android | --uninstall | --dry-run (Linux: show what would change)
# Env: SNIPT_VERSION=v1.2.0 (default: latest release), SNIPT_REPO=owner/name,
#      SNIPT_ASSET_DIR=/path (use already-downloaded release files instead)
set -euo pipefail

REPO="${SNIPT_REPO:-Tariq-Anjum/snipt}"
VERSION="${SNIPT_VERSION:-latest}"
MODE=linux
PASS=()
for a in "$@"; do
  case "$a" in
    --android) MODE=android ;;
    --uninstall) PASS+=(--uninstall) ;;
    --dry-run) PASS+=(--dry-run) ;;
    -h|--help) sed -n 2,13p "$0" 2>/dev/null || true; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

if [ "$VERSION" = latest ]; then
  BASE="https://github.com/$REPO/releases/latest/download"
else
  BASE="https://github.com/$REPO/releases/download/$VERSION"
fi

die() { echo "snipt-install: $*" >&2; exit 1; }
for c in curl sha256sum; do command -v "$c" >/dev/null || die "'$c' is required"; done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fetch() { # fetch <asset>
  if [ -n "${SNIPT_ASSET_DIR:-}" ]; then
    cp "$SNIPT_ASSET_DIR/$1" "$TMP/$1" || die "missing $SNIPT_ASSET_DIR/$1"
    return
  fi
  echo "Downloading $1 ..."
  curl -fL --proto '=https' --tlsv1.2 --retry 3 -o "$TMP/$1" "$BASE/$1" || die "download failed: $BASE/$1"
}

verify() { # verify <asset>: checksum line from the release's SHA256SUMS
  local line
  line="$(grep -E "  $1\$" "$TMP/SHA256SUMS" || true)"
  [ -n "$line" ] || die "no checksum for $1 in SHA256SUMS"
  (cd "$TMP" && printf '%s\n' "$line" | sha256sum -c --quiet -) || die "checksum mismatch for $1"
  echo "Verified $1"
}

fetch SHA256SUMS

if [ "$MODE" = android ]; then
  ADB="$(command -v adb || true)"
  [ -n "$ADB" ] || { [ -n "${ANDROID_HOME:-}" ] && [ -x "$ANDROID_HOME/platform-tools/adb" ] && ADB="$ANDROID_HOME/platform-tools/adb"; } || true
  [ -n "$ADB" ] || die "adb not found (install Android platform-tools), or install the APK on the phone: $BASE/Snipt-arm64.apk"
  mapfile -t DEVICES < <("$ADB" devices | awk 'NR>1 && $2=="device" {print $1}')
  [ "${#DEVICES[@]}" -eq 1 ] || die "connect exactly one phone with USB debugging allowed (found ${#DEVICES[@]})"
  ABI="$("$ADB" -s "${DEVICES[0]}" shell getprop ro.product.cpu.abilist | tr -d '\r')"
  case ",$ABI," in *,arm64-v8a,*) ;; *) die "this release is for arm64 phones (device reports: $ABI)" ;; esac
  fetch Snipt-arm64.apk
  verify Snipt-arm64.apk
  echo "Installing on ${DEVICES[0]} (an update keeps your messages) ..."
  "$ADB" -s "${DEVICES[0]}" install -r "$TMP/Snipt-arm64.apk"
  cat <<'EOF'
Done. On the phone: open Snipt and tap "Set as default". If Android says
"Restricted setting", open Settings > Apps > Snipt > (top-right menu) >
"Allow restricted settings", then try again.
EOF
  exit 0
fi

[ "$(uname -s)" = Linux ] || die "the desktop app is Linux-only; for Android use --android"
[ "$(uname -m)" = x86_64 ] || die "the Linux build is x86_64 only (this machine: $(uname -m))"
command -v tar >/dev/null || die "'tar' is required"
fetch Snipt-linux-x86_64.tar.gz
verify Snipt-linux-x86_64.tar.gz
tar -xzf "$TMP/Snipt-linux-x86_64.tar.gz" -C "$TMP"
[ -x "$TMP/snipt-linux/scripts/install-linux.sh" ] || die "release bundle is incomplete"
bash "$TMP/snipt-linux/scripts/install-linux.sh" "${PASS[@]}"
