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
