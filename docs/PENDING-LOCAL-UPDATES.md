# Snipt — Pending Local Updates (For Local AI Agent Execution)

This file lists every native Android (Kotlin), Rust (`sms-engine` / `sms-store` / Tauri), APK build, device verification, and GitHub push task that **cannot** be executed inside the web preview sandbox (which has no Rust/Cargo toolchain, Android SDK/NDK, or `.git` remote).

All TypeScript UI/UX, Glass OS styling, 5 themes, italic `snipt` header, Companion Mode UI + Default SMS RCS-off alert dialog, Contacts UI/engine, New SMS composer, Finance upgrades (Manual Expense/Income, Transaction Detail editor, Accounts & Cards + EPF mask, Analytics charts, Savings Goals), and Stock Market Portfolio & Broker SMS parser are **already implemented and verified in `src/`**.

---

## 1. Android Kotlin — Snipt RCS Companion Mode (`NotificationListenerService` + `RemoteInput`)

**Goal:** Let Snipt operate fully on Android 13–16 **without** becoming the Default SMS App, keeping Google Messages as default so native E2E RCS remains active.

### Files to add / modify
1. `plugins/tauri-plugin-telephony/android/src/main/java/com/snipt/telephony/SniptNotificationListenerService.kt` (NEW)
   - Extend `android.service.notification.NotificationListenerService`.
   - Filter `sbn.packageName` for `com.google.android.apps.messaging` (and optionally Samsung Messages `com.samsung.android.messaging`).
   - Extract `NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(sbn.notification)`:
     - Parse sender display name, phone/URI (`person.uri` / `tel:`), message text, timestamp, and `conversationTitle`.
     - Detect RCS vs SMS cues and emit a Tauri native event `companion-message-received` (or directly invoke Rust `ingest_incoming` via JNI/channel) with `protocol: "RCS"` when Google Messages is active.
   - Cache the latest `Notification.Action` containing a `RemoteInput` per conversation key (`Map<String, PendingReplyAction>` in memory, keyed by normalized sender phone/conversation key).
   - Implement `sendCompanionReply(addressOrKey: String, body: String): Boolean`:
     - Look up the cached `RemoteInput` + `PendingIntent`.
     - Populate a `Bundle` with `RemoteInput.addResultsToIntent(...)` and fire `action.actionIntent.send(context, 0, intent)`.
     - Return `false` if no active notification `RemoteInput` exists (UI falls back to opening SMS intent or sending SMS via `SmsManager` if user grants `SEND_SMS`).
2. `plugins/tauri-plugin-telephony/android/src/main/AndroidManifest.xml`
   - Register `<service android:name="com.snipt.telephony.SniptNotificationListenerService" android:permission="android.permission.BIND_NOTIFICATION_LISTENER_SERVICE" android:exported="true">` with `<intent-filter><action android:name="android.service.notification.NotificationListenerService" /></intent-filter>`.
3. `plugins/tauri-plugin-telephony/android/src/main/java/com/snipt/telephony/TelephonyPlugin.kt`
   - Add commands:
     - `isNotificationListenerEnabled(): Boolean` (checks `Settings.Secure.ENABLED_NOTIFICATION_LISTENERS`).
     - `openNotificationListenerSettings()` (`Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)`).
     - `sendCompanionReply(address: String, body: String): Boolean`.
   - Update `capabilities()` to expose `companionEnabled: Boolean` alongside `defaultSmsRole: Boolean`.
   - Keep `requestDefaultSmsRole()` intact — the React UI (`SettingsSheet.tsx`) now gates it behind the explicit **RCS-off confirmation alert dialog**.

---

## 2. Android Kotlin — Contacts Permission (`READ_CONTACTS`) & Name Resolution

**Goal:** Resolve saved phonebook names onto all SMS/RCS threads and populate the `+ New SMS` contact picker on real devices.

### Files to modify
1. `plugins/tauri-plugin-telephony/android/src/main/AndroidManifest.xml`
   - Add `<uses-permission android:name="android.permission.READ_CONTACTS" />`.
2. `plugins/tauri-plugin-telephony/android/src/main/java/com/snipt/telephony/TelephonyPlugin.kt`
   - Add runtime permission check/request command `requestContactsPermission(): Boolean` (requests `Manifest.permission.READ_CONTACTS`).
   - Add `listContacts(): List<ContactDto>` querying `ContactsContract.CommonDataKinds.Phone.CONTENT_URI` (`DISPLAY_NAME`, `NUMBER`, `TYPE`), deduplicated by last-10-digit normalized phone number.
   - Add `resolvePhoneNumbers(numbers: List<String>): Map<String, String>` using `ContactsContract.PhoneLookup.CONTENT_FILTER_URI` (or batch cache) so thread list rows resolve in O(1) without blocking the UI thread.
3. `src/api/telephony.ts` & `src/App.tsx`
   - Bridge `requestContactsPermission()` and `listContacts()` when `platform === 'android'`, replacing the local demo contacts seed with real Android contacts while keeping `src/engine/contacts.ts` as the shared normalizer/matcher.

---

## 3. Rust Core (`crates/sms-engine` & `crates/sms-store`) — Parity Port

Currently, **Stocks**, **Manual Expenses**, **Transaction Overrides**, **EPF Balance**, and **Savings Goals** persist in `localStorage` in the web/preview layer so they work immediately. Port them into SQLite + Rust for encrypted `.mysms.enc` backup parity:

1. **`crates/sms-engine/src/stocks.rs`** (NEW)
   - Mirror `src/engine/stocks.ts`:
     - `parse_stock_sms(body, sender, timestamp_ms)` for NSE/BSE/CDSL/NSDL/Zerodha/Groww/Upstox Buy, Sell, and Dividend alerts.
     - `compute_stock_portfolio(trades, ltp_map)` with FIFO cost basis, unrealized/realized P&L, dividends, and Indian STCG (20%) / LTCG (12.5% above ₹1.25L) estimates.
     - Hook `parse_stock_sms` into the ingestion pipeline so real incoming broker SMS automatically populate the Stocks ledger.
2. **`crates/sms-store/src/schema.sql` & migrations**
   - Add tables:
     - `stock_trades(id, symbol, exchange, side, quantity, price_per_share, gross_value, charges, broker, timestamp_ms, source, raw_sms, note)`
     - `stock_ltp(symbol PRIMARY KEY, price, updated_at_ms)`
     - `manual_transactions(id, direction, amount, category, note, date_iso, timestamp_ms, payment_method)`
     - `txn_overrides(id PRIMARY KEY, category, subcategory, note, flagged)`
     - `savings_goals(id, title, nickname, target_amount, saved_amount, start_date, target_date)`
   - Include these tables in `backup_create` / `backup_restore` (bump `schemaVersion: 2` with backward-compatible v1 import).
3. **`src-tauri/src/commands.rs`**
   - Expose Tauri commands for stocks, manual transactions, transaction overrides, and goals, and wire `src/api/` wrappers to call Tauri commands on native builds while keeping `devFallback` for web dev.

---

## 4. Local Build, Test & Device Verification Checklist

Run these commands on your local machine after applying Sections 1–3:

```bash
# 1. Verify TypeScript unit tests (existing 11 parity tests + new contacts/stocks/finance tests)
npx tsx --test src/engine/engine.test.ts src/engine/features.test.ts

# 2. Verify Rust workspace tests & Clippy
cargo test --workspace
cargo clippy --workspace -- -D warnings

# 3. Verify frontend production build & lint
npm run lint
npm run build

# 4. Build Android APK (release or debug)
npm run tauri android build -- --apk

# 5. Install on connected Android 15/16 device and verify:
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

### Real-Device Verification Matrix
- [ ] **Companion Mode**: Keep Google Messages as Default SMS App -> grant Snipt Notification Access -> send an RCS message from another phone -> confirm Snipt ingests it offline, extracts OTP/UPI/Stock data, and replies via Google Messages `RemoteInput` while E2E RCS stays active.
- [ ] **Default SMS RCS Warning**: Open Settings -> Messaging Mode -> click **Switch to Default SMS App...** -> confirm the **RCS-off pop-up alert** appears before Android's `RoleManager` prompt.
- [ ] **Contacts Permission**: Grant `READ_CONTACTS` -> confirm phone-number threads display real contact names and `+ New SMS` searches the device phonebook.
- [ ] **Themes & 120 Hz Scroll**: Switch across all 5 palettes (`Glass OS`, `Emerald`, `Warm Sand`, `Midnight`, `Ink Mono`) in Light, Dark, and Pure Black OLED; confirm 0 idle frames in GPU profiling.

---

## 5. Git Commit & Push to GitHub

Because the cloud preview workspace does not contain `.git`, copy the updated files into your local repository clone and push:

```bash
git status
git add index.html src/ docs/NEXT-UPDATE-RCS.md docs/PENDING-LOCAL-UPDATES.md
git commit -m "feat: Glass OS themes, RCS Companion Mode, Contacts, New SMS, Finance upgrades & Stocks tracker"
git push origin main
```
