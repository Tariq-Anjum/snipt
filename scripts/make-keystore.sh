#!/usr/bin/env bash
# Creates a local release keystore OUTSIDE git. Run once. BACK UP the directory: losing it means installed apps can't be updated.
set -euo pipefail
D="$HOME/.config/mysms-signing"
[ -e "$D/mysms-release.jks" ] && { echo "keystore exists: $D"; exit 0; }
source "$(dirname "$0")/env.sh"
umask 077; mkdir -p "$D"
PW=$(head -c 24 /dev/urandom | base64 | tr -d '/+=' | head -c 24)
keytool -genkeypair -v -keystore "$D/mysms-release.jks" -alias mysms -keyalg RSA -keysize 4096 -validity 10000 \
  -storepass "$PW" -keypass "$PW" -dname "CN=mySMS personal release" >/dev/null 2>&1
printf 'storePassword=%s\nkeyPassword=%s\nkeyAlias=mysms\nstoreFile=%s\n' "$PW" "$PW" "$D/mysms-release.jks" > "$D/keystore.properties"
chmod 600 "$D"/*
echo "created $D (files 0600). Back it up."
