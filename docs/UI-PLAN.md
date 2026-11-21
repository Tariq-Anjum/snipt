# mySMS — UI Plan (Liquid Glass on system webviews)

Labels: **[F]** verified this session · **[I]** inference · **[R]** recommendation · **[U]** must be measured.

## 1. Goal and constraints
Reuse the approved React UI (App.tsx + 9 components, `src/index.css` Liquid Glass | Crisp Solid) on two engines:
- Android: system WebView (Chromium, auto-updated via Play) — `backdrop-filter` supported since Chromium 76 [I, prior knowledge].
- Linux: WebKitGTK 2.54.3 (installed [F]) — supports `backdrop-filter` and `-webkit-backdrop-filter` [I]; `index.css` already declares both [F, 8 occurrences].

Keep: Tailwind 4, motion, lucide, existing components. Change: data source (Tauri `invoke` adapter instead of in-page TS engine), list virtualization, fonts, perf guards.

## 2. Code changes to the UI (thin)
- `src/api/` (new): `threads.ts`, `messages.ts`, `finance.ts`, `backup.ts`, `vault.ts`, `platform.ts` — typed wrappers over `invoke()`; types imported from `src/engine/types.ts` (types only; type imports are erased so the TS engine is not bundled).
- Remove from the shipped bundle: `RustCoreView.tsx` + `src/data/rustWorkspace.ts` (40 KB blueprint viewer) and the in-page engine/seed (`SimulateSmsModal` stays behind a `import.meta.env.DEV` flag).
- `InboxView.tsx` (55 KB): thread list and conversation list become virtualized windows fed by keyset pages (50 rows); search calls the Rust FTS command (debounced 150 ms).
- Platform gating from `platform.capabilities()`: Linux hides send/reply/role UI and shows "Import .mysms.enc"; Android shows the default-SMS onboarding (with the RCS/MMS honesty notes from ARCHITECTURE §6.4).
- Safe areas: CSS `padding-top: max(env(safe-area-inset-top), var(--inset-top, 0px))` etc.; Kotlin pushes `--inset-*` vars (edge-to-edge is mandatory at targetSdk ≥ 36 [F]).
- Back: overlays/thread view register with a small back-stack; when non-empty, `setBackHandler(true)`, else false so predictive back-to-home works [F Android 16 behaviour change].
- Fonts: drop the Google Fonts `<link>` (network + privacy); self-host `@fontsource-variable/dm-sans` and `@fontsource-variable/plus-jakarta-sans` 5.3.0 (OFL-1.1 fonts [F npm]) latin subset only; JetBrains Mono → `ui-monospace` stack. Indic/Urdu text uses system Noto (Linux has Noto Devanagari/Tamil/Arabic installed [F fc-list]; Android ships Noto).

## 3. Liquid Glass rules for webviews
Cost model [I]: every `backdrop-filter` element forces an offscreen render of what is behind it, every frame it or its backdrop changes. Cost scales with blurred area × radius and with how often the backdrop moves (scrolling content under a sticky header = every frame).

Rules [R]:
1. **Max 3 simultaneously visible blurred surfaces** (top bar, bottom dock, one sheet/dialog). Never per-row/per-card glass inside a scrolling list; cards use a translucent gradient + 1 px inner highlight (no blur).
2. Blur radius: chrome ≤ 24 px (current `blur(24px) saturate(185%)` OK), sheets ≤ 16 px, toasts 0 (solid translucent).
3. Ambient background (`liquid-ambient-a/b`) animates `transform` only, on its own layer; [R] pause it (`animation-play-state: paused`) while the user scrolls and when the window is hidden (`visibilitychange`).
4. Animate only `transform`/`opacity`; no `layout` animations from motion on virtualized rows; no animated `box-shadow`/`filter`.
5. Rows: `contain: layout paint style; content-visibility: auto` on long conversation bubbles.
6. Fallbacks: `@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px)))` → Crisp Solid tokens. `prefers-reduced-motion` already respected; [U] `prefers-reduced-transparency` → Crisp Solid where the engine supports the query.
7. Auto-degrade: Android `powerState().powerSave` or thermal ≥ severe → Crisp Solid (toast once). Runtime frame monitor (rAF deltas, 2 s window): if > 5 % of frames exceed 1.5× budget during scroll, switch to Crisp Solid for the session.
8. Linux window stays **opaque** (no transparent Tauri window / compositor blur): Hyprland blur rules would add compositor cost and differ per WM [I].

## 4. Adaptive refresh / jank budget
| Display | Frame budget | Target |
|---|---|---|
| 120 Hz | 8.3 ms | scroll p95 ≤ 8.3 ms, janky frames ≤ 1 % |
| 90 Hz | 11.1 ms | same rule |
| 60 Hz | 16.7 ms | same rule (emulator, Linux 60 Hz panels) |

Mechanics [R]:
- JS work per frame during scroll ≤ 4 ms (virtualizer + React reconcile); IPC never on the scroll path — prefetch the next page at 70 % scroll; Rust commands are `async` (Tauri thread pool).
- Page payload ≤ 64 KB JSON (50 threads with previews truncated to 140 chars).
- Android boost: `display.boost(true)` on `pointerdown`/scroll start, release 1.5 s after the last input; no boost in power-save. API 35+ uses `View.setRequestedFrameRate(...HIGH)`; API 30–34 `preferredDisplayModeId`.
- Linux: GTK frame clock follows the monitor; nothing to do.
- Measurement: Android `adb shell dumpsys gfxinfo <pkg> framestats` + `adb shell dumpsys SurfaceFlinger --latency`; in-app debug overlay (rAF histogram, DEV only). Linux: in-app overlay.
- [U] Whether a WebView honours `setRequestedFrameRate` on LTPO panels, and real 120 Hz behaviour, **cannot be verified on the emulator (60 Hz)**; requires the owner's phone.

## 5. Icon plan
- Source: extract the SVG from `MySmsGlassIcon.tsx` / `index.html` favicon into `assets/icon/mysms.svg` (single source).
- Raster: `rsvg-convert -w 1024 -h 1024 assets/icon/mysms.svg -o assets/icon/mysms-1024.png` (rsvg-convert installed [F]) → `npx tauri icon assets/icon/mysms-1024.png` generates desktop PNGs and Android mipmaps [I: tauri icon output set verified in S10/S11].
- Android adaptive icon: foreground = white bubble glyph (66 % safe zone), background = blue gradient, **monochrome** layer = glyph only (themed icons, Android 13+). Edit `gen/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml` to add `<monochrome>`.
- Linux: hicolor 32–512 PNG + scalable SVG (install-linux.sh).
- In-app icons: keep `lucide-react` (tree-shaken named imports only; never `import * as`).

## 6. Third-party UI libraries (vetted 2026-11-22)
Criteria: MIT/Apache/BSD(-equivalent), maintained, small, solves a real function. Sizes = bundlephobia min+gzip (full package; tree-shaking reduces lucide/motion further).

| Library | Licence [F] | Maintenance [F GitHub pushed] | Size gz [F] | Verdict |
|---|---|---|---|---|
| @tanstack/react-virtual 3.14.26 | MIT | 2026-11-21, 7.2k★ | 10.3 KB | **Adopt** — required for 10k-message lists at 120 Hz |
| motion 12.43.0 (already used) | MIT | 2026-11-21 | 64 KB full | **Keep**, switch to `LazyMotion` + `m` + `domAnimation` to cut the shipped cost [I ~15 KB]; stay on 12.x (14.x is a major bump) |
| lucide-react 0.546.0 (already used) | ISC (GitHub shows NOASSERTION; LICENSE file is ISC [F]) | 2026-11-20 | per-icon after tree-shake | **Keep** — ISC is BSD-equivalent; owner ack needed |
| sonner 2.0.8 | MIT | 2026-08-10, 13k★ | 9.4 KB | **Optional** — adopt only if it replaces `NotificationBanner` toast code (undo-delete, "clipboard clears in 30 s") with less code |
| @fontsource-variable/* 5.3.0 | OFL-1.1 (font) | — | ~20–40 KB/font woff2 latin [I] | **Adopt** (self-hosted fonts; owner ack OFL) |
| clsx 2.1.1 | MIT | 2024-06 (stable, finished) | 0.35 KB | Optional, trivial |
| vaul 1.1.2 | MIT | README: "This repo is unmaintained" [F] | 18.5 KB | **Reject** — bottom sheet done with motion drag |
| cmdk 1.1.1 | MIT | 2025-10 | 16.7 KB | **Reject** — FTS search covers it; no phone use |
| react-virtuoso 4.18.24 | MIT (npm) | 2026-11-21 | 19.9 KB | **Reject** — duplicates TanStack, larger |
| @radix-ui/react-dialog 1.3.1 | MIT | 2026-10-14 | 13.2 KB | **Defer** — only if a11y audit finds focus-trap bugs in existing modals |
| rdev/liquid-glass-react | MIT | 2025-06-13 (stale) | — | **Reject** — SVG displacement; README: Safari/Firefox "displacement will not be visible" → broken on WebKitGTK, inconsistent across our two engines |
| Haze (Compose) | Apache-2.0 | — | — | N/A (no Compose UI) |

Bundle budget [R]: initial JS ≤ 250 KB gzip, CSS ≤ 30 KB gzip, fonts ≤ 100 KB; checked from `vite build` output in S9 and recorded.

## 7. UI acceptance checks
- `npm run lint` (tsc) and `npm run build` pass; bundle sizes within budget.
- Linux: 10k-message DB scrolls with overlay showing ≤ 1 % frames > 16.7 ms at 60 Hz (this machine) [U actual result].
- Android emulator: functional flows (inbox, thread, search, finance, vault reveal + copy, backup/restore, theme toggle) and `gfxinfo` janky-frame % recorded (emulator numbers are indicative only).
- Crisp Solid toggle and auto-degrade both reachable.
