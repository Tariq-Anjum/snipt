# Android notes (S11)

## Commands run (cwd mySMS, `source scripts/env.sh` first)
1. `npx tauri android init` (tauri.conf.json `bundle.android.minSdkVersion: 29` set first). Template pins untouched: Gradle 9.6.1, Kotlin 2.2.10 (AGP 9.3.1 per template). Also installed rust targets aarch64/armv7/i686/x86_64-linux-android.
2. `npx tauri android build --apk --debug --target x86_64`
3. `avdmanager create avd -n mysms36 -k "system-images;android-36;google_apis_ps16k;x86_64" -d pixel_7`
4. `emulator -avd mysms36 -no-window -no-snapshot -no-audio -gpu swiftshader_indirect` (background); `adb wait-for-device`; poll `getprop sys.boot_completed` (boot ~40 s).
5. `adb install -r <apk>`; `adb shell monkey -p app.snipt.sms -c android.intent.category.LAUNCHER 1`; `adb logcat -d`; `adb exec-out screencap -p`; `aapt2 dump permissions <apk>`; `adb emu kill`.

## Result
- APK: `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk` (gitignored), 195 MB (203,753,523 B); contains only `lib/x86_64/libsnipt_lib.so` (188.6 MB, unstripped debug). Release/aarch64 is S16.
- Cold build 2 failed runs total ~4.5 min (first Gradle run also auto-installed SDK Platform 36).
- App launches; process alive; screenshot shows the Snipt inbox UI rendering (empty state "No messages yet", tabs, bottom nav). logcat: no FATAL / AndroidRuntime crash / SIGSEGV for the app (only the monkey launcher's own AndroidRuntime lines).
- Not verified: inbox with an imported seed DB (no import path on device yet).
- Permissions (aapt2): the template DOES add `android.permission.INTERNET`; also `app.snipt.sms.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`. Decide later (S13) whether to remove INTERNET (Tauri dev server needs it only in dev; remove for release via manifest override if the no-network goal holds).
- AVD description shows "16 KB Page Size" tag (page_size_16kb image).

## Problems
- Build failed first: Gradle rust plugin runs `npm run tauri android android-studio-script`, and package.json had no `tauri` script ("Missing script: tauri"). Fix: added `"tauri": "tauri"` to package.json scripts.
- Java 8 source/target deprecation warnings from template (harmless).
- `keystore.properties` and `*.jks` are gitignored (gen/android/.gitignore + root .gitignore; verified with git check-ignore).

# S12 / S13 — telephony plugin, JNI path, security polish (2026-11-22)

## What exists
- `plugins/tauri-plugin-telephony` (Rust + Kotlin library `app.snipt.telephony`). Desktop build: `status` all-false, `send`/role/copy return Unsupported, toggles no-op; the Linux binary stays a read-only viewer (`canSend=false`).
- `src-tauri/src/jni_bridge.rs`: `NativeCore` JNI surface (init with Keystore DEK, ingest, ingestBatch, queueOutgoing, setSendStatus, markRead, deleteMessage, otpCode, reconcile, emit). Every entry is in `catch_unwind`; failures return null/false; nothing is logged. Window and receivers share one process-wide core (`AppState::shared`: one SQLite connection, one vault). Native events reach the UI as Tauri events (`sms-received`, `sms-changed`, `send-status`, `compose`, `back`).
- **Pin deviation:** `jni =0.21.1`, not 0.22.4 (ARCHITECTURE §6.3): 0.21.1 is the version Tauri 2.12.7/wry already link on Android (Cargo.lock), so no second JNI crate is built. Licence MIT/Apache-2.0.
- Manifest (plugin): SMS_DELIVER (BROADCAST_SMS), WAP_PUSH_DELIVER mime `application/vnd.wap.mms-message` (BROADCAST_WAP_PUSH; S15 stub that logs only "MMS not supported"), RESPOND_VIA_MESSAGE service (SEND_RESPOND_VIA_MESSAGE), compose trampoline for SENDTO/VIEW `sms|smsto|mms|mmsto` + SEND text/plain; own status/action receivers `exported=false`. Checked against developer.android.com `android.provider.Telephony` "Creating an SMS app". API levels checked offline in `platforms/android-37.0/data/api-versions.xml` (setRequestedFrameRate/REQUESTED_FRAME_RATE_CATEGORY_HIGH 35, EXTRA_IS_SENSITIVE 33, createForSubscriptionId 31, FLAG_MUTABLE 31, getSlotIndex 29, getCurrentThermalStatus 29, clearPrimaryClip 28).
- Permissions (aapt2): RECEIVE_SMS, READ_SMS, SEND_SMS, RECEIVE_MMS, RECEIVE_WAP_PUSH, READ_PHONE_STATE (Dual-SIM subscription list), POST_NOTIFICATIONS (+ AndroidX's DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION). **No INTERNET in any build type**; `allowBackup=false`, `fullBackupContent=false`, data-extraction rules exclude everything; `enableOnBackInvokedCallback=true`. Granting the SMS role granted all seven runtime permissions (`GRANTED_BY_ROLE`).
- `tauri android dev` (live dev server) would need INTERNET: add `<uses-permission android:name="android.permission.INTERNET"/>` in a local, uncommitted `app/src/debug/AndroidManifest.xml` when using it.
- gradle.properties: `-Xmx2g`, `org.gradle.workers.max=4`; builds ran with `CARGO_BUILD_JOBS=8` (OOM incident).

## Commands (cwd mySMS, `source scripts/env.sh`)
`npx tauri android build --apk --debug --target x86_64`; `emulator -avd mysms36 -memory 2048 -no-window -no-snapshot -no-audio -gpu swiftshader_indirect`; `adb install -r <apk>`; `adb shell cmd role add-role-holder android.app.role.SMS app.snipt.sms`; `adb emu sms send <from> "<text>"`; notification actions tapped via `cmd statusbar expand-notifications` + `uiautomator dump` + `input tap`; app DB pulled with `adb exec-out run-as app.snipt.sms cat snipt.db{,-wal,-shm}` and queried on the host (only masked facts printed).

## Verified on the emulator (android-36 x86_64, 2026-11-22)
- Role: `get-role-holders` -> `app.snipt.sms`; UI banner hidden once held (capabilities merge live `status`).
- Receive with UI open: `content://sms/inbox` row written by Snipt (`_id=1`), app DB row with `provider_id=1`, thread appears live in the inbox (screenshot).
- Notification: channel `snipt_codes`, `vis=PRIVATE` + public version, 4 actions (Copy code, Reply, Mark read, Delete); text shows `Your OTP is •••••• ...`.
- Killed process: `am force-stop`, then `sms send` -> process started with no activity (`MainActivity` count 0), provider row `_id=2` + DB row `provider_id=2` (JNI path).
- Mark read action -> provider `read=1` and DB `is_read=1`. Delete action -> provider row and DB row gone. Reply (RemoteInput) -> provider `type=2 status=0`, DB `send_status=delivered`, original marked read; notification cleared by the timed update.
- First-run import after `pm clear` + role: `inbox import: inserted=5 skipped=0`, `reconcile: {"deleted":0,"markedRead":2}`.
- SENDTO `sms:5556` -> draft sheet pre-filled; Send -> provider SENT row, DB `delivered` (UI `queue_outgoing` -> plugin send).
- FLAG_SECURE: Vault tab -> window `fl=... SECURE ...`, `screencap` all black; back key -> web layer pops to inbox (predictive-back callback + `back` event), SECURE cleared, screenshot normal.
- Clipboard: Copy code -> `SniptClip: sensitive clip set; wipe in 30 s` at 10:27:22.737, `sensitive clip wiped` at 10:27:52.755.
- logcat (`-b all`): 0 hits for the OTP `482913` and for every test body; no FATAL/SIGSEGV/panic.

## Not verified / limits
- Emulator-to-emulator: a second AVD on 5556 booted; Snipt's send showed SENT + delivered, but 5556's `content://sms/inbox` had no row. Cause not established (likely no default SMS app writing the provider on the fresh AVD); stopped there because free RAM fell under 5 GiB. Treat cross-device receive as unverified. Emulator delivery reports are synthetic (a send to 5556 with no receiver running also reported delivered).
- Clipboard *contents* (and EXTRA_IS_SENSITIVE on the clip) could not be read from the shell (`cmd clipboard` has no shell implementation on this image); only the wipe events are verified. The wipe runs in-process; if the process dies within 30 s, Android's own clipboard auto-clear is the backstop. The "user copied something else" cancel path is untested (cannot set the clipboard from the shell).
- Provider -> app reconcile for changes made by other apps: covered by a unit test only (`adb shell content delete/update` on `content://sms` is a silent no-op for the shell user).
- Delivery status: status PendingIntents are FLAG_IMMUTABLE, so the status-report PDU extra is not attached; "delivered" = a status report arrived (report failure codes not distinguished). Multipart: any failed part marks failed (tracked in-process).
- RESPOND_VIA_MESSAGE service: not exercised (only the phone app holds SEND_RESPOND_VIA_MESSAGE).
- Dual-SIM (emulator has one SIM), real carrier send/receive and delivery reports, MMS (S15), RCS, OEM background limits, arm64 execution, refresh-rate boost effect (no 90/120 Hz panel; only "no crash" verified), insets on notched/gesture devices beyond the emulator screenshot.
- Import covers the provider inbox only; existing sent messages in `content://sms/sent` are not imported (the store has no provider-outgoing ingest yet).
- OTP codes are stored in plaintext in the app-private DB (store design); the Keystore-wrapped DEK protects vault payloads crossing IPC, not the DB at rest (SQLCipher is an open owner decision).

# S14 - encrypted backup via native pickers (2026-11-22)
- Android: `TelephonyPlugin.safSave` (`ACTION_CREATE_DOCUMENT`, `application/octet-stream`, `EXTRA_TITLE` = writer's `mysms-backup-YYYY-MM-DD.mysms.enc`) and `safOpen` (`ACTION_OPEN_DOCUMENT`, `*/*` + octet-stream/json/text). Ciphertext is streamed via `ContentResolver` (`"wt"` output, buffered writer; partial doc deleted on failure); errors carry no URI/name/content; nothing logged. Read cap 64 MiB in Kotlin (core accepts 256 MiB). Still no INTERNET (aapt2 count 0). The calls are Rust-only (not in the webview permission set); the app commands `backup_save` / `backup_restore_picked` keep ciphertext and passphrase out of JS.
- Linux: `tauri-plugin-dialog =2.8.4` (exists on crates.io, not yanked), desktop-only target dependency, called from Rust (`src-tauri/src/picker.rs`); no capability grants the webview any `dialog:*` permission. Files are written 0600.
- UI: `caps.nativePicker` (true on Android/Linux, false in the dev browser) picks native vs download/file-input; passphrase state is cleared after every use; wrong passphrase shows the core's message plus a retry hint. Provider cards are a hint only; SAF lists the installed cloud apps.
- Verified on emulator (android-36 x86_64, debug APK; webview driven over CDP because the vault tab is FLAG_SECURE and uiautomator cannot see webview content; the SAF picker itself was real DocumentsUI, tapped with adb): Create -> picker opened in Downloads with the suggested name -> Save -> `/sdcard/Download/mysms-backup-2026-11-22.mysms.enc` (4088 B, kept `.mysms.enc`), 11 messages; `adb pull`; file contains none of the seeded bodies or OTP; `scripts/backup-xcheck.ts` decrypts it (11 msgs, 600000 iters) and rejects a wrong passphrase; logcat -b all: 0 hits for bodies/OTP. Restore via the open picker: wrong passphrase -> clear error; right passphrase -> "Restored 0 messages (11 already present)"; passphrase field empty after each.
- Not verified: picker cancel path on device, real Drive/OneDrive/Dropbox providers (none installed on the emulator), Linux dialogs (builds, no display-driven run), files above ~64 MiB, arm64.
