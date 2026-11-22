# Snipt — result (2026-11-22)

Snipt (working repo name mySMS; app id `app.snipt.sms`) = one Rust core + shared Liquid Glass web UI (Tauri 2) for Android (full default-SMS app) and Linux (backup reader).

## What you can install now
| Target | Artifact | Check |
|---|---|---|
| Android arm64 phone | `dist/Snipt-arm64.apk` (13.8 MB, v1.0.0, v2-signed with your release key, no INTERNET permission) | `cd dist && sha256sum -c SHA256SUMS` ; steps in `docs/INSTALL-ANDROID.md` |
| Linux x86_64 (this machine) | `dist/Snipt-linux-x86_64` (14 MB) + `scripts/install-linux.sh` (user-scoped, `--dry-run`, `--uninstall`) | `bash scripts/install-linux.sh --dry-run` then run it without the flag |

Back up `~/.config/mysms-signing/` (release keystore): losing it means the app cannot be updated in place.

## Verified (commands I ran myself at the end)
- `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --release --locked`: **149 passed, 0 failed**. TS oracle (original prototype tests): **11/11**. `npm run lint`, `npm run build` OK; production bundle contains no dev/fallback engine.
- Rust engine is a byte/parity port of the TS engine (fixtures generated from the TS oracle; 10,000-message ingest ~0.4-0.7 s, search ~3 ms).
- APK: `apksigner verify` OK, package/version/permissions as expected, arm64-v8a only, 16 KB aligned. Linux install/uninstall regression script passes (`scripts/test-install-linux.sh`).
- Emulator (Android 16, x86_64): default-SMS role, receive with app open/killed, notification actions (mark read, delete, reply, masked copy), delete/mark-read sync, FLAG_SECURE vault, SAF encrypted backup create/restore, MMS parse->provider->store->UI via a debug fixture, R8-shrunk signed release build smoke.

## NOT verified (needs your phone / carrier)
arm64 code execution (only the x86_64 build was run), real carrier send/receive and delivery reports, **real MMS download**, Dual-SIM, RCS (not available to third-party apps; leaving Google Messages ends RCS), OEM battery limits, 90/120 Hz behaviour, Drive/OneDrive/Dropbox providers. Linux GUI was launched headlessly only (no screenshot of the real window).

## Independent review (Codex Sol-6.2) — honest status: NOT READY
Five review rounds are in `docs/reviews/` (R1-R5). Rounds R1-R4 findings were fixed and re-verified. **R5 (final) still lists 7 blockers**; under the retry policy (1 attempt + 3 corrections) the autonomous budget is spent, so a 4th correction needs your decision:
1. v4 migration can fail on a DB that contains thread ids above 2^40 (only reachable when upgrading an already-saturated pre-v4 DB; a fresh install on your phone is not affected).
2. Restoring your own backup can swap archive/reminder flags between two byte-identical messages.
3. Android: deleting a message during a live send can remove the OUTBOX row; core init has a rare double-init race.
4. SMS replay can re-add a message that cleanup deleted in a narrow race window.
5. MMS: a repeat WAP push can discard a final successful download attempt; provider write precedes core ingest.
6. MMS transaction ids are treated as globally unique (two MMS with the same id on different SIMs collapse).
7. A >40 MP image bypasses the native thumbnail pixel cap and reaches the WebView.
Plus optionals (retry UI when capabilities fail, MMS temp-file cleanup, "some parts omitted" flag). My assessment: 1 and 4 are edge cases, 2-3 are rare, 5-7 only affect MMS, which the emulator cannot exercise anyway. For daily use the risk is low-to-moderate; for a Play Store release these should be fixed first.

## Decisions waiting for you
1. Allow a 4th correction round for R5 blockers (recommended before Play Store; optional for personal use)?
2. MMS: ship as-is (receive only, retry on app open, text-only backups) or add background retry/backups with attachments?
3. SQLCipher at rest for the SMS database (OTPs are plaintext inside the app-private DB; vault key is Keystore-wrapped)?
4. Trademark/Play-name check for "Snipt" and rename of the GitHub repo (yours to run).
5. Push: nothing has been pushed. Local commits only in `/mnt/MD/AI-tools/snipt` (and one in `/mnt/MD/AI-tools/batisseur`, 0.3.0 with the `phone` form factor and `android-phone` pack).

## Incident
Mid-run the session was killed by a kernel OOM (a 10 GB `script` process in the terminal that hosts herdr). Not a herdr bug; details and mitigations in `docs/INCIDENT-20261122-OOM.md`.
