# Phone round 1 (2026-11-22) — owner-requested fixes after real-device test

Device: vivo phone, Android 16 (SDK 36), arm64, 1260x2800 @ 120 Hz, density 432 (override).
Connected via wireless adb, later USB.

## Device facts (verified)
- Installed APK hash == dist/SHA256SUMS (371d6078…c4f3). Installed via LocalSend -> packageinstaller.
- Android 16 ECM blocked the SMS role ("restricted setting") until the owner allowed restricted settings; now Snipt holds ROLE_SMS, all perms granted.
- Play Protect showed `AdiWarnModeTrampolineActivity` (developer-verification warn mode) at 16:12; warning only.
- Native abort on exit 16:06:57: `FORTIFY: pthread_mutex_lock called on a destroyed mutex` (threads after onDestroy). No tombstone.
- Scroll: SurfaceFlinger presents the app layer at 8.33 ms steady (125 frames, 0 > 12 ms). gfxinfo p99 11 ms.

## Owner issues -> cause -> fix
| # | Issue | Cause (evidence) | Status |
|---|---|---|---|
| 1 | Light mode vs system dark mismatch | theme defaulted to 'light', ignored prefers-color-scheme | fixed: System/Light/Dark, default System (verified on phone) |
| 2 | Status bar disappears | light theme kept white status-bar icons on light bg | fixed: setSystemBars command follows theme (verified screenshot 24) |
| 3 | Dull blacks / low contrast | navy slate palette, translucent panels | fixed: Pure black (OLED) palette default on, brighter secondary text, opaque dock/sheet |
| 4 | Pinch zooms whole app | viewport allowed zoom; no text-scale gesture | fixed in code: page zoom off (viewport + WebView), pinch scales message text; NOT verified on device (adb cannot pinch) |
| 5 | No settings menu | toggles sat in the inbox toolbar | fixed: gear -> Settings sheet (verified) |
| 6 | Widgets (liquid glass) SMS + budget | not present | added: SmsWidgetProvider + BudgetWidgetProvider (registered per dumpsys appwidget); placement/visual NOT verified yet |
| 7 | Scroll not smooth at 120 Hz | frames on time before and after (8.35 ms, 0/125 >12 ms); likely virtualizer flushSync + thread re-measure corrections mid-fling | changed: thread list plain DOM + content-visibility, inbox useFlushSync:false; feel needs owner check |

## Found during review (screenshots 02-10)
- Finance: "USD 5.54 spent ... Avl Limit: INR 95,221.00" booked as ₹95,221 debit (budget 651 %).
- Finance: bill-due notice "Rs. 680 is due ... Ignore if paid" booked as debit.
- Finance: biller receipt "payment ... of Rs. 890 has been received" booked as a transaction.
- Merchant: "SMS BLOCK <card> to <bank number>" -> merchant = phone number instead of the UPI payee name.
- Credit-card-on-UPI debits tagged UPI (card cycle wrong).
- Composer shown for alphanumeric DLT senders (cannot reply).
- Bottom dock translucent; list text bleeds through.

## Progress log
- 2026-11-22: v1.1.0 (versionCode 2) built, signed with the same key, installed via USB; dist/Snipt-arm64-v1.1.0.apk (+SHA256SUMS line). v1.0.0 kept.
- Finance: due notices, future debits, biller/card-payment receipts, own-account transfers (INFT) are not transactions; balance/limit amounts skipped; foreign currency kept in its currency and left out of INR totals; credit card on UPI = Credit Card; UPI payee / "; NAME credited" merchants. TS oracle + Rust identical, fixtures regenerated (8+2 new inputs).
- Store migrations v5/v6: one-time re-analysis of stored messages (phone: debits ₹1,95,364 -> ₹23,836 active cycles).
- Duplicate alerts (same SMS twice, bank + merchant SMS) counted once (Rust store only; TS dev fallback does not dedupe).
- Budget: monthly reset day (1-28), "Start fresh now" (manual floor, undo), "Restore default limits". Nothing deleted.
- Exit abort (FORTIFY destroyed mutex) fixed: MainActivity ends the process on a finishing destroy; verified no abort on exit.
- Checks: cargo fmt/clippy -D warnings clean; cargo test 152/152; TS tests 14/14; tsc clean.
- Open: classification of some service SMS (a retailer loyalty-points notice -> Transactions, a lab-report notice -> Promotions) is the ML model; not changed.
