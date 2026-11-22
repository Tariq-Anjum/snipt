# Snipt Privacy Policy

**Effective date:** 22 November 2026
**App:** Snipt (package `app.snipt.sms`) for Android, and the Snipt desktop app for Linux
**Developer:** Tariq Anjum. Contact: open an issue at https://github.com/Tariq-Anjum/snipt/issues, or use the contact details on the Google Play listing.

## Summary

Snipt works entirely on your device. **We do not collect, receive, store, sell or share any of your data.** The Android app does not request the internet permission, so it cannot send your messages anywhere.

## What Snipt accesses on your device, and why

Snipt is an SMS app. When you choose it as your **default SMS app**, Android gives it the SMS permissions that every default SMS app needs:

| Permission | Why Snipt uses it |
|---|---|
| Read, receive and send SMS; receive MMS and WAP push | To show, receive, send and delete your text messages, as any SMS app does. |
| Read phone state | To tell your SIM cards apart on dual-SIM phones. |
| Post notifications | To notify you of new messages, with actions such as Reply, Mark read and Copy code. |

Snipt uses your messages only to provide its features **on your device**: sorting messages into categories, protecting one-time codes in an encrypted vault, reading bank and card alerts to build your spending ledger and monthly budget, offline translation, reminders and the weekly clean-up.

## Where your data is stored

- Messages and the information Snipt derives from them (categories, transactions, budget, settings) are stored in an **app-private database on your phone**. Other apps cannot read it.
- One-time codes are additionally encrypted (AES-256-GCM) with a key protected by the Android Keystore.
- Android's automatic cloud backup of app data is **disabled** for Snipt.
- Home-screen widgets show only what you allow in Settings and never show one-time codes.

## Backups

Snipt can create an **encrypted backup file** (`.mysms.enc`, protected by a passphrase you choose) **only when you ask it to**. You pick where it goes using Android's file picker (for example a folder on your phone, Google Drive, OneDrive or Dropbox). The file is encrypted before it leaves Snipt; the developer never receives it or the passphrase. Your use of a cloud storage provider is governed by that provider's privacy policy.

## What we do not do

- No accounts, sign-in or registration.
- No analytics, crash reporting, advertising SDKs or trackers.
- No data is sent to the developer or to any third party.
- Your data is not used for advertising, profiling or credit decisions.

## Data retention and deletion

Your data stays on your device until you delete it. You can delete individual messages or conversations in Snipt, use the weekly clean-up, or remove everything by uninstalling Snipt or clearing its storage in Android Settings. Messages also remain in Android's system SMS store, which is managed by your phone; switching back to another SMS app keeps them available there. Backup files you created are under your control and can be deleted wherever you saved them.

## Children

Snipt is not directed at children under 13.

## Security

Snipt uses Android's app sandbox, an app-private database, Android Keystore-protected encryption for one-time codes, screenshot blocking on sensitive screens, and automatic clearing of copied codes from the clipboard after 30 seconds. No method of storage is perfectly secure; keep your phone protected with a screen lock.

## Changes to this policy

If this policy changes, the new version will be published at this address with a new effective date. If a future version of Snipt ever offers features that send data off your device (for example optional advertising), this policy will be updated **before** that version is released, and the Google Play listing and Data safety section will say so.

## Contact

Questions or requests: https://github.com/Tariq-Anjum/snipt/issues
