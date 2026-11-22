# BATISSEUR routing log (slice B1)

Repo: /mnt/MD/AI-tools/batisseur, version 0.2.0 -> 0.3.0 (local commit only, not pushed).

## Before (known, from BATISSEUR-EXTENSION.md section 1)
- Android phone intent: `batisseur: target.form_factor: unknown enum value`, exit 2.
- Linux desktop intent: ok -> implement:linux-desktop-gui.

## After
Command: `batisseur route <intent> --json` (debug build, `time` wall-clock).

| Intent | route_status | selected_pack | capability | score | risk_gate | real time |
|---|---|---|---|---|---|---|
| intent-android.json (android/android/phone/local, direct_touch/touch/spatial) | ok | android-phone | implement:android-phone | 59 | allowed | 0.006 s |
| intent-linux.json (linux/linux/desktop/local, precision_pointer/pointer/sequential) | ok | linux-desktop-gui | implement:linux-desktop-gui | 59 | allowed | 0.005 s |

Score terms (both): platform_fit 30, interaction_fit 25, runtime_fit 10, distribution_fit 0, existing_stack_fit 0, verification_fit 0, accessibility_fit 0, dependency -2, maintenance -2, context -1, runtime -1, penalties 0 (sum 59). Single eligible pack in each case.
Outputs: /tmp/claude-1000/-mnt-MD-AI-tools/a657bbe8-a65c-45ae-9d37-79091d502cbd/scratchpad/route-android.json, /tmp/claude-1000/-mnt-MD-AI-tools/a657bbe8-a65c-45ae-9d37-79091d502cbd/scratchpad/route-linux.json.

## Gaps and limitations found
- `constraints.existing_stack = "tauri-webview"` routes identically (score 59, existing_stack_fit stays 0) for both intents, because tauri-webview is only a framework candidate, not the pack default stack (as designed). The router does not surface candidate choice; the stack decision stays with the owner.
- Not expressible (per SPEC): constraints.privacy/security/latency_ms; adding any makes the router abstain.
- A two-target product (Android + Linux) must be routed as two intents; no multi-target/composition route in v0.x.
- Scores and required verifiers are pack heuristics (costs mirror android-tablet), not measurements.

## Bench
`cargo run -q --offline --locked -- bench --json`: suite buildbench-v0, total 16, correct 16, accuracy 100.0, correct_abstentions 4/4, unexpected_activations 0, deterministic_three_runs 16, passed true (frozen bench file unchanged; tt-1.1.0 sha256 301e80ce...f6c9 matches provenance).

## Performance on this project (orchestrator assessment, 2026-11-22)
- Routing itself: 5-6 ms per intent, deterministic, offline; the 16-case bench stayed 16/16 after the extension.
- It earned its keep once: it exposed a real gap (no `phone` form factor, so an Android phone product could not be routed at all). Fixing that took one Sonnet slice (~75 s agent time) and a 0.3.0 release with 4 new tests.
- Limits seen: it only picks a pack and a verifier list. It could not express this project's real constraints (privacy, no-network, dual target), could not compose a two-target product, and `tauri-webview` stayed invisible as a candidate. It did not influence the Rust/Tauri choice, the slices or the review gates; those came from Opus's plan and Sol's reviews.
- Verdict: useful as a cheap sanity gate on "does a pack exist for this target"; not a planning tool yet. Suggested next steps: multi-target intents, constraint fields (privacy/offline), surface framework candidates in the route output.
