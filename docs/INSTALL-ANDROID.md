# Installing Snipt on Android (arm64)

Artifact: `dist/Snipt-arm64.apk` (app.snipt.sms, versionName 1.0.0, versionCode 1, minSdk 29, arm64-v8a only, v2-signed with the owner's release key). Check the checksum against `dist/SHA256SUMS` first: `cd dist && sha256sum -c --ignore-missing SHA256SUMS`.

## Install
Option A (USB, adb): enable Developer options and USB debugging on the phone, then
`adb install -r dist/Snipt-arm64.apk`

Option B (copy): copy the APK to the phone (USB, Syncthing, etc.), open it in the Files app, and allow "Install unknown apps" for that file manager when Android asks (Settings > Apps > Special access > Install unknown apps). Turn it off again afterwards.

Updates must be signed with the same key (`~/.config/mysms-signing/`). Back that directory up: if it is lost, the app cannot be updated in place (uninstall loses local data).

## Make Snipt the default SMS app
1. Open Snipt. A banner offers to set it as the default SMS app; tap it and choose Snipt (or Settings > Apps > Default apps > SMS app > Snipt).
2. Granting the role grants SMS, notification and phone-state permissions. Allow notifications if asked.
3. First run imports your existing inbox from the system SMS provider.

## Caveats
- RCS: leaving Google Messages as the default SMS app ends RCS chats for this number (Snipt does plain SMS and MMS receive only; MMS send is not supported). Switching back to Google Messages restores it.
- Snipt requests no INTERNET permission by design.
- Not verified on a real phone/carrier: dual-SIM, delivery reports, MMS, OEM battery limits. Keep a backup (Vault & Backup tab) before switching.
- To revert: set Google Messages (or another app) as default SMS app; messages stay in the system provider.
