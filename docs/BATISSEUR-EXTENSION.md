# batisseur extension: `phone` form factor + `android-phone` pack + Tauri/webview stack entry

Target repo: `/mnt/MD/AI-tools/batisseur` (v0.2.0, clean tree at d8a38a2).
Labels: **[F]** verified this session · **[I]** inference · **[R]** recommendation · **[U]** verify when implemented.

## 1. Problem (reproduced)
[F] Phone intent (android/android/`phone`/store, direct_touch/touch, gui/local/foreground):
```text
$ cargo run -q --offline -- route /tmp/.../phone.json --json
batisseur: target.form_factor: unknown enum value          # exit 2, 0.024 s
```
[F] `schemas/build-intent.schema.json` `target.form_factor` enum = tv, tablet, desktop, kiosk, browser, terminal, server, library, unknown, null. Packs: 9; Android packs only `android-tablet` (tablet) and `android-tv` (tv).
[F] Linux intent `examples/linux-desktop-gui.json` routes `ok → implement:linux-desktop-gui` (stack `rust-slint`, whose own fit_notes flag GPLv3 licensing).

## 2. Constraints respected
- tt-1.1.0 untouched: `contracts/tt-1.toml` sha256 `301e80ce…f6c9` equals `contracts/provenance.json` [F]. Form factor is a build axis in the intent schema, not a taxonomy value, so no contract change is needed [I: SPEC's "JSON Schema enums are projections of the same snapshot" sentence is about `task_type`/artifact IDs; `target.*` enums do not appear in tt-1.toml — F, grep].
- No router logic change: eligibility and scoring are data-driven from pack TOML (`rejection()`/`score()` in `src/lib.rs`) [F]. Only the embedded pack list grows.
- BuildBench v0 stays **frozen and unchanged** (16 cases, same precedent as the 0.2.0 linux-desktop-gui addition, CHANGELOG [F]). New behaviour is covered by new unit/integration tests, not by editing frozen oracles.
- No new dependency; no network; deterministic output.

## 3. Design
### 3.1 Schema
`target.form_factor` enum: add `"phone"` (after `"tablet"`). Nothing else.

### 3.2 New pack `packs/android-phone.toml`
```toml
id = "android-phone"
version = "0.1.0"
taxonomy_version = "tt-1.1.0"
platforms = ["android"]
operating_systems = ["android"]
form_factors = ["phone"]
deployment_envs = ["store", "local"]
application_kinds = ["gui"]
modes = ["foreground"]
runtimes = ["local", "networked", "hybrid"]
interaction_profiles = ["direct_touch"]
primary_inputs = ["touch"]
offline_supported = true
native_io_supported = true
stack = "compose-adaptive"
required_verifiers = ["touch-targets", "edge-to-edge-insets", "predictive-back", "one-handed-reach"]
conflicts = []
official_references = [
  "https://developer.android.com/develop/ui/compose/layouts/insets",
  "https://developer.android.com/about/versions/16/behavior-changes-16",
  "https://v2.tauri.app/develop/plugins/develop-mobile/",
]
dependency_cost = 2
maintenance_cost = 2
context_cost = 1
runtime_cost = 1
licensing_penalty = 0
security_penalty = 0
unsupported_penalty = 0

[[framework_candidates]]
id = "compose-adaptive"
name = "Kotlin with adaptive Android UI"
fit_notes = "Native phone UI; validate insets, predictive back and touch targets."

[[framework_candidates]]
id = "tauri-webview"
name = "Tauri 2 (Rust core + system WebView UI, Kotlin plugins)"
fit_notes = "One web UI shared with Linux desktop; native APIs via Kotlin plugins; measure WebView jank on target devices."
```
Rationale: verifiers reflect Android 16 phone obligations — edge-to-edge cannot be opted out and predictive back is default at targetSdk ≥ 36 [F Android 16 behaviour page]; costs mirror `android-tablet` so no pack wins by tuning. `android-tablet` is not widened to phones because its verifiers (rotation, window-resize) are tablet contracts [R].

### 3.3 Stack entry for the Tauri/webview shell
- [R] Represent it as a **framework candidate** `tauri-webview` in `android-phone` (above) and in `linux-desktop-gui` (below). The pack `stack` field stays the pack default: an exact `constraints.existing_stack = "tauri-webview"` then simply scores +0 instead of +20, which cannot change any selection because each of these intents has a single eligible pack. No scoring code change.
- `packs/linux-desktop-gui.toml`: append the candidate and bump pack `version` to `0.1.1`:
```toml
[[framework_candidates]]
id = "tauri-webview"
name = "Tauri 2 (Rust core + WebKitGTK UI)"
fit_notes = "MIT/Apache shell; reuses a web UI shared with Android; depends on system webkit2gtk-4.1."
```
- Not done (out of minimal scope): a multi-stack `stacks = [...]` pack field — it would change the Pack struct (`deny_unknown_fields`), every pack file and the SPEC scoring rule.

## 4. Exact files to change in `/mnt/MD/AI-tools/batisseur`
| File | Change |
|---|---|
| `schemas/build-intent.schema.json` | add `"phone"` to `target.form_factor.enum` |
| `packs/android-phone.toml` | **new** (§3.2) |
| `packs/linux-desktop-gui.toml` | add `tauri-webview` candidate; version 0.1.0 → 0.1.1 |
| `src/lib.rs` | add `include_str!("../packs/android-phone.toml"),` to the `packs()` array (one line) |
| `tests/cli.rs` | `packs list` line count `9` → `10` |
| `tests/routing.rs` | new tests: (a) phone intent → `ok`, `implement:android-phone`, verifiers equal the pack list, 14 terms summing to score; (b) bench `android-tablet` fixture still selects `android-tablet`; (c) phone + `primary_input:"pointer"` with direct_touch → `needs_information` (existing collision rule); (d) `android-alone` fixture still abstains |
| `examples/android-phone.json` | **new** complete phone intent (validates) |
| `docs/SPEC.md` | "Nine versioned TOML packs" → "Ten"; one sentence: framework candidate `tauri-webview` is advisory, not a stack match |
| `README.md` | "Nine platform/interaction packs" → "Ten"; add `examples/android-phone.json` usage line |
| `CHANGELOG.md` | `## 0.3.0` entry: phone form factor, android-phone pack, tauri-webview candidates; frozen BuildBench v0 unchanged |
| `Cargo.toml` (+ `Cargo.lock` package version line) | version 0.2.0 → 0.3.0 (local commit only; no tag, no push, no release — owner gate) |

Untouched: `contracts/*` (tt-1.toml, provenance, families, interaction-profiles, workflows), `bench/buildbench-v0.jsonl`, `schemas/route-result.schema.json` (`selected_pack` is free `string|null` [F]), `src/main.rs`, `skill/batisseur/SKILL.md`, `.github/*`, `install.sh`.

## 5. Verification (RB checkpoint)
```bash
cd /mnt/MD/AI-tools/batisseur
sha256sum contracts/tt-1.toml            # must equal provenance.json sha256
git diff --stat bench/ contracts/        # must be empty
cargo fmt --check && cargo clippy --offline --all-targets -- -D warnings && cargo test --offline
cargo run -q --offline -- bench          # 16/16, passed=true (unchanged)
cargo run -q --offline -- packs list | wc -l                     # 10
cargo run -q --offline -- validate examples/android-phone.json
cargo run -q --offline -- route examples/android-phone.json --json | jq -r '.route_status,.selected_pack,.capabilities[0]'
#   expect: ok / android-phone / implement:android-phone   [I until run]
for i in 1 2 3; do cargo run -q --offline -- route examples/android-phone.json --json | sha256sum; done   # identical
```

## 6. mySMS intents to log in BATISSEUR-LOG.md (before and after)
- Android: `implement`, `code`, `new_product`, target android/android/phone/local, direct_touch/touch, gui/local/foreground, `constraints: {offline: true, native_io: true, distribution: "local", existing_stack: "tauri-webview"}`, permitted_ops read+write.
- Linux: same with linux/linux/desktop/local, precision_pointer/pointer, `focus_model: "sequential"`.
- Known gaps to record [F from SPEC]: `constraints.privacy`/`security`/`latency_ms` cannot be expressed — any of them makes the router abstain with `constraints.<field>_evidence`; a two-target product (one app, Android + Linux) must be routed as two intents — there is no multi-target/composition route in v0.x.
