//! R2 #7: the production webview can call only the app commands the UI
//! uses. Static checks over the ACL inputs (no window needed):
//! - `build.rs` declares an app manifest (so Tauri enforces app-command ACL),
//! - the granted set (`permissions/ui.toml`) only holds commands the UI calls,
//! - native-only operations are neither granted nor registered.

use std::collections::BTreeSet;
use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
}

fn quoted_after(src: &str, prefix: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = src;
    while let Some(i) = rest.find(prefix) {
        rest = &rest[i + prefix.len()..];
        let end = rest.find('"').unwrap();
        out.insert(rest[..end].replace('-', "_"));
        rest = &rest[end..];
    }
    out
}

/// `call('name'` / `call<T>('name'` in the UI's API adapter (app commands).
fn ui_calls() -> BTreeSet<String> {
    ui_calls_all()
        .into_iter()
        .filter(|n| !n.starts_with("plugin:"))
        .collect()
}

/// Every `call(...)` literal, `plugin:` ones included.
fn ui_calls_all() -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api");
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("ts") || p.ends_with("devFallback.ts") {
            continue;
        }
        let src = std::fs::read_to_string(&p).unwrap();
        let mut rest = src.as_str();
        while let Some(i) = rest.find("call") {
            rest = &rest[i + 4..];
            // Optional generic argument: `call<{ a: T; b: U }>(`.
            let r = match rest.strip_prefix('<') {
                Some(g) => g.find(">(").map_or("", |e| &g[e + 1..]),
                None => rest,
            };
            if let Some(after) = r.strip_prefix("('")
                && let Some(end) = after.find('\'')
            {
                used.insert(after[..end].to_string());
            }
        }
    }
    used
}

/// Commands listed in `generate_handler![...]` blocks of commands/mod.rs.
fn registered() -> BTreeSet<String> {
    let src = read("src/commands/mod.rs");
    let mut out = BTreeSet::new();
    let mut rest = src.as_str();
    while let Some(i) = rest.find("generate_handler![") {
        rest = &rest[i + "generate_handler![".len()..];
        let end = rest.find(']').unwrap();
        for name in rest[..end].split(',') {
            let n = name.trim();
            if !n.is_empty() {
                out.insert(n.to_string());
            }
        }
        rest = &rest[end..];
    }
    out
}

const NATIVE_ONLY: &[&str] = &[
    "set_send_status",
    "provider_refs",
    "provider_mark_read",
    "provider_delete",
    "provider_reconcile",
    "begin_send_attempt",
    "record_send_part",
    "end_send_handoff",
    "provider_missing",
];

#[test]
fn app_acl_manifest_is_declared() {
    let b = read("build.rs");
    assert!(b.contains("AppManifest::new().commands(COMMANDS)"));
    let declared = quoted_after(&b, "    \"");
    for c in registered() {
        assert!(
            declared.contains(&c),
            "{c} registered but not in the app ACL manifest"
        );
    }
}

#[test]
fn webview_grants_only_commands_the_ui_calls() {
    let granted = quoted_after(&read("permissions/ui.toml"), "\"allow-");
    let used = ui_calls();
    assert!(
        used.contains("threads_page") && used.contains("vault_reveal"),
        "parser"
    );
    for g in &granted {
        assert!(used.contains(g), "{g} granted but the UI never calls it");
    }
    for n in NATIVE_ONLY {
        assert!(
            !granted.contains(*n),
            "{n} must not be callable from the webview"
        );
        assert!(
            !registered().contains(*n),
            "{n} must not be a webview command"
        );
    }
    for cap in ["capabilities/default.json", "capabilities/mobile.json"] {
        let c = read(cap);
        assert!(c.contains("\"ui\""), "{cap} lacks the ui set");
        for n in NATIVE_ONLY {
            assert!(!c.contains(&n.replace('_', "-")), "{cap} grants {n}");
        }
    }
    // Dev-only commands exist only in debug / dev-seed handler lists.
    let src = read("src/commands/mod.rs");
    let release_list = src
        .split("#[cfg(not(any(debug_assertions, feature = \"dev-seed\")))]")
        .nth(1)
        .unwrap();
    assert!(!release_list.contains("ingest_incoming"));
    assert!(!release_list.contains("import_seed"));
}

/// UI calls the native webview is deliberately not granted: the browser
/// backup path (ciphertext through JS), taken only when capabilities report
/// `nativePicker: false`. The Tauri runtime always reports `true`
/// (`platform_capabilities`), and the UI keeps the backup controls pending
/// until capabilities load (R4 optional), so production never reaches them.
const BROWSER_ONLY: &[&str] = &["backup_create", "backup_restore"];

/// R4 optional (reverse direction): every command the production UI can
/// invoke has a grant, so no reachable control fails with an ACL error.
#[test]
fn every_command_the_ui_calls_is_granted() {
    let granted = quoted_after(&read("permissions/ui.toml"), "\"allow-");
    for u in ui_calls() {
        assert!(
            granted.contains(&u) || BROWSER_ONLY.contains(&u.as_str()),
            "the UI calls {u} but the webview is not granted it"
        );
    }
    assert!(
        read("src/core.rs").contains("native_picker: true,"),
        "BROWSER_ONLY relies on the native runtime always reporting a picker"
    );
    let ui = read("../src/App.tsx");
    assert!(
        ui.contains("nativePicker={platform === 'pending' ? null : caps.nativePicker}"),
        "backup controls must wait for capabilities"
    );
    // Plugin commands: exposed by the plugin manifest and in its default
    // permission set, which the Android capability grants.
    let plugin_cmds = quoted_after(
        &read("../plugins/tauri-plugin-telephony/build.rs"),
        "    \"",
    );
    let plugin_default = quoted_after(
        &read("../plugins/tauri-plugin-telephony/permissions/default.toml"),
        "\"allow-",
    );
    assert!(read("capabilities/mobile.json").contains("\"telephony:default\""));
    let mut n = 0;
    for c in ui_calls_all() {
        let Some(cmd) = c.strip_prefix("plugin:telephony|") else {
            continue;
        };
        n += 1;
        assert!(
            plugin_cmds.contains(cmd),
            "plugin command {cmd} not exposed"
        );
        assert!(
            plugin_default.contains(cmd),
            "plugin command {cmd} not granted"
        );
    }
    assert!(n >= 4, "plugin call parser");
}
