# mySMS — Implementation Plan

Inputs: BRIEF.md, ORIGIN.md, ARCHITECTURE.md, UI-PLAN.md, BATISSEUR-EXTENSION.md.
Labels: **[F]** verified · **[I]** inference · **[R]** recommendation · **[U]** verify in the slice.
Owners: **Opus-impl** (parity-, crypto-, security- or platform-critical code) · **Sonnet** (toolchain, scaffolding, wiring, packaging, docs). Reviewer: **Codex Sol-6.2** at checkpoints R0–R4, RB.
Rules for every slice: thin, one commit per green slice (local only, no push), report exact commands + output; a failed fix follows `/mnt/MD/AI-tools/local-policy/AGENT_CORRECTION_RETRY_RESEARCH.md` (1 attempt + max 3 evidence-backed corrections, then owner).

## Environment file (created in S7, sourced by every build command)
`scripts/env.sh` — no edits to `~/.bashrc` or system files:
```bash
export JAVA_HOME=/mnt/MD/AI-tools/jdk/current
export ANDROID_HOME=/mnt/MD/AI-tools/android-sdk
export ANDROID_SDK_ROOT=$ANDROID_HOME
export NDK_HOME=$ANDROID_HOME/ndk/29.0.14206865
export PATH=$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/build-tools/37.0.0:$PATH
export TZ_FIXTURE=Asia/Kolkata
```

## Slices

### S0 — Baseline the prototype (Sonnet)
Files: `package.json`, `package-lock.json` (new), `vite.config.ts` (`build.outDir: 'web-dist'`), `.gitignore` (+`web-dist/`, `src-tauri/target/`, `src-tauri/gen/android/keystore.properties`, `*.jks`), `index.html` (remove Google Fonts links).
Steps: remove `@google/genai`, `express`, `dotenv`, `@types/express` (unused [F grep]); add devDep `tsx@4.23.15`; `npm install`.
Accept: TS oracle 10/10 green; build passes; no network URLs in index.html.
Verify:
```bash
cd /mnt/MD/AI-tools/mySMS && TZ=Asia/Kolkata npx tsx --test src/engine/engine.test.ts
npm run lint && npm run build && ls web-dist/index.html
grep -rn "genai\|googleapis\|express" package.json index.html src || echo CLEAN
```

### S1 — Cargo workspace + sms-types (Opus-impl)
Files: `Cargo.toml` (workspace, exact `=` pins from ARCHITECTURE §11), `crates/sms-types/{Cargo.toml,src/lib.rs}`, `rust-toolchain.toml` (`channel = "1.98.1"`).
Accept: serde round-trip of every TS seed message JSON (exported in S2) is byte-stable after re-serialise.
Verify: `cargo test -p sms-types && cargo clippy -p sms-types -- -D warnings`

### S2 — Parity fixtures from the TS oracle (Opus-impl)
Files: `scripts/gen-fixtures.ts`, `crates/sms-engine/tests/fixtures/{seed_raw.json,analyze.json,threads.json,cycles.json,budget.json,scan.json,translate.json,bulk_sample.json}`, `crates/sms-crypto/tests/fixtures/ts_backup.mysms.enc` (passphrase is a test constant, not a secret).
Content: `analyzeMessage` output for every seed message + first 2,000 bulk messages (fixed `nowMs`), `buildThreads` summaries, cycle windows for days 1/15/28 across month ends, budget stats, weekly scan report, translations for all 7 languages, FNV hashes incl. non-ASCII strings.
Accept: generator deterministic (two runs → identical sha256).
Verify:
```bash
TZ=Asia/Kolkata npx tsx scripts/gen-fixtures.ts && sha256sum crates/*/tests/fixtures/* > /tmp/f1
TZ=Asia/Kolkata npx tsx scripts/gen-fixtures.ts && sha256sum crates/*/tests/fixtures/* | diff - /tmp/f1 && echo DETERMINISTIC
```

### S3 — sms-engine core: dlt, otp, finance, reminder, ml, thread, engine (Opus-impl)
Files: `crates/sms-engine/src/{lib,dlt,otp,finance,reminder,ml,thread,engine}.rs`, `tests/parity_core.rs`.
Hazards to encode: FNV over UTF-16 low bytes; ASCII `(?-u:\b)`; float tolerance 1e-9 (ARCHITECTURE §3).
Accept: ports of TS tests 1 (DLT), 2 (OTP), 3 (finance), FNV part of 8; `analyze.json` 100 % equal on category/reason/otp/txn/reminder, scores within 1e-9.
Verify: `cargo test -p sms-engine --test parity_core`

### S4 — sms-engine: budget, cleanup, translate + sms-testdata (Opus-impl)
Files: `crates/sms-engine/src/{budget,cleanup,translate}.rs`, `crates/sms-testdata/src/{lib,seed,bulk}.rs`, `tests/parity_rest.rs`.
Accept: ports of TS tests 4 (cycles/budget, `tz_offset_min=330`), 5 (weekly scan safety: never personal/transactions/starred/active reminders), 6 (≥ 10,000 generated, < 1 s release), 9 (7-language detection/translation); fixtures equal.
Verify: `cargo test -p sms-engine -p sms-testdata --release`

### S5 — sms-crypto: backup + vault (Opus-impl)
Files: `crates/sms-crypto/src/{lib,backup,vault,jstrim}.rs`, `tests/{backup.rs,vault.rs}`, `scripts/backup-xcheck.ts`.
Accept: TS tests 7 (vault round-trip) and 10 (backup round-trip, wrong passphrase → "Incorrect passphrase…", ciphertext lacks plaintext marker); TS fixture decrypts in Rust; Rust-written file decrypts in node; iterations cap; PBKDF2 600k timing recorded.
Verify:
```bash
cargo test -p sms-crypto
cargo run -p sms-crypto --example write_fixture -- /tmp/rust.mysms.enc
npx tsx scripts/backup-xcheck.ts /tmp/rust.mysms.enc   # prints messageCount, exit 0
```
**Checkpoint R1 (Codex Sol-6.2): engine parity + crypto.**

### S6 — sms-store (Opus-impl)
Files: `crates/sms-store/src/{lib,schema,ingest,threads,query,finance,search,migrate}.rs`, `tests/{store.rs,bench_10k.rs}`.
Accept: FTS5 trigram table creates (proves bundled FTS5); ingest 10k (classify + insert) ≤ 2 s release on this machine; thread page ≤ 5 ms; search ≤ 20 ms; `threads` table equals `threads.json` fixture for seed; store parts of TS test 8 (RCS filter, OTP purge, delete) pass; keyset paging has no gaps/dupes over 10k.
Verify: `cargo test -p sms-store --release -- --nocapture` (timings printed) and `cargo test --workspace`

### S7 — Toolchain install, user-scoped (Sonnet; owner gate for licence acceptance)
Side effects: ~5 GB under `/mnt/MD/AI-tools/{jdk,android-sdk}`; no sudo, no system files.
```bash
set -euo pipefail
mkdir -p /mnt/MD/AI-tools/jdk /mnt/MD/AI-tools/android-sdk/cmdline-tools
cd /mnt/MD/AI-tools/jdk
curl -fLO https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13.1%2B2/OpenJDK21U-jdk_x64_linux_hotspot_21.0.13.1_2.tar.gz
echo "bb8a93f9ae6e7a693b2b721e85dbeff26ae1a77447985a8d7afcdd9c7cf6222e  OpenJDK21U-jdk_x64_linux_hotspot_21.0.13.1_2.tar.gz" | sha256sum -c
tar xzf OpenJDK21U-jdk_x64_linux_hotspot_21.0.13.1_2.tar.gz && ln -sfn jdk-21.0.13.1+2 current
cd /mnt/MD/AI-tools/android-sdk
curl -fLO https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip
echo "e025545c62a8e64c7559119566a569fb1dec5f60  commandlinetools-linux-16111833_latest.zip" | sha1sum -c
unzip -q commandlinetools-linux-16111833_latest.zip -d cmdline-tools && mv cmdline-tools/cmdline-tools cmdline-tools/latest
source /mnt/MD/AI-tools/mySMS/scripts/env.sh
# OWNER GATE: Android SDK licence acceptance is the owner's legal act — owner runs:  ! bash -c 'source .../env.sh; sdkmanager --licenses'
sdkmanager --install "platform-tools" "platforms;android-37.0" "build-tools;37.0.0" "ndk;29.0.14206865" "emulator" "system-images;android-36;google_apis_ps16k;x86_64"
rustup target add aarch64-linux-android x86_64-linux-android   # aarch64 already present [F]
```
JDK 17 not needed (AGP 9.3 runs on 21) [I]; install Temurin 17.0.21.1+2 (sha256 `0b0ac5ea…f7b4`, api.adoptium.net) only if Gradle demands it.
Accept / Verify:
```bash
source scripts/env.sh && java -version 2>&1 | head -1 && sdkmanager --list_installed && ls $NDK_HOME/source.properties && adb version
```
Rollback: `rm -rf /mnt/MD/AI-tools/jdk /mnt/MD/AI-tools/android-sdk ~/.android/avd/mysms*`.

### S8 — Tauri app scaffold, desktop first (Sonnet; Opus reviews)
Files: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,src/main.rs,src/lib.rs,src/commands/*.rs,capabilities/default.json}`, devDeps `@tauri-apps/cli@2.12.6`, dep `@tauri-apps/api@2.12.5`.
Init: `npx tauri init --ci --app-name mysms --window-title mySMS --frontend-dist ../web-dist --dev-url http://localhost:3000 --before-dev-command "npm run dev" --before-build-command "npm run build"` [U flags: confirm with `npx tauri init --help`].
Commands (async): `threads_page, messages_page, search, mark_read, star, archive, block, delete_messages, finance_summary, budget_stats, set_cycle, set_budget, weekly_scan, apply_cleanup, otp_items, vault_reveal, translate, backup_create, backup_restore, capabilities, import_seed (DEV only)`.
Accept: `cargo tauri dev` equivalent opens the window on Hyprland; `capabilities()` returns linux; CSP set per ARCHITECTURE §10.
Verify: `npx tauri build --no-bundle && ls -la src-tauri/target/release/mysms && cargo test --workspace`

### S9 — UI port (Sonnet)
Files: `src/api/*.ts` (new), `src/App.tsx`, `src/components/{InboxView,FinanceView,RemindersView,CloudBackupSection,TopBar,NotificationBanner}.tsx`, `src/index.css`, `src/main.tsx` (fonts), package.json (+`@tanstack/react-virtual@3.14.26`, `@fontsource-variable/dm-sans@5.3.0`, `@fontsource-variable/plus-jakarta-sans@5.3.0`).
Per UI-PLAN §2–§4: invoke adapter, virtualization, LazyMotion, glass limits, Crisp auto-degrade, safe-area vars, back-stack, remove RustCoreView/rustWorkspace from bundle.
Accept: lint + build; bundle within budget (record numbers); `grep -c rustWorkspace web-dist/assets/*.js` = 0; manual Linux run with a 10k DB: all views render, search works.
Verify: `npm run lint && npm run build && du -b web-dist/assets/* && npx tauri build --no-bundle`

### S10 — Linux deliverable (Sonnet)
Files: `assets/icon/mysms.svg`, `src-tauri/icons/*` (from `npx tauri icon`), `scripts/build-linux.sh`, `scripts/install-linux.sh`, `packaging/mysms.desktop`.
Accept: `dist/mySMS-linux-x86_64` runs; install into a throwaway HOME places exactly the listed files; desktop file validates; `--uninstall` removes them; importing the S5 Rust fixture backup shows messages.
Verify:
```bash
bash scripts/build-linux.sh && file dist/mySMS-linux-x86_64
T=$(mktemp -d) && HOME=$T bash scripts/install-linux.sh && find $T -type f | sort && desktop-file-validate $T/.local/share/applications/mysms.desktop
HOME=$T bash scripts/install-linux.sh --uninstall && find $T -type f | wc -l   # expect 0
```
**Checkpoint R2 (Codex Sol-6.2): store + Tauri commands + Linux packaging.**

### S11 — Android init + empty-shell APK on emulator (Sonnet)
Files: `src-tauri/gen/android/**` (generated, committed), `tauri.conf.json` (`bundle.android.minSdkVersion: 29`, identifier).
```bash
source scripts/env.sh && npx tauri android init
npx tauri android build --apk --debug --target x86_64
avdmanager create avd -n mysms36 -k "system-images;android-36;google_apis_ps16k;x86_64" -d pixel_7
emulator -avd mysms36 -no-snapshot -no-audio        # run_in_background, then: adb wait-for-device
adb install -r <apk path printed by the build>     # [U] path; record it
```
Accept: app launches; inbox renders with an imported seed DB; `adb logcat` shows no crash; check whether the template adds INTERNET permission (`aapt2 dump permissions`) and record.

### S12 — tauri-plugin-telephony + JNI path (Opus-impl)
Files: `plugins/tauri-plugin-telephony/{Cargo.toml,build.rs,src/*.rs,permissions/*.toml,android/build.gradle.kts,android/src/main/AndroidManifest.xml,android/src/main/java/app/mysms/telephony/{TelephonyPlugin,SmsDeliverReceiver,MmsWapPushReceiver,SmsStatusReceiver,NotificationActionReceiver,RespondViaMessageService,ComposeTrampolineActivity,NativeCore,ProviderSync,Notifier,SecureClipboard,KeystoreWrap,DisplayBoost}.kt}`, `src-tauri/src/jni_bridge.rs`.
Accept (emulator):
```bash
adb shell cmd role add-role-holder android.app.role.SMS <appId> && adb shell cmd role get-role-holders android.app.role.SMS
adb emu sms send 5551234 "Your OTP is 482913 for HDFC Bank. Do not share."
adb shell content query --uri content://sms/inbox --projection address:body | tail -1     # row written by mySMS
adb shell dumpsys notification --noredact | grep -A3 <appId>                               # notification w/ actions
# app killed first (adb shell am force-stop <appId>) → repeat sms send → message present on next launch (JNI path)
# second AVD on port 5556: send from app to 5556, receive there (Dual emulator SEND path)
```
Plus: role-request UI flow, permission prompts, delete/mark-read reflect in provider, Reply action from notification sends.

### S13 — Security and platform polish (Opus-impl for Kotlin, Sonnet for UI)
FLAG_SECURE (vault + optional global), `copySensitive` + 30 s wipe, Keystore-wrapped vault DEK, insets, predictive back, display boost, power-save → Crisp, `allowBackup=false`, PendingIntents `FLAG_IMMUTABLE` (mutable only for RemoteInput), receivers `exported` exactly as required by the system permissions, no message bodies in logs.
Verify: `adb exec-out screencap -p > /tmp/v.png` on vault screen is black [I]; `adb shell dumpsys window | grep -i secure`; `adb logcat -d | grep -c 482913` = 0; clipboard empty after 30 s (`adb shell cmd clipboard get-primary-clip` [U availability]).
**Checkpoint R3 (Codex Sol-6.2): Kotlin plugin + JNI + Android security.**

### S14 — Backup via SAF + cross-device restore (Sonnet)
Android SAF create/open; Linux file dialog (`tauri-plugin-dialog 2.8.4`).
Accept: backup created on emulator into Downloads (`adb pull`), restored on Linux binary with the same passphrase; wrong passphrase rejected on both.

### S15 — MMS receive (Opus-impl; owner decides ship-blocking)
`MmsWapPushReceiver` → `SmsManager.downloadMultimediaMessage` → Rust `mms-pdu` parser (text + image parts) → provider + store.
Verify: parser unit tests from captured PDU samples; [U] end-to-end needs a carrier — emulator cannot deliver real MMS.

### S16 — Signed release APK (Sonnet; keystore = owner-run)
```bash
! bash scripts/make-keystore.sh            # OWNER runs; writes ~/.config/mysms-signing/{mysms-release.jks,keystore.properties} 0600
source scripts/env.sh && npx tauri android build --apk --target aarch64
cp <release apk> dist/mySMS-arm64.apk
apksigner verify --print-certs dist/mySMS-arm64.apk
zipalign -c -P 16 -v 4 dist/mySMS-arm64.apk | tail -1        # 16 KB alignment [U flag support]
unzip -l dist/mySMS-arm64.apk | grep -E "lib/arm64-v8a/.*\.so"
git status --porcelain | grep -E "\.jks|keystore\.properties" && echo LEAK || echo NO-SECRETS-TRACKED
```

### S17 — Docs (Sonnet)
`docs/RESULT.md` (exact phone + Linux install steps, what was and was not verified), `docs/BATISSEUR-LOG.md` (route outputs before/after extension, gaps, timings).
**Checkpoint R4 (Codex Sol-6.2): final review; blockers fixed before RESULT.md is marked done.**

### B1 — batisseur extension (Sonnet; can run in parallel after R0)
Per BATISSEUR-EXTENSION.md. **Checkpoint RB (Codex Sol-6.2).**

Order: S0 → S1 → S2 → S3 → S4 → S5 (R1) → S6 → S8 → S9 → S10 (R2) ; S7 can run in parallel from S1; S11 → S12 → S13 (R3) → S14 → S16 → S17 (R4); S15 when owner decides; B1 anytime after R0.

## Test strategy
- **Rust parity vs the 10 TS tests** (each ported 1:1, same inputs/asserts) + fixture-equality over seed + 2,000 bulk messages:

| TS test | Rust test |
|---|---|
| 1 DLT sender parsing | sms-engine `parity_core::dlt_sender_parsing` |
| 2 OTP extractor | `parity_core::otp_extractor` |
| 3 Finance UPI/CC/Savings | `parity_core::finance_methods` |
| 4 Billing cycle + budget | `parity_rest::cycles_and_budget` |
| 5 Weekly useless scan | `parity_rest::weekly_scan_safety` |
| 6 10k generator | sms-testdata `bulk_10k` |
| 7 OTP vault AES-GCM | sms-crypto `vault::roundtrip` |
| 8 Reminder, FNV, RCS filter, OTP purge | `parity_core::reminder_fnv` + sms-store `rcs_filter_and_purge` |
| 9 Offline translate 7 langs | `parity_rest::translate_seven` |
| 10 PBKDF2 backup round-trip + wrong pass | sms-crypto `backup::roundtrip_and_reject` + node cross-check |

- Whole-repo gate: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && TZ=Asia/Kolkata npx tsx --test src/engine/engine.test.ts && npm run lint && npm run build`.
- Android: emulator (KVM available: `/dev/kvm` crw-rw-rw- [F]) for role, receive (`adb emu sms send`), provider writes, notifications, emulator-to-emulator send, SAF to local storage, FLAG_SECURE screencap, killed-process receive.
- **Cannot be verified without the owner's phone (state this in RESULT.md):** real carrier send/receive and delivery reports; Dual-SIM (emulator has one SIM); carrier MMS download; RCS (not available to third-party apps at all); 90/120 Hz and LTPO behaviour and real jank/thermal; OEM background restrictions; SAF to Google Drive/OneDrive/Dropbox apps (no accounts on emulator); arm64 native performance (emulator is x86_64 — arm64 APK is only checked for structure/signature, not executed, unless the owner sideloads).

## Risks
| Risk | Impact | Mitigation |
|---|---|---|
| MMS lost while mySMS is default before S15 | data loss | warning screen; owner decision; S15 |
| RCS ends when leaving Google Messages | feature loss | explicit consent text on role request |
| Regex/float/timezone parity drift | wrong categories | fixture tests, `(?-u:\b)`, tz param |
| Tauri plugin ↔ background receiver lifecycle | missed SMS | JNI path independent of WebView; killed-process test |
| WebView glass jank on mid phones | UX | UI-PLAN rules + auto Crisp |
| Edge-to-edge insets not reported to WebView CSS | clipped UI | Kotlin-injected CSS vars |
| New RustCrypto generation (aes-gcm 0.11) API changes | build friction | S5 compile check; fall back only with evidence |
| Tauri 3 alpha / template drift | churn | exact pins; no `cargo update` without review |
| Keystore loss | cannot update app | owner backs up `~/.config/mysms-signing` |
| Name "mysms" collision | Play listing later | owner decision before Play |
| Android SDK licence acceptance | legal | owner-run step |

## Codex Sol-6.2 review checklist
- **R0 (this plan):** Tauri decision evidence; crate split; backup format compatibility; Kotlin surface vs default-SMS requirements; unverifiable list honest.
- **R1:** every TS test ported with equal asserts; fixtures generated from TS, not hand-written; FNV UTF-16 detail; `\b` handling; tz param; no `unwrap` on untrusted input; PBKDF2 params, nonce randomness (`getrandom`), constant-time tag check (aes-gcm), zeroize of keys/passphrases; iteration cap.
- **R2:** SQL injection-free (parameters only); WAL + busy_timeout; migrations; FTS triggers consistent on delete; keyset paging correctness; command input validation; CSP and capabilities least-privilege; install script touches only listed paths, no sudo.
- **R3:** manifest permissions/exports; receivers protected by `BROADCAST_SMS`/`BROADCAST_WAP_PUSH`; PendingIntent mutability; provider writes on deliver; JNI error handling (no panics across FFI: `catch_unwind`); no PII/OTP in logs; FLAG_SECURE coverage; sensitive clipboard; Keystore wrap; `allowBackup=false`; no INTERNET permission in release (or justified).
- **R4:** deliverables exist and verify (`apksigner`, install script dry run); RESULT.md claims match evidence; no secrets tracked (`git ls-files | grep -E 'jks|keystore|\.env'` empty); Gemini key absent (`git grep -n "AIza"` empty).
- **RB:** batisseur `cargo fmt --check`, clippy, tests, bench 16/16 unchanged, new tests, tt-1.1.0 untouched (sha256 matches provenance.json).
