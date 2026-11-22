<div align="center">

# Snipt

**Snip the noise.** An offline-first SMS app for Android with a built-in money tracker, OTP vault and monthly budget planner, plus a Linux reader for your encrypted backups.

[Download for Android](https://github.com/Tariq-Anjum/snipt/releases/latest/download/Snipt-arm64.apk) · [Install on Linux](#linux-desktop) · [Features](#features) · [Privacy](#privacy) · [Build from source](#build-from-source)

![Android 10+](https://img.shields.io/badge/Android-10%2B%20arm64-3DDC84) ![Linux](https://img.shields.io/badge/Linux-x86__64-FCC624) ![No internet permission](https://img.shields.io/badge/internet-not%20required-2563EB) ![License](https://img.shields.io/badge/license-proprietary%2C%20source--available-lightgrey)

</div>

---

## Why Snipt

Most of the SMS on an Indian phone is noise: OTPs, promotions, bank alerts, delivery updates. Snipt sorts it for you, keeps one-time codes in an encrypted vault, turns bank and card alerts into a ledger and a monthly budget, and clears out the junk every week. It does all of this **on the phone, without an internet connection**: the Android app does not even request the internet permission.

## Install

### Android (one tap)

1. On your phone, download **[Snipt-arm64.apk](https://github.com/Tariq-Anjum/snipt/releases/latest/download/Snipt-arm64.apk)** and open it. Allow "Install unknown apps" for your browser or file manager when Android asks.
2. Open Snipt and tap **Set as default** (Android only lets the default SMS app receive, send and delete SMS).
3. If Android shows **"Restricted setting"**: Settings → Apps → Snipt → ⋮ (top right) → **Allow restricted settings**, then tap *Set as default* again.

Play Protect may warn that the app comes from an unregistered developer. That is a warning about the distribution channel, not a malware finding; tap *More details → Install anyway*.

**From a computer over USB** (adb, USB debugging on):

```bash
curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash -s -- --android
```

Updates install over the old version and keep your messages. To go back to your previous SMS app, pick it as the default SMS app again; your messages stay in Android's SMS store.

### Linux desktop

```bash
curl -fsSL https://raw.githubusercontent.com/Tariq-Anjum/snipt/main/install.sh | bash
```

User-scoped (no sudo): installs `~/.local/bin/snipt`, a launcher and icons. Add `--dry-run` to preview, `--uninstall` to remove (your data in `~/.local/share/app.snipt.sms` is kept). Needs `webkit2gtk-4.1` and `gtk3`. The Linux app opens and searches your encrypted Snipt backups; sending SMS needs the phone.

The installer downloads the release assets from GitHub and checks them against the release's `SHA256SUMS` before installing anything.

## Features

### Inbox that sorts itself
- Categories: **Personal, Priority, Transactions, Promotions, Spam**, plus Starred, Archived and RCS.
- Understands Indian **DLT sender headers** (`-S` service, `-T` transactional, `-P` promotional, `-G` government) and combines them with an on-device classifier and rules for OTPs, bills, reminders and service notices.
- Dual-SIM filter, full-text search across 10,000+ messages, conversation view with inline cards for OTPs, transactions and reminders.
- Business sender IDs that cannot receive replies are labelled as such instead of showing a useless reply box.
- **Offline translation** of Hindi, Urdu, Hinglish, Marathi, Tamil, Telugu and Bengali SMS to English.

### OTP vault
- One-time codes are encrypted with AES-256-GCM (Android Keystore-wrapped key) and shown masked until you tap Reveal.
- Copying a code marks it sensitive and wipes it from the clipboard after 30 seconds.
- The vault screen blocks screenshots and screen recording (`FLAG_SECURE`); optionally every screen does.
- Notification actions: **Copy code, Reply, Mark read, Delete**.

### Money: UPI, cards, bank accounts
- Reads bank, card and UPI alerts and builds a **ledger** with merchant, account, reference and balance.
- Handles the hard cases: card **available-limit** amounts are not spending, **bill-due reminders** and **payment-received** confirmations are not transactions, transfers between your own accounts (e.g. paying a card bill from savings) are not counted twice, credit-card spends made through UPI land in the card's cycle, and foreign-currency spends keep their currency.
- **Duplicate alerts count once**: the same SMS delivered twice, or a bank alert plus the shop's own SMS for the same purchase.
- Per-method **billing cycles** (UPI, credit card, savings account), Active Cycle vs All History, top merchants, latest balances and card limits.

### Monthly budget planner
- Spending is categorised automatically (Food & Dining, Transport & Travel, Bills & Utilities, Shopping & Retail, General & Other) with editable limits and On track / Near limit / Over budget status.
- **Monthly reset day** (1–28) for your budget month, **Start fresh now** (count only from today, undoable) and **Restore default limits**. Nothing is ever deleted; All History always shows everything.

### Home-screen widgets
- **Snipt messages**: unread count and the latest three conversations. Codes are never shown, and message text can be hidden in Settings.
- **Monthly budget**: spent vs limit with your top three categories. Tapping it opens the Finance tab.

### Cleanup and backup
- **Weekly useless-SMS scan**: expired OTPs, old promotions, spam/phishing and finished deliveries. It never touches personal messages, transactions, starred messages or active reminders.
- **Encrypted backup** (`.mysms.enc`, PBKDF2 + AES-256-GCM) to any folder or cloud app through the system file picker (Drive, OneDrive, Dropbox, local storage); restore merges without creating duplicates.

### Look and feel
- **Liquid Glass** or **Crisp Solid** surfaces; **System / Light / Dark** theme with a **Pure black** OLED mode; status-bar icons follow the theme.
- **Pinch to resize message text** (the page itself never zooms) or use the Text size slider.
- Built for 90/120 Hz screens: compositor-only animations, native scrolling, and an automatic drop to Crisp Solid when power saver is on.

## Privacy

Full policy: [PRIVACY.md](PRIVACY.md).

- **No internet permission** on Android. No accounts, analytics, ads or trackers.
- Messages are stored in an app-private SQLite database. Android cloud backup of app data is disabled; the only backup is the encrypted file you create yourself.
- OTP codes are only decrypted when you tap Reveal or Copy, and never appear in notifications.
- Widgets show no codes, and message previews in widgets can be turned off.

## What it does not do (yet)

- **RCS**: Google does not let third-party apps use RCS; RCS chats stay in Google Messages, and choosing Snipt as the default SMS app ends them for that number.
- **MMS**: receive only. Pictures are shown once Android has checked their size; very large images are not displayed. Sending MMS is not supported.
- Real-carrier coverage so far: Indian banks and UPI apps on one arm64 phone running Android 16. Dual-SIM routing and delivery reports have had limited real-world testing.

## Build from source

Requirements: Rust (see `rust-toolchain.toml`), Node 22+, JDK 21, Android SDK + NDK (for the APK), `webkit2gtk-4.1` (for Linux).

```bash
npm ci
cargo test --workspace --release --locked           # Rust core, store, crypto, MMS (+ TS parity fixtures)
TZ=Asia/Kolkata npx tsx --test src/engine/engine.test.ts   # TypeScript engine oracle
npm run lint                                         # type check

# Linux binary
bash scripts/build-linux.sh

# Android APK (arm64); release signing reads ~/.config/mysms-signing/keystore.properties
source scripts/env.sh   # adjust paths for your machine
npx tauri android build --apk --target aarch64
```

### How it is built

| Layer | Technology |
|---|---|
| Core | Rust workspace: `sms-engine` (classifier, DLT, OTP, finance, budget, translation), `sms-store` (bundled SQLite + FTS5, migrations, sync), `sms-crypto` (vault, backups), `sms-mms` (WAP/MMS decoder) |
| App shell | Tauri 2: one core shared by the Android WebView UI and the JNI receivers |
| Android | Kotlin plugin: default-SMS role, receivers, sending, MMS download, notifications, Keystore, widgets |
| UI | React 19, Tailwind CSS 4, Motion, TanStack Virtual |

The TypeScript engine in `src/engine` is the reference ("oracle"): `scripts/gen-fixtures.ts` generates fixtures from it, and the Rust port must reproduce them exactly.

## License

Snipt is **proprietary, source-available software**. Copyright © 2026 Tariq Anjum. All rights reserved.

You may read the code, and build and install it on your own devices for personal, non-commercial use. Redistribution, modified versions and commercial use need written permission. See [LICENSE](LICENSE) for the full terms. Third-party libraries and fonts remain under their own licenses.
