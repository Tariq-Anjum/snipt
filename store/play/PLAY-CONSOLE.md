# Snipt — Google Play Console answers (copy-paste)

Package: `app.snipt.sms` · Version 1.1.2 (versionCode 4) · AAB: `dist/Snipt-v1.1.2.aab`
Privacy policy URL: `https://github.com/Tariq-Anjum/snipt/blob/main/PRIVACY.md`

---

## 0. Create the developer account (only you can do this)

1. Go to https://play.google.com/console/signup and sign in with the Google account that should own the app (consider a dedicated one).
2. Account type: **Yourself (personal)**. *Organization* needs a D-U-N-S number but skips the 12-tester closed test (§7).
3. Pay the one-time **US$25** registration fee.
4. Complete **identity verification**: legal name and address exactly as on your government ID, then upload the ID when asked. Verify the contact email and phone number.
5. Choose the **developer name** shown on Play (for example "Tariq Anjum" or a brand). If you later sell the app or in-app items, Play displays your address publicly; an organisation account or a business address avoids showing your home address.
6. In the Play Console: **Create app** → name "Snipt: SMS & Money Organizer", default language English (India) or English (US), **App**, **Free**, accept the declarations.
7. Work through the Dashboard's "Set up your app" tasks using §1–§6 below, then §7.

---

## 1. Store listing (Grow → Store presence → Main store listing)

**App name (max 30):**
`Snipt: SMS & Money Organizer`
(Plain "Snipt" is already used on Play by an unrelated hair-cutting app; a descriptive suffix avoids confusion. See §8.)

**Short description (max 80):**
`Smart SMS inbox with OTP vault, UPI & card tracker and a monthly budget.`

**Full description (max 4000):**

```
Snipt snips the noise out of your SMS.

Most messages on an Indian phone are OTPs, promotions, bank alerts and delivery updates. Snipt sorts them for you, keeps one-time codes safe, turns bank and card alerts into a clear spending ledger and a monthly budget, and clears out the junk every week. Everything happens on your phone: Snipt has no internet permission.

SMART INBOX
• Automatic categories: Personal, Priority, Transactions, Promotions and Spam, plus Starred and Archived
• Understands Indian DLT sender IDs (service, transactional, promotional, government)
• Dual-SIM filter and fast search across 10,000+ messages
• Offline translation of Hindi, Urdu, Hinglish, Marathi, Tamil, Telugu and Bengali SMS to English

OTP VAULT
• One-time codes are encrypted and stay hidden until you tap Reveal
• Copied codes are cleared from the clipboard after 30 seconds
• Screenshots are blocked on the vault screen
• Notification actions: Copy code, Reply, Mark read, Delete

MONEY TRACKER (UPI, CREDIT CARDS, BANK ACCOUNTS)
• Reads bank, card and UPI alerts and builds a ledger with merchant, account and balance
• Smart about the tricky ones: card limits, bill reminders, payment receipts and transfers between your own accounts are not counted as spending
• Duplicate alerts count once, even when both your bank and the shop send an SMS
• Separate billing cycles for UPI, credit cards and savings accounts

MONTHLY BUDGET
• Spending sorted into Food & Dining, Transport & Travel, Bills & Utilities, Shopping & Retail and General
• Set your limits, pick the day your budget month starts, or start fresh any time

HOME-SCREEN WIDGETS
• Messages widget: unread count and latest conversations (codes never shown)
• Budget widget: this month's spending against your limit

CLEAN-UP AND BACKUP
• Weekly clean-up of expired OTPs, old promotions and spam, never touching personal messages or transactions
• Encrypted backups to any folder or cloud app you choose (Drive, OneDrive, Dropbox…)

BEAUTIFUL AND FAST
• Liquid Glass or Crisp Solid look, Light / Dark / System theme, Pure black for OLED screens
• Pinch to resize message text
• Smooth on 90 Hz and 120 Hz displays

PRIVATE BY DESIGN
• No internet permission, no accounts, no ads, no analytics
• Your messages never leave your phone unless you export an encrypted backup yourself

Note: Snipt must be your default SMS app to receive and send messages. RCS chats are not available to third-party SMS apps.
```

**Category:** Communication · **Tags:** SMS, Messaging, Finance tracking (pick the closest offered)
**Contact email:** a support address you are happy to publish (Play requires one; consider a dedicated address)
**Website:** `https://github.com/Tariq-Anjum/snipt`

**Graphics (files in `store/play/`):**
- App icon 512×512: `icon-512.png`
- Feature graphic 1024×500: `feature-graphic-1024x500.png`
- Phone screenshots (1080×2158, demo data only): `screenshots/01-inbox.png` … `06-backup-cleanup.png`

---

## 2. Sensitive permissions — SMS (App content → Sensitive app permissions → SMS and Call Log)

**Core functionality:** select **Default SMS handler** (the app is used to read, send and manage SMS as the user's default messaging app).

**Description of use (paste):**
```
Snipt is a full replacement SMS app. It asks the user to make it the default SMS app (RoleManager.ROLE_SMS) before any SMS feature works, and requests the SMS permissions only as the default SMS handler. As the default handler it receives SMS/MMS (SMS_DELIVER, WAP_PUSH_DELIVER), writes them to the system SMS provider, shows conversations, sends replies, marks messages read and deletes them, as required of a default SMS app. All processing happens on the device; the app has no INTERNET permission and transmits no SMS data. Additional on-device features (categorisation, OTP vault, expense ledger from bank alerts, budget, offline translation) operate on messages the default SMS app already holds.
```

**Demo video:** Play may ask for a short video showing the default-SMS prompt and the app working as the messaging app. Record on the phone: open Snipt → "Set as default" → system dialog → inbox → open a conversation → send a reply. Upload to YouTube as *Unlisted* and paste the link.

**Permissions used (all only while default SMS app):** READ_SMS, RECEIVE_SMS, SEND_SMS, RECEIVE_MMS, RECEIVE_WAP_PUSH. Also READ_PHONE_STATE (dual-SIM), POST_NOTIFICATIONS.

---

## 3. Data safety (App content → Data safety)

- **Does your app collect or share any of the required user data types?** → **No.**
  Rationale (Play's definition): data processed only on the device and not transmitted off it is not "collected". Snipt has no INTERNET permission. Encrypted backups are written by the user to a location they pick through the system file picker; the developer never receives them.
- **Is all of the user data collected by your app encrypted in transit?** → not applicable (nothing collected).
- **Do you provide a way for users to request that their data is deleted?** → not applicable / "No data collected". (Users delete data in the app or by uninstalling.)
- Security practices you may state in the description: data stays on device; OTP codes encrypted with Android Keystore; encrypted backups.

---

## 4. Content rating (App content → Content rating)

Category: **Utility, productivity, communication or other**. Answer **No** to violence, sexuality, language, controlled substances, gambling. For "Does the app allow users to interact or exchange content?": **Yes — users can send and receive SMS/MMS with other people** (this is a messaging app). Location sharing: No. Digital purchases: No. Expected rating: Everyone / 3+ with "Users interact" notice (IARC decides).

---

## 5. Other App content declarations

| Form | Answer |
|---|---|
| Target audience and content | 18+ (or 13+); not designed for children |
| Ads | **No, my app does not contain ads** (change before shipping any ad-supported version) |
| App access | All functionality is available without login (no account). Note that SMS features need the default-SMS role. |
| Government app | No |
| Financial features | Choose the option that the app provides **personal finance management / budgeting from the user's own SMS**, with **no** loans, payments, banking, crypto or trading |
| Health | No |
| News | No |
| Data deletion | No account → not applicable |
| Foreground service / exact alarms / full-screen intent | Not used |

---

## 6. Release setup

- **App signing:** use **Play App Signing** (default). Upload `dist/Snipt-v1.1.2.aab`, which is signed with your existing key in `~/.config/mysms-signing/`; that key becomes your **upload key**. Keep it backed up; if lost, request an upload-key reset from Play support (Play keeps the app-signing key).
- **Target API:** the AAB targets API 37, above Play's current minimum.
- **ABI:** arm64-v8a only. Covers nearly all current Android phones; 32-bit-only phones will not see the app.
- **Countries:** start with India; widen later.
- **Price:** Free. Changing a free app to paid later is **not allowed** on Play (it would need a new package). If you plan paid features, keep the app free and add in-app purchases or a subscription later (Play Billing).

---

## 7. Testing track required for new personal accounts

New **personal** developer accounts must run a **closed test with at least 12 testers opted in for 14 consecutive days** before they can apply for production. Organisation accounts are exempt (need a D-U-N-S number).
1. Testing → Closed testing → Create track → upload the AAB.
2. Add testers by email list or Google Group (12+ people with Android phones who will keep the app installed for 14 days).
3. After 14 days: Dashboard → *Apply for production* (answer the questionnaire about the test), then create the production release.

---

## 8. Name check (2026-11-22, web search only — not legal advice)

- **Google Play:** "Snipt: Learn How To Cut Hair" (Australian hair-education app, 10K+ downloads) and "SNIPT Provider" (beauty bookings, inactive since 2020) already use the name in other categories. Play does not reserve names, but identical names cause confusion and can trigger impersonation complaints. A descriptive title ("Snipt: SMS & Money Organizer") reduces that.
- **Trademarks:** no "SNIPT" record surfaced in quick US searches (similar: SNIPBOX, SNIPPOD); IP India, IP Australia and WIPO were **not** searched directly. Before paying for marketing or a paid launch, search IP India (classes 9, 38, 42) and WIPO Global Brand Database, or ask a trademark agent.
- **Domains/social:** snipt.com.au belongs to the hair app.
