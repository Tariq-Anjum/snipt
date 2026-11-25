# Next Update Plan (v1.1.3+) — Audit Findings & RCS Feasibility

Date: 2026-11-24  
Scope: Review of post-v1.1.2 edits, UI/copy accuracy fixes for the next release, evaluation of `android-rcs/rcsjta`, and practical alternatives for using RCS with Snipt.

---

## 1. Review of Current Updates (Post-v1.1.2)

### Verified Clean
- **Rust store deduplication (`crates/sms-store/src/platform.rs`, `mms.rs`)**: Extracting `lookup_provider_refs(&self, ids, sql)` cleanly removes duplicated query logic between `provider_refs` and `mms_refs` while keeping `prepare_cached` and `Option<Option<i64>>` null-skipping intact.
- **Tauri delete reservation cleanup (`src-tauri/src/commands/mod.rs`)**: Propagating `released?` after `out?` in `delete_messages` fixes silent dropping of reservation-release errors without masking the primary `delete_reserved` error.
- **Frontend UI & error handling (`src/App.tsx`, `ThreadPane.tsx`, `OtpCopyLabel.tsx`, `CloudBackupSection.tsx`, `RemindersView.tsx`, `NotificationBanner.tsx`)**:
  - `OtpCopyLabel` removes duplicated Copy/Copied JSX across three views.
  - `RemindersViewProps extends Omit<CloudBackupSectionProps, ...>` prevents prop drift.
  - Explicit `.catch` handlers on delete actions surface `"Could not confirm the message was deleted."` instead of failing silently.

### Verification Run
- `tsc --noEmit` (`npm run lint`): **Passed**
- `vite build`: **Passed**
- Frontend engine unit tests (`src/engine/engine.test.ts`): **11 / 11 Passed**
- Rust workspace tests (`cargo test --workspace`): **Run locally** (`cargo` is not installed in the web build sandbox).

---

## 2. Next Update Backlog (v1.1.3 Fixes)

| # | Area | Finding | Planned Fix |
|---|---|---|---|
| 1 | Thread header & Inspector (`src/components/ThreadPane.tsx`) | Threads with `isRcsCapable` display **"RCS Encrypted Chat"** (line 538) and **"RCS Universal Profile (SMS fallback)"** (line 621) even though `capabilities.rcs === false` and replies send over unencrypted SMS. | When `!caps.rcs`, label imported RCS threads **"Imported RCS · Replies via SMS"** and show **"SMS fallback (RCS unavailable to 3rd-party apps)"** in Engine Inspector. |
| 2 | Inbox filter (`src/components/InboxView.tsx`) | **"RCS Chats"** remains pinned in desktop sidebar and mobile filter bar even when `capabilities.rcs === false` and count is `0`. | Show the **RCS Chats** filter only when `caps.rcs === true` or `(counts?.filters.RCS ?? 0) > 0`. |
| 3 | Android role banner (`src/App.tsx`) | Default-SMS onboarding prompt (lines 952–954) still states *"MMS is not supported yet"*, which conflicts with implemented MMS receive/attachments (`docs/INSTALL-ANDROID.md`). | Update copy to: *"RCS chats stay in Google Messages and end when you switch; MMS receive is supported (MMS send is not supported yet)."* |

---

## 3. Evaluation of `https://github.com/android-rcs/rcsjta`

**Verdict: Does NOT work for modern Android RCS and should not be integrated.**

Why `rcsjta` cannot enable RCS in Snipt:
1. **Unmaintained ~10-year-old legacy stack**: `android-rcs/rcsjta` implements early GSMA RCS-e / joyn / Terminal API (TAPI) from the mid-2010s. It predates GSMA Universal Profile 2.4–2.7, Signal/MLS end-to-end encryption, and Android 10–16 background/telephony restrictions.
2. **Blocked at carrier & IMS provisioning**: Modern Indian carriers (Jio, Airtel, Vi) and Google Jibe require authenticated IMS Autoconfiguration Server (ACS) provisioning, carrier certificates, and Play Services / DroidGuard attestation. An independent SIP/MSRP client stack cannot self-register a consumer SIM on Jio/Airtel or Google Jibe.
3. **Android OS permission wall**: Modern Android exposes low-level IMS/RCS APIs (`ImsRcsManager`, `SipDelegate`) only to system-signed or carrier-privileged apps (`MODIFY_PHONE_STATE` / carrier privilege), never to normal Play Store or sideloaded third-party apps holding `ROLE_SMS`.

---

## 4. Alternative Ways to Use RCS with Snipt

Because Android blocks native third-party RCS clients, there are only four realistic architectural paths:

### Option A — Snipt Companion Mode (`NotificationListenerService` + `RemoteInput`) *(Best practical alternative if you need live RCS)*
- **How it works**: Keep **Google Messages** as the default SMS/RCS app so Jibe RCS stays active on your number. Snipt runs in an optional **Companion Mode**:
  - Captures incoming Google Messages RCS (and SMS) notifications via Android's `NotificationListenerService`.
  - Runs Snipt's full offline Rust intelligence on them (UPI/Credit Card finance tracking, Budget Planner, Encrypted OTP Vault, offline Hindi/Urdu/Hinglish translation, Reminders).
  - Sends RCS replies by triggering Google Messages' cached notification `RemoteInput` `PendingIntent` so Google Messages transmits the real E2E-encrypted RCS message.
- **Pros**: Zero servers, zero internet permission, real E2E RCS stays working on Jio/Airtel/Google Messages, works on unrooted Android 16.
- **Trade-offs**:
  - Can only reply to threads with an active Google Messages notification (starting a brand-new RCS chat must hand off to Google Messages).
  - Cannot delete messages from Google Messages' private RCS database (`bugle_db`).
  - `READ_SMS` alongside non-default mode is restricted on Play Store (ideal for personal sideloaded APK; Play Store release would stay notification-only in Companion Mode).

### Option B — Google Messages Web Pairing Bridge (e.g., `mautrix-gmessages` / `libgm` protocol)
- **How it works**: Keep Google Messages as Default SMS/RCS on Android and pair Snipt (especially useful for the Linux desktop app) using Google Messages' Web pairing protocol (QR / Google account).
- **Pros**: Full bidirectional RCS send/receive, full conversation history, media, and read receipts—including starting new RCS chats.
- **Trade-offs**:
  - Requires internet access and a Google/Web pairing session (breaks Snipt's strict zero-internet / zero-account privacy guarantee).
  - Uses an undocumented Google protocol that breaks periodically and risks account/session friction.

### Option C — Default SMS App Mode (Current Snipt Architecture: SMS + MMS Receive)
- **How it works**: Snipt holds `ROLE_SMS` directly with full control over `content://sms` (send, receive, Dual-SIM, delete, weekly cleanup) and MMS receive.
- **Pros**: Zero internet permission, Play-Store-compliant, reliable offline Rust store, full delete/cleanup capability.
- **Trade-offs**: No native RCS until Google/AOSP opens a public third-party `ROLE_SMS` RCS API.

### Option D — Root / Custom-ROM Private Database Hooks (`bugle_db` / System-Signed `SipDelegate`)
- **Not recommended**: Requires root or custom system signing, breaks Play Integrity / banking apps, and cannot ship on Google Play.

---

## 5. Recommended Roadmap

1. **v1.1.3 (Immediate)**: Ship the three UI/copy accuracy fixes in Section 2 (`ThreadPane.tsx`, `InboxView.tsx`, `App.tsx`) and run `cargo test --workspace` locally before tagging.
2. **v1.2.0 (Optional RCS Companion Mode)**: If keeping Google Messages RCS active is essential for daily use, prototype **Option A (`NotificationListenerService` + `RemoteInput`)** behind an opt-in **"Companion Mode (Keep Google Messages RCS)"** toggle in Settings, keeping Default-SMS mode as the primary full-control mode.
