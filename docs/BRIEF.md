# mySMS — owner brief (2026-11-22)

Owner: Tariq. Orchestrator: Sonnet 5.5. Engineer/architect/designer: Opus. Reviewer: Codex Sol-6.2. Router log: batisseur.

## Goal
Final installable apps from the existing prototype (repo root = React/Vite prototype, TS engine in `src/engine`, Rust+Kotlin blueprint as strings in `src/data/rustWorkspace.ts`):
1. **Android APK** (arm64, installable by sideload, signed) — full default-SMS app (receive/send/read/delete, Dual-SIM, RCS UI with SMS fallback).
2. **Linux app** for this machine (CachyOS/Arch, Wayland+X11) — reads the phone's encrypted `.mysms.enc` backup (PBKDF2+AES-256-GCM); live Wi-Fi sync is v2.
Same UI on both: the approved **Liquid Glass** design (+ Crisp Solid toggle), glass icon, buttery adaptive-refresh motion.

## Owner decisions (answered)
- Build style: ONE app for both devices, Rust does all heavy work, shared UI. Preferred stack: **Tauri 2** (Rust core + web UI, Android + Linux). Opus may overturn ONLY with evidence and must say so.
- "Fastest language, preferably Rust": engine/store/crypto/backup/translate in Rust crates; UI layer as thin as possible.
- Use free open-source UI/UX libraries from GitHub where they improve function (check licence: MIT/Apache/BSD only; no GPL in the shipped app).
- Use features already built in the prototype: 10,000+ SMS capacity, weekly useless-SMS scan, RCS, Indian UPI/credit-card/savings detection, per-method monthly billing cycle, auto-fed budget planner, encrypted OTP vault (30 s clipboard wipe, FLAG_SECURE, EXTRA_IS_SENSITIVE), offline translate Hindi/Urdu/Hinglish/Marathi/Tamil/Telugu/Bengali -> English, multi-cloud encrypted backup via SAF/file picker, actionable notifications.
- Personal use now; Play Store later (keep zero-backend, no analytics, privacy-clean).

## Environment (verified 2026-11-22)
Rust 1.98.1 (rustup), node, npm, herdr, codex installed. NO Java/JDK, Gradle, Android SDK/NDK. 16 cores, 14 GiB RAM, 466 GB free on /mnt/MD.
Install user-scoped under `/mnt/MD/AI-tools/android-sdk`, `/mnt/MD/AI-tools/jdk` (Rust Android targets via rustup are fine). Owner pre-approved bash/sudo, but prefer rootless, reversible, no system-wide changes.

## Hard rules (owner AGENTS.md)
- Work only in `/mnt/MD/AI-tools/mySMS` (+ android-sdk/jdk dirs + batisseur extension below). Do NOT touch ZHOOR-AIOS, my-ai-brain, OB1, live `skills/`, tisserand/repo.
- No pushes, no repo creation, no registry/OB1 writes (owner-run only). Local commits OK.
- Never print/commit secrets. A Gemini key was pasted earlier in chat: it is NOT used anywhere in this project; do not add it.
- Evidence over claims: report exact commands run + results. Nothing "done" without verification.

## Batisseur (router + must be improved)
`batisseur route` currently REJECTS Android phone intents: `target.form_factor: unknown enum value` (only android-tablet / android-tv packs exist). Opus must design a minimal, contract-respecting extension: `phone` form factor + `android-phone` pack (and whatever a Tauri/webview-shell stack entry needs), keeping tt-1.1.0 pinned, tests + bench passing. Log how batisseur performed on this project (route outputs for android+linux intents, gaps found, time) in `docs/BATISSEUR-LOG.md`.

## Prototype conversation history (design intent)
See `docs/ORIGIN.md`.

## Deliverables
- Source in repo; `cargo test` green; engine parity tests vs the 10 TS test cases.
- `dist/mySMS-arm64.apk` (signed with a local keystore stored OUTSIDE git) and `dist/mySMS-linux-x86_64` (+ `install-linux.sh` user-scoped, .desktop, icon).
- Review report by Codex Sol-6.2; blockers fixed; `docs/RESULT.md` with exact install steps for phone and Linux.
