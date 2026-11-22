# Plugin class is instantiated by name from Rust (register_android_plugin).
-keep class app.snipt.telephony.TelephonyPlugin { *; }
# JNI: native method names must survive R8 (src-tauri/src/jni_bridge.rs).
-keep class app.snipt.telephony.NativeCore { native <methods>; *; }
# Manifest components are kept by AAPT rules; @Command/@ActivityCallback methods
# are found by reflection.
-keepclassmembers class app.snipt.telephony.** {
    @app.tauri.annotation.Command *;
    @app.tauri.annotation.ActivityCallback *;
}
-keep @app.tauri.annotation.InvokeArg class app.snipt.telephony.** { *; }
