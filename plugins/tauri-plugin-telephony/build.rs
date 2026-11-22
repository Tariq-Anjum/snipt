// Commands the web UI may invoke as `plugin:telephony|<name>`. Commands the
// app crate calls internally (send, provider sync, back handler, power state)
// are Rust methods on `Telephony` and are not exposed to the webview.
const COMMANDS: &[&str] = &[
    "request_default_sms_role",
    "status",
    "copy_sensitive",
    "set_secure",
    "display_boost",
    "import_inbox",
    "take_compose",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
