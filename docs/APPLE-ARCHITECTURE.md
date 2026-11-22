# Snipt for Apple (iOS · iPadOS · macOS) — Architecture, Liquid Glass Design & Build Plan

Date: 2026-11-23. Status: architecture & plan (ready for slice S1).
Labels: **[F]** verified in this repo/session · **[I]** inference · **[R]** recommendation · **[U]** must be verified on macOS / Xcode 26 SDK.

---

## 1. Executive Decision: One Native SwiftUI Multiplatform App + Existing Rust Core

**[R] Build ONE native SwiftUI Multiplatform app (`iOS`, `iPadOS`, `macOS`) backed by Snipt's existing Rust workspace (`sms-types`, `sms-engine`, `sms-store`, `sms-crypto`, `sms-mms`) via a thin C-ABI static library (`crates/sms-ffi` → `SniptCore.xcframework`).**

Why native SwiftUI + Rust FFI instead of Tauri 2 (WKWebView) on Apple:
1. **True Apple Liquid Glass**: Apple's Liquid Glass (`glassEffect`, `GlassEffectContainer`, optical refraction, lensing, specular edges, concentric corners, floating tab bar, automatic Reduce Transparency / Low Power Mode degradation) is a **native SwiftUI/UIKit/AppKit compositor material** — WKWebView CSS `backdrop-filter` cannot reproduce it and breaks visual parity with iOS 26 / iPadOS 26 / macOS 26 Tahoe.
2. **Mandatory Apple App Extensions are Swift-only**: `ILMessageFilterExtension` (SMS filtering inside Apple Messages), `WidgetKit` (Home Screen / Lock Screen / macOS Desktop), `AppIntents` (Shortcuts auto-ingest + Siri), and `ShareExtension` cannot run inside WKWebView.
3. **Zero duplicated business logic (Ponytail rule)**: `Core` in `src-tauri/src/core.rs` is already window-free [F]. Extracting it (or sharing it via `crates/sms-core` + a ~180-line C-ABI JSON bridge `crates/sms-ffi`, matching `jni_bridge.rs`) reuses 100% of Snipt's DLT parser, OTP detector/vault, finance/UPI ledger, budget planner, Indic offline translator, SQLite+FTS5 store, and `.mysms.enc` backup format — with **zero** `uniffi`/MPL-2.0 codegen overhead.
4. **One Apple codebase**: SwiftUI Multiplatform targets iOS, iPadOS, and macOS from a single target with adaptive chrome (`TabView` floating glass bar on iPhone; `NavigationSplitView` translucent sidebar on iPad & Mac).

---

## 2. Honest Platform Reality & Feature Parity Matrix

Unlike Android (`RoleManager.ROLE_SMS`), **iOS and iPadOS do not allow third-party apps to become the default SMS app or read Apple's Messages database** [F Apple platform policy]. Snipt on Apple is designed honestly around what Apple **does** allow, giving full practical parity without hacks or servers:

| Snipt Feature | Android (shipped) | Linux (shipped) | iOS / iPadOS (Apple App) | macOS (Apple App) |
|---|---|---|---|---|
| **Offline DLT / Bank / UPI / Spam Classifier** | Default SMS app (`SMS_DELIVER`) | Backup viewer | **`ILMessageFilterExtension`** classifies incoming unknown-sender SMS directly inside Apple Messages into Transactional (Finance / Orders / Reminders / Health / Carrier), Promotional (Offers / Coupons), and Junk! | Same Rust engine on imported `chat.db` & `.mysms.enc` |
| **Live SMS Ingest → Ledger / Budget / OTP Vault** | Automatic via `SMS_DELIVER` | Manual `.mysms.enc` import | **1. Shortcuts Personal Automation** ("When I get a Message" → `IngestSmsIntent` via `AppIntents`, zero-touch once enabled)<br>**2. Share Sheet / Paste** (`ShareExtension`)<br>**3. `.mysms.enc` import** (Files / iCloud Drive) | **1. Direct read-only `~/Library/Messages/chat.db` sync** (Developer ID build with user-granted Full Disk Access; captures iPhone SMS via Messages in iCloud / Text Message Forwarding!)<br>**2. `.mysms.enc` import** |
| **Sending / Deleting SMS** | Direct (`SmsManager`) | Not supported | Opens Apple Messages pre-filled (`sms:` URL / `MFMessageComposeViewController`); deletion stays in Apple Messages | Opens Messages.app (`sms:` URL) |
| **OTP Vault (AES-256-GCM)** | Android Keystore DEK + `FLAG_SECURE` + 30s clipboard wipe | Ephemeral session DEK | **Keychain (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`) + `LocalAuthentication` (Face ID / Touch ID)** + `UIPasteboard.setItems(..., options: [.localOnly: true, .expirationDate: +30s])` + screen-capture privacy overlay | **Keychain + Touch ID** + `NSPasteboard` concealed type + 30s timer wipe |
| **Finance Ledger & Monthly Budget** | Full | Full | **100% identical** (same `sms-engine` + `sms-store`) | **100% identical** |
| **Offline Indic Translation** | Full (hi/ur/mr/bn/ta/te) | Full | **100% identical** (same `sms-engine::translate`) | **100% identical** |
| **Encrypted `.mysms.enc` Backup & Cross-Device Sync** | SAF picker | Native file dialog | **Files / iCloud Drive** (`UIDocumentPicker` + registered `.mysms.enc` UTType `app.snipt.backup`); seamlessly opens Android/Linux backups! | **Finder / iCloud Drive** (`NSOpenPanel`/`NSSavePanel` + `.mysms.enc` double-click open) |
| **Widgets** | Android Glance/AppWidget | None | **WidgetKit** (Home Screen + Lock Screen + StandBy): Monthly Budget & Unread Summary (no OTP codes ever shown) | **WidgetKit** (macOS Desktop & Notification Center) |
| **Search & Ecosystem** | FTS5 in-app | FTS5 in-app | FTS5 + **CoreSpotlight** (opt-in, OTPs excluded) + **Handoff** + **Siri / AppIntents** ("How much did I spend on Food this month?") | FTS5 + **Spotlight** + **Handoff** + **Menu Bar Quick-View** (`MenuBarExtra`) + keyboard shortcuts (`⌘K`, `⌘1–4`, `⌘I`) |
| **Privacy** | Zero `INTERNET` permission | Zero network | **Zero network calls** (no `URLSession`, no telemetry, no analytics; App Store Privacy Nutrition Label: "Data Not Collected") | **Zero network calls** |

> **Important `ILMessageFilterExtension` Sandbox Note [U — verify on Xcode 26]:**
> Apple sandboxes `ILMessageFilterExtension` strictly (no network, tight memory limit ~12–24 MB, historically restricted from writing to shared App Group containers). Therefore:
> - `SniptMessageFilter` links **only `sms-engine`** (stateless classifier, < 1 MB RAM, zero SQLite) to classify incoming SMS into Apple Messages' native Transactional/Promotional/Junk sub-actions in < 1 ms.
> - Persistent ingestion into `sms-store` SQLite happens via the **AppIntents Shortcuts Automation** (`IngestSmsIntent`), **Share Extension**, **`.mysms.enc` backup restore**, and **macOS `chat.db` reader**.

---

## 3. Repository & Module Layout

Keep everything inside `/mnt/MD/AI-tools/snipt/` so Android, Linux, and Apple share one Rust workspace and one CI pipeline:

```text
snipt/
  Cargo.toml                       # add crates/sms-core, crates/sms-ffi to workspace members
  crates/
    sms-types/                     # existing (unchanged)
    sms-engine/                    # existing (unchanged)
    sms-crypto/                    # existing (unchanged)
    sms-store/                     # existing (unchanged)
    sms-mms/                       # existing (unchanged)
    sms-core/                      # NEW: window-free Core extracted from src-tauri/src/core.rs
                                   #      (reused by BOTH src-tauri AND sms-ffi — zero duplication!)
    sms-ffi/                       # NEW: ~180-line C-ABI staticlib (`crate-type = ["staticlib"]`)
                                   #      exports snipt_init, snipt_dispatch(json)->json,
                                   #      snipt_classify_sms(sender,body)->json, snipt_free_string
                                   #      + include/snipt_ffi.h + module.modulemap
  apple/
    project.yml                    # XcodeGen spec (single source of truth; avoids merge-hell .xcodeproj)
    Scripts/
      build-xcframework.sh         # builds Rust staticlibs for ios/ios-sim/macos -> SniptCore.xcframework
    Snipt/                         # SwiftUI Multiplatform App (iOS + iPadOS + macOS)
      App/                         # SniptApp.swift, AppState.swift, SniptCoreBridge.swift
      Design/                      # LiquidGlass.swift, Theme.swift (Liquid Glass / Crisp Solid / OLED Black)
      Features/
        Inbox/                     # InboxView.swift, ThreadDetailView.swift, CategoryRail.swift
        Finance/                   # FinanceView.swift, BudgetView.swift, MerchantLedgerView.swift
        Vault/                     # OtpVaultView.swift (Keychain + FaceID/TouchID + 30s pasteboard)
        Reminders/                 # RemindersView.swift
        Settings/                  # SettingsView.swift, BackupSheet.swift, ShortcutsSetupGuide.swift
        MacOS/                     # ChatDbImporter.swift (read-only ~/Library/Messages/chat.db), MenuBarView.swift
      Intents/                     # IngestSmsIntent.swift, BudgetSummaryIntent.swift (AppIntents)
    Extensions/
      SniptMessageFilter/          # ILMessageFilterExtension (links stateless sms_ffi classify only)
      SniptWidgets/                # WidgetKit (Budget + Unread, reads shared App Group snapshot JSON)
      SniptShareExtension/         # Share text or .mysms.enc directly into Snipt
  .github/workflows/
    apple-ci.yml                   # macos-latest runner: cargo test + build-xcframework + xcodebuild test
```

### 3.1 C-ABI Bridge (`crates/sms-ffi`) — Ponytail Design
No `uniffi`, no C++ bridging, no generated Swift boilerplate. Four `extern "C"` functions mirroring `jni_bridge.rs`:
1. `snipt_init(db_path: *const c_char, tz_offset_min: i32) -> *mut c_char` — opens WAL SQLite store, sets file permissions `0600`, runs migrations.
2. `snipt_set_vault_key(key_ptr: *const u8, key_len: usize) -> i32` — installs 32-byte DEK unwrapped from Apple Keychain into `Zeroizing` memory.
3. `snipt_dispatch(req_json: *const c_char) -> *mut c_char` — tagged JSON command dispatch (`threads_page`, `messages_page`, `search`, `finance_summary`, `budget_stats`, `otp_items`, `vault_reveal`, `ingest_incoming`, `translate`, `weekly_scan`, `apply_cleanup`, `backup_create`, `backup_restore`, etc.) returning `{"ok": ...}` or `{"err": "..."}`. Runs on a Swift `actor` / background `Task.detached` so the main UI thread never blocks.
4. `snipt_classify_sms(sender: *const c_char, body: *const c_char, ts_ms: i64) -> *mut c_char` — stateless `sms_engine` classification for `ILMessageFilterExtension` (zero DB open, < 1 ms, tiny memory footprint).
5. `snipt_free_string(ptr: *mut c_char)` — frees C strings allocated by Rust.

---

## 4. Making Liquid Glass Shine on Apple (iOS 26 · iPadOS 26 · macOS 26)

Snipt's visual identity is **calm, high-craft Liquid Glass with instant legibility for Indian SMS & finance data**. On Apple, we elevate it from CSS blur to **true hardware-accelerated Apple Liquid Glass** while strictly preserving Snipt's **Crisp Solid** and **Pure Black OLED** modes:

### 4.1 Surface Hierarchy (Glass only on Chrome & Controls — Never on Dense Lists)
1. **Navigation & Floating Chrome (`glassEffect`)**:
   - **iPhone**: Native floating Liquid Glass bottom tab bar (`Inbox`, `Finance`, `Vault`, `Reminders`) with automatic scroll-edge soft refraction and morphing search pill (`Tab(role: .search)`).
   - **iPad & Mac**: `NavigationSplitView` with Liquid Glass translucent sidebar (categories: Personal, Priority, Transactions, Promotions, Spam, Starred, Archived) + unified toolbar glass buttons.
2. **Interactive Glass Morphing (`GlassEffectContainer`)**:
   - **Category Filter Pills & Dual-SIM / Account Switcher**: Grouped inside a `GlassEffectContainer(spacing: 12)` with `glassEffectID` so switching between *Personal / Priority / Transactions* or *Active Cycle / All History* fluidly morphs the glass capsule across pills at 120 Hz ProMotion.
   - **Inline OTP Action Capsule**: Inside conversation threads and the OTP Vault, masked codes sit in a tinted interactive `.glassEffect(.regular.tint(.blue).interactive(), in: .capsule)`. Tapping authenticates (Face ID / Touch ID if locked), reveals the code with a smooth numeric transition, and morphs into a **"Copied · wipes in 30s"** glass countdown pill.
3. **Finance & Budget Hero Surfaces**:
   - Top monthly budget ring and active-cycle spend card use subtle concentric corner radii (`ConcentricRectangle` / `.containerRelativeFrame`) with glass-accented action chips ("Start fresh", "Reset day", "Export `.mysms.enc`").
   - **Scrollable list rows** (10,000+ SMS threads, ledger rows) remain **clean, crisp, non-blurred surfaces** so scrolling stays locked at **120 Hz ProMotion (≤ 8.3 ms/frame)** with zero overdraw!
4. **Adaptive & Accessibility Fallbacks (Zero-Compromise)**:
   - `@Environment(\.accessibilityReduceTransparency)` → automatically switches to **Crisp Solid** tokens.
   - `ProcessInfo.processInfo.isLowPowerModeEnabled` → automatically degrades to **Crisp Solid** (matching Android's `powerState` rule).
   - User toggle in Settings: **Liquid Glass | Crisp Solid**, **System | Light | Dark**, and **Pure Black OLED** (`#000000` canvas with subtle hairline glass chrome).
   - **OS Compatibility**: Primary target **iOS 26 / iPadOS 26 / macOS 26** (`#available(iOS 26, macOS 26, *)` for `.glassEffect` / `GlassEffectContainer`), with an optional thin `SniptSurface` modifier falling back to `.ultraThinMaterial` + 1 pt specular stroke if owner chooses iOS 18 / macOS 15 support.

### 4.2 App Icon (Apple Liquid Glass Icon Composer)
- Export Snipt's existing `assets/icon/mysms.svg` layers (background blue gradient + floating white glass chat-snip glyph) into **Xcode 26 Icon Composer** (`.icon` multi-layer asset) so the Snipt app icon automatically renders with real specular Liquid Glass depth in Default, Dark, Clear, and Tinted Home Screen modes!

---

## 5. Batisseur Routing Verification (Before & After v0.4.0)

`batisseur` (v0.3.0) previously routed `macos-desktop` (`ok`, score 79) but abstained (`needs_information` / `supported_pack`) on `ios/phone` and `ios/tablet` [F].
Extended `/mnt/MD/AI-tools/batisseur` to **v0.4.0** (local, BuildBench 16/16 & `tt-1.1.0` SHA-256 unchanged [F]) with `ios-phone.toml`, `ipados-tablet.toml`, and `swiftui-rust-ffi` candidate on all three Apple packs:

| Target Intent | Selected Pack | Capability | Score | Required Verifiers |
|---|---|---|---|---|
| iPhone (`ios/phone/store`) | `ios-phone` | `design:ios-phone` | 79 | `touch-targets`, `safe-area-insets`, `dynamic-type`, `reduced-transparency`, `one-handed-reach` |
| iPad (`ios/tablet/store`) | `ipados-tablet` | `design:ipados-tablet` | 79 | `touch-targets`, `split-view-multitasking`, `window-resize`, `keyboard-shortcuts`, `reduced-transparency` |
| Mac (`macos/desktop/local`) | `macos-desktop` | `design:macos-desktop` | 79 | `keyboard-only`, `shortcuts`, `window-resize` |

---

## 6. Thin-Slice Build Plan (Linux-First Core → macOS/Xcode Native UI)

Since this workstation runs Linux (CachyOS x86_64) [F], slices **S1–S2** can be implemented, compiled, and unit-tested **right here on Linux**, while **S3–S8** compile and run on macOS (local Mac or GitHub Actions `macos-latest` with Xcode 26):

| Slice | Deliverable | Where Built & Verified | Done Check |
|---|---|---|---|
| **S1** | Extract `crates/sms-core` from `src-tauri/src/core.rs` (zero behavior change on Android/Linux) | Linux (here) | `cargo test --workspace --release --locked` green |
| **S2** | Add `crates/sms-ffi` (C-ABI staticlib + `snipt_ffi.h` + `module.modulemap` + C-ABI roundtrip tests) | Linux (here) | `cargo test -p sms-ffi` tests init, classify, ingest, finance, OTP vault, `.mysms.enc` roundtrip |
| **S3** | Add `apple/project.yml` (XcodeGen) + `build-xcframework.sh` + `.github/workflows/apple-ci.yml` + minimal `SniptCoreBridge.swift` | Linux + GitHub Actions `macos-latest` | CI compiles `SniptCore.xcframework` + empty SwiftUI Multiplatform shell for iOS Sim & macOS |
| **S4** | Liquid Glass Design System (`LiquidGlass.swift`, `Theme.swift`, OLED Black, Crisp Solid, Reduce Transparency / Low Power Mode) + Adaptive Shell (`TabView` on iPhone, `NavigationSplitView` on iPad/Mac) | macOS / Xcode 26 | SwiftUI Previews + UI test on iPhone, iPad, Mac |
| **S5** | Core Screens: Inbox (virtualized `LazyVStack` + keyset paging + FTS5 search), Thread View (inline OTP/Txn/Reminder cards + offline translation), Finance & Monthly Budget, Reminders | macOS / Xcode 26 | 10k-message seed scroll ≤ 8.3 ms/frame; matches Rust parity fixtures |
| **S6** | OTP Vault (Keychain DEK + Face ID/Touch ID + 30s `.localOnly` pasteboard) + `.mysms.enc` Backup/Restore (Files / iCloud Drive + UTType) | macOS / Xcode 26 | Cross-decrypt `.mysms.enc` created on Android/Linux and vice versa |
| **S7** | Apple Ecosystem Extensions: `SniptMessageFilter` (`ILMessageFilterExtension`), `AppIntents` (`IngestSmsIntent` + Shortcuts setup guide + Siri budget query), `SniptShareExtension`, `SniptWidgets` (`WidgetKit`) | macOS / Xcode 26 + real iPhone | Unknown-sender Indian DLT SMS sorted in Apple Messages; Shortcuts automation ingests SMS into ledger; Widget updates |
| **S8** | macOS Power Features: read-only `~/Library/Messages/chat.db` importer (Full Disk Access flow, Developer ID notarized DMG), `MenuBarExtra` quick budget/unread popover, `CoreSpotlight`, `Handoff`, keyboard shortcuts | macOS / Xcode 26 | Live `chat.db` import on Mac without duplicating messages; DMG signed & notarized |

---

## 7. Open Owner Decisions (Before S3 / App Store Submission)

1. **Minimum OS version**: **iOS 26 / iPadOS 26 / macOS 26 only** (pure native Liquid Glass, smallest code — Ponytail default) vs **iOS 18 / macOS 15 fallback** (adds ~60 lines of `.ultraThinMaterial` compatibility shim).
2. **macOS distribution channel**:
   - **Developer ID notarized DMG** (via GitHub Releases, like Linux/Android): allows **direct `~/Library/Messages/chat.db` reading** (with user-granted Full Disk Access) + `.mysms.enc` backups.
   - **Mac App Store (sandboxed)**: `.mysms.enc` backups only (Mac App Store sandbox blocks `~/Library/Messages/chat.db`).
   - *[R] Ship iOS/iPadOS on the App Store (TestFlight first) and macOS as a Developer ID notarized DMG (optionally also Mac App Store without `chat.db`).*
3. **Apple Developer Account & Bundle ID**: Confirm Apple Team ID and bundle identifier (`app.snipt.sms`, matching Android/Linux, plus App Group `group.app.snipt.sms`).
4. **Build host**: Do you have a Mac for Xcode 26 previews/signing, or should we bootstrap S1–S3 here on Linux and use GitHub Actions `macos-latest` runners for Apple builds?
