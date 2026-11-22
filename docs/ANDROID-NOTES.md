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
- `src-tauri/src/jni_bridge.rs`: `NativeCore` JNI surface (init with Keystore DEK, ingest, ingestBatch, queueOutgoing, beginSend/bindSend/sendPartResult/failSend, actionTarget, markRead, deleteMessage, otpCode, reconcileBegin/reconcile, emit; R2/R3 revision below). Every entry runs its whole body, including JString/byte-array conversion and the result `NewStringUTF`, inside `catch_unwind`; failures return null/false/-1; nothing is logged. Window and receivers share one process-wide core (`AppState::shared`: one SQLite connection, one vault). Native events reach the UI as Tauri events (`sms-received`, `sms-changed`, `send-status`, `compose`, `back`).
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
- Delivery status: superseded by the R2/R3 revision below (mutable delivery intents, TP-Status interpreted, results persisted).
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

# R2/R3 review fixes (2026-11-22)

## What changed on Android
- **Send attempts.** `SmsSender` asks the core for a send attempt (`beginSend`, AUTOINCREMENT id, never reused), writes the OUTBOX row and binds it once (`bindSend`; a different provider id later is refused). Each part gets its own sent and delivery PendingIntent made unique by the data URI `snipt-send://<attempt>/<part>/<kind>` (request code 0; `Intent.filterEquals` compares data), so parts past 32 and concurrent sends never collide or overwrite extras. Sent intents stay FLAG_IMMUTABLE; delivery intents are explicit + FLAG_MUTABLE so the system attaches `pdu`/`format`.
- **Persisted results, monotonic state.** `SmsStatusReceiver` stores every (attempt, part, kind) result in the core (`sendPartResult`); the message state is derived from all stored results and only moves forward (`queued < sending < sent < unconfirmed < delivered`; `failed`/`delivered` terminal). Nothing lives in process memory. TP-Status (3GPP TS 23.040): `0x00-0x1F` ok, `0x20-0x3F` still trying (ignored, final report follows), `>= 0x40` failed; a missing/unparseable/3GPP2 report is `unknown` -> **unconfirmed** ("Sent · delivery unconfirmed"), never "delivered". The provider row is updated only when the core reports a change.
- **Inline reply.** "Sending reply..." after the radio accepted the request; "Reply sent"/"Reply not sent" only from the sent callback (`previous == sending`).
- **Notification actions** carry the message id and sender; `actionTarget` re-validates both (ids are never reused, so a deleted source never matches) and returns the provider id (the intent no longer carries one). A stale action is dropped with its notification and a toast. Replies pass `threadId` + `address` + `replyTo`; the store rejects a mismatched recipient or a deleted source.
- **Delete / mark read (provider first).** `ProviderSync.delete/markRead` check the SMS role, write, then read the row back; results are per row. The core changes the local row only for verified provider rows (UI `delete_messages` returns `failedIds`; `mark_read` errors and the message stays unread); notification actions keep the notification and toast on failure.
- **Reconcile without the snapshot race.** `reconcileBegin` captures the local `(id, providerId)` candidates first, then the provider is queried, then the core deletes only candidates still bound to the same provider id. Inbox rows the store never ingested (and did not delete locally: `provider_tombstones`; rows at or below the v1 replay floor are excluded) are replayed by provider id; rows younger than 30 s are left for the next pass (a receipt may be mid-flight).
- **Clipboard.** Each copy carries a random ownership token in the clip description; the 30 s wipe clears only if the current clip still carries it. If Android hides the clipboard (app not focused), nothing is cleared and the OS expiry is the backstop (best-effort, logged). `copySensitive` resolves only after `setPrimaryClip` succeeded; the UI countdown starts only then and an Android failure is reported (no silent web fallback).
- **FLAG_SECURE.** `setSecure` resolves with the window's FLAG_SECURE state read back on the UI thread. Vault reveal controls are disabled until it is confirmed; the thread's Reveal acquires the vault scope first (hidden if unconfirmed) and releases it on hide/30 s/unmount. Settings has a global "Block screenshots" switch (Android only), re-applied on start.
- **Vault readiness.** No vault payloads are issued or decrypted before the Keystore key is installed (no throwaway-key payloads); `NativeCore.ensure` retries key installation if the Keystore failed. SMS receipt does not wait for the key.
- **Optionals.** `Recipients` splits the *encoded* scheme-specific part before decoding (`%26` stays in the body, `%25` decoded once; first recipient only); the compose trampoline accepts only SENDTO/VIEW with sms/smsto/mms/mmsto or SEND text/plain; RESPOND_VIA_MESSAGE sends on `SubscriptionManager.getDefaultSmsSubscriptionId()`.
- **JNI failure policy.** null/false/-1 only; a Java exception left pending by a failed JNI call (e.g. allocation failure in `NewStringUTF`) is cleared before returning.
- **Webview surface.** The app declares an ACL manifest (`src-tauri/build.rs`); only the UI's commands are granted (`src-tauri/permissions/ui.toml`). `set_send_status` and `provider_*` are no longer webview commands; `ingest_incoming` (dev simulator) is registered only in debug/`dev-seed` builds.

## Verified on the emulator (android-36 x86_64, debug APK, 2026-11-22 11:22-11:32)
Installed over the S14 build: v1 DB migrated to schema v2 (`user_version=2`, `id_alloc`/`send_attempts`/`provider_tombstones` present, `provider_replay_floor=15`), first reconcile `deleted=0 markedRead=0 missing=0`. Inbox renders through the new ACL. No INTERNET permission (aapt2).
- Thread view: OTP bodies show `••••••`; Reveal -> window `SECURE` (dumpsys) and screencap all black; Hide -> `SECURE` cleared.
- Copy OTP (focused): `sensitive clip set; ownership-checked wipe in 30 s` -> `sensitive clip wiped` 30 s later. Copy then HOME: `clip ownership not verifiable; left to the OS clipboard expiry`.
- Receive with UI open: provider `_id=16` + notification on `snipt_codes`, `vis=PRIVATE`, 4 actions, text `Your OTP is •••••• ...`.
- Killed process (`am force-stop`): provider `_id=17`, notification posted, no MainActivity.
- Notification Mark read: provider `read=1`, notification cancelled, inbox row read. Notification Delete: provider row and DB row gone, notification cancelled. Copy code action: clip set.
- Stale action: message deleted in the UI (provider row verified gone), then its notification's Mark read -> `stale notification action ignored`, notification removed.
- Inline reply: provider SENT `_id=19`; DB `send_attempts(1, msg 1016, parts 1, provider 19)`, `send_part_results` sent=ok + delivery=ok, `send_status=delivered`, provider `status=0` after the report; notification text "Reply sent".
- UI open thread -> provider `read=1`; UI delete -> provider row gone.
- Role removed (`cmd role remove-role-holder`): opening an unread thread and deleting it left provider and DB rows unchanged (`mark read failed for 1 row(s)`, `delete failed for 1 row(s)`; DB `is_read=0`, row kept). The in-thread failure message was hidden in read-only mode; fixed in `b37c0e1` (UI only, not re-run on the emulator).
- FLAG_SECURE: vault tab -> `SECURE`, back -> cleared; global "Secure" toggle -> `SECURE`, kept after force-stop + relaunch, toggled off -> cleared.
- `logcat -b all` since the reply step: 0 hits for the test OTPs/bodies.
- Observed once at first launch after install: the WebView renderer crashed (SIGSEGV in `art::KernelSupportsUffd` under the renderer seccomp sandbox, Chromium/emulator), taking the app down (`Render process ... crash wasn't handled`); the relaunch and all later runs were fine. Not caused by Snipt code; not reproduced.

## Not verified
- Removing the SMS role kills the app process and Android removes its notifications, so the "notification action keeps the notification on provider failure" path was not exercised on the device (code path shared with the verified UI failure path).
- "User copied something else" before the wipe (shell cannot set the clipboard); covered by the token check, not exercised.
- Reconcile race and missed-receipt replay: unit tests only (`crates/sms-store/tests/send_state.rs`); the shell cannot write `content://sms`.
- Multipart (> 1 part) and carrier failure reports on real radios; TP-Status interpretation is only exercised with the emulator's synthetic success report.
- RESPOND_VIA_MESSAGE default-subscription path, Recipients edge cases (no JVM unit-test harness in the plugin; would need Robolectric).

# S15 - MMS receive (text + image parts), 2026-11-22

## Sources checked (primary)
- developer.android.com `SmsManager`: `downloadMultimediaMessage(Context, String locationUrl, Uri contentUri, Bundle configOverrides, PendingIntent downloadedIntent)` API 21 ("contentUri: the content uri to which the downloaded pdu will be written"; result `Activity.RESULT_OK` or `MMS_ERROR_*` 1..12); `sendMultimediaMessage` (same result codes). `Telephony` "Creating an SMS app" (WAP_PUSH_DELIVER + BROADCAST_WAP_PUSH); `Telephony.Sms.Intents.WAP_PUSH_DELIVER_ACTION` extras (`data` = payload, `subscription` optional long); `Telephony.BaseMmsColumns`, `Mms.Addr` (TYPE = PduHeaders FROM/TO), `Mms.Part` (CONTENT_URI API 29, `getPartUriForMessage`/`getAddrUriForMessage` API 30, so `content://mms/<id>/part|addr` is built by hand for minSdk 29).
- AOSP source (googlesource, main): `MmsServiceBroker.downloadMessage` enforces **RECEIVE_MMS** + appop and grants the phone process READ|WRITE on `contentUri` *from the caller's uid* (so our provider must have `grantUriPermissions`); `sendMessage` enforces **SEND_SMS**. `packages/services/Mms`: `MAX_MMS_FILE_SIZE = 8 MiB`, PDU written with `openFileDescriptor(contentUri, "w")`, auto-persist pref defaults to false (the default app persists); "AcknowledgeInd and NotifyRespInd are parts of downloading sequence" sent by the app. AOSP Messaging (reference default app): FileProvider for the PDU, `Mms.DATE` in **seconds**, NotifyRespInd(Retrieved) after download with `locationUrl = null`.
- OMA-TS-MMS_ENC-V1_3-20110913-A (Tables 3/4/5/25, §7) and WAP-230-WSP-20010705-a (8.4.2, 8.4.2.24, 8.5, Tables 38/39/42); WSP content-type codes from Wireshark `packet-wsp.c` (WINA table is not in the PDF).
- **INTERNET**: not needed. The HTTP fetch runs in the platform MmsService over the carrier MMS APN; aapt2 permission count for INTERNET = 0 (debug APK), and the release merged manifest has none. CSP unchanged; images reach the WebView only as base64 -> `blob:`.

## What exists
- `crates/sms-mms` (no deps, `forbid(unsafe)`): M-Notification.ind, M-Retrieve.conf (multipart related/mixed/alternative, depth <= 2, <= 32 parts, <= 8 MiB, <= 128 headers), charsets UTF-8/ASCII/Latin-1/UCS-2/UTF-16, `/TYPE=PLMN` stripping, M-NotifyResp.ind encoder, magic-byte image sniffing (JPEG/PNG/GIF/WebP/BMP; never SVG). Parts are byte ranges into the input. Example `make_fixture` writes the debug fixtures.
- Store schema **v3**: `messages.mms_id` (unique; SMS `provider_id` and MMS `mms_id` are separate namespaces), `mms_parts` (image bytes <= 5 MiB, <= 32 per message, <= 8 MiB total, JPEG thumb <= 256 KiB, FK cascade), `mms_pending` (transaction-keyed: location, expiry, attempts <= 3, state).
- Flow: `MmsWapPushReceiver` -> `NativeCore.mmsNotification` (parse + dedup) -> `MmsDownloader.start` (FileProvider `app.snipt.sms.snipt.mms` over `cacheDir/snipt-mms/`, subclass `SniptMmsFileProvider` to avoid merging with the template's FileProvider; immutable explicit PendingIntent `snipt-mms://download/<id>`) -> `MmsDownloadReceiver` -> `mmsRetrieved` -> `MmsProviderWriter` (content://mms row + addr FROM/TO + parts; text/plain and SMIL in `text`, binary streamed to the part URI; all-or-nothing) -> `mmsIngest` (body = text parts, else subject, else `[Photo]`/`[Attachment]`; classified like SMS, OTP masked at the IPC edge) -> `BitmapFactory` thumbnail (bounds-first, sample size, <= 480 px, 40 MP bomb guard) -> `mmsThumb` -> notification -> M-NotifyResp.ind via `sendMultimediaMessage`.
- Failures: result != OK -> retry on the next trigger (app start/resume, next WAP push); expired or 3 attempts -> placeholder message "Multimedia message could not be downloaded." in the sender's thread + notification.
- UI: new command `mms_image` (app manifest + `ui` set; `tests/acl.rs` passes). Bubbles show image thumbnails (tap -> full-size viewer portalled to `<body>`) and "Attachment not supported" chips (name, size). Delete / mark read go provider-first per namespace (`ProviderIds.mms`); notification actions carry `mmsId`.
- Debug-only entry: `src/debug` source set of the plugin (`MmsDebugReceiver`, `android:permission="android.permission.DUMP"`), absent from the release merged manifest (`processUniversalReleaseMainManifest`: 0 hits).
- MMS **send** is out of scope (replies to an MMS go as SMS). Group MMS: recipients are written to the provider, but Snipt threads by sender only.

## Commands (emulator android-36 x86_64, debug APK)
`cargo run -p sms-mms --features fixture --example make_fixture -- <dir>`; `adb push <dir>/*.pdu /data/local/tmp/`; `adb shell run-as app.snipt.sms sh -c 'mkdir -p files/debug-mms && cp /data/local/tmp/*.pdu files/debug-mms/'`; `adb shell am broadcast -a app.snipt.sms.DEBUG_MMS -n app.snipt.sms/app.snipt.telephony.MmsDebugReceiver --es mode fixture|download|maintenance`.

## Verified on the emulator (2026-11-22 15:04-15:16)
- Guard: `am broadcast -a android.provider.Telephony.WAP_PUSH_DELIVER ... -n .../MmsWapPushReceiver` -> `SecurityException: Permission Denial: not allowed to send broadcast ... WAP_PUSH_DELIVER from uid=2000` (protected action); `dumpsys package` lists the receiver for the action + MIME.
- Fixture path: provider `content://mms` `_id=2` (`msg_box=1 m_type=132 sub=Photo tr_id=SNIPT-FIX-1 ct_t=...multipart.related text_only=0`), addr rows FROM 137 / TO 151, parts SMIL/text(chset 106)/image.png with a provider part file; `content read content://mms/part/4` is byte-identical to the fixture PNG (18,564 B). App DB `user_version=3`, message `mms_id=2 provider_kind=mms`, `otp_json` set, `mms_parts` image 18,564 B + JPEG thumb 1,160 B (96x64), `mms_pending` done. Inbox and thread show the masked OTP and the thumbnail; tap opens the full-size viewer.
- `download` mode (real `downloadMultimediaMessage`): result 7 (`MMS_ERROR_CONFIGURATION_ERROR`, no MMSC on the emulator) -> retry, retry (via `maintenance`), give up after 3 -> placeholder row + notification "Multimedia message could not be downloaded.". Re-sending a handled notification -> "duplicate MMS notification ignored". NotifyResp send returned 12 (`MMS_ERROR_MMS_DISABLED_BY_CARRIER`).
- First run found a bug: SMIL streamed to a part URI -> FileNotFoundException -> provider row dropped (message still stored). Fixed (SMIL in `text`), re-verified above.
- S12 smoke on the S15 APK: receive with UI open (provider `_id=21`, DB row), killed-process receive (provider `_id=22`, process started with no MainActivity), notification Mark read (provider `read=1`, DB `is_read=1`), notification Delete (provider and DB rows gone). `logcat -b all`: 0 hits for the OTP and test bodies; no FATAL/SIGSEGV.

## Not verifiable without a carrier / not verified
- Real WAP push delivery, real MMSC download (HTTP, APN, carrier config overrides), NotifyResp acceptance by an MMSC, Dual-SIM subscription routing for downloads, real-world PDUs (carrier quirks, Content-Disposition 1.4, unusual charsets, large images/HEIC), expiry timing against real networks.
- Retries only fire on app start/resume or the next WAP push (no AlarmManager/JobScheduler); a failed download with the app never opened waits until then (expiry still yields a placeholder).
- Group-MMS threading, MMS in backups (`.mysms.enc` carries the text body only, no attachments), MMS in the first-run provider import/reconcile (SMS only), video/audio/vCard display (shown as unsupported).
