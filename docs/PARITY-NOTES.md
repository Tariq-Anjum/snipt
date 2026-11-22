# Rust ↔ TS parity notes

Oracle: `src/engine/*.ts`. Fixtures: `scripts/gen-fixtures.ts` (TZ=Asia/Kolkata, fixed nowMs,
deterministic). Rust is kept equal to TS; deviations are listed here.

## Encoded hazards (covered by fixtures)
| Hazard | Rust handling | Evidence |
|---|---|---|
| `fnv1a32` hashes `charCodeAt(i) & 0xff` | iterate `encode_utf16()`, `unit & 0xff` | `parity_core::fnv_fixture_utf16_low_bytes` (₹, Devanagari, Urdu, emoji, U+0100) |
| JS `\b` is ASCII | `jsre` rewrites `\b` → `(?-u:\b)` | hazard cases `éOTP 482913é`, `G-392814é`; mutation to Unicode `\b` gives 2/2058 mismatches |
| JS non-unicode `/i` never folds U+212A/U+017F onto ASCII | `jsre` expands letters to `[kK]` instead of `(?i)` | `jsre::tests::ascii_semantics` |
| JS `\s`, `\d`, `trim()` | exact ECMAScript whitespace set (incl. U+FEFF, excl. U+0085); `\d` = `[0-9]`; `sms_types::js::trim` | hazard senders with U+FEFF / U+3000 / U+0085 |
| `body.length` = UTF-16 units | `js::utf16_len` (ml length feature, merchant length, scan byte estimate) | emoji hazard bodies |
| OTP prefix window is 12 UTF-16 units | `otp::utf16_prefix` | ₹ / emoji cases |
| `Float32Array` weights | weights stored as `f32`, widened to `f64` per multiply | `analyze.json` ml_linear scores within 1e-9 |
| Local-time cycles | explicit `tz_offset_min` (fixtures at +330) | `cycles.json` |

## Known residual differences (not reachable by current fixtures)
- Bounded quantifiers over negated classes (`[^0-9]{0,24}` in OTP_AFTER_KW) count code points in
  Rust but UTF-16 units in JS. Differs only when astral characters (emoji) sit between an OTP
  keyword and the code and push the JS count past 24. Accepted.
- `Math.sin/cos/exp/pow` (V8 fdlibm) vs Rust libm may differ in the last ulp; scores are
  compared with 1e-9 tolerance, categories/reasons exactly. An exact tie between two ML
  categories could in theory flip; none occurs in fixtures.
- `toLowerCase()` Unicode tables of V8's ICU vs Rust's `char::to_lowercase` may differ for
  characters added in newer Unicode versions.

## Intentional safety deviations (R1 review)
- Weekly scan: transaction and active-reminder exclusions run before spam bucketing (fixed in
  both the TS oracle and Rust; `scan.json` `blockedSafety`). Active reminder = BillDue / TravelPnr
  at any age, Delivery younger than 7 days.
- Timestamps: Rust rejects `|timestampMs| > 8.64e15` (JS `Date` range) at `Store::ingest_*` and
  `restore_encrypted_backup` (`NotAnArchive`); TS accepts any number there. Engine/store expiry and
  age arithmetic saturates (`sms_types::time::expiry_ms`, `saturating_sub`), so far-future rows
  never wrap into "expired" and far-past rows count as old.

## Billing cycles across DST (R1 optional, not fixed)
- `get_cycle_window` / `Store::tz_offset_min` use ONE fixed UTC offset for both cycle boundaries.
  TS `new Date(y, m, d)` resolves the local offset separately at each boundary, so in DST zones
  (e.g. America/New_York, March 2026: start at UTC−5, end at UTC−4) the Rust window can be shifted
  by one hour at one end; a transaction within an hour of local midnight on a boundary day can land
  in a different cycle. Asia/Kolkata (no DST; all fixtures) is exact.
- Not fixed because it is not API-compatible: per-boundary resolution needs an offset-at-instant
  source (a tz database such as `chrono-tz`, or a platform callback) threaded through
  `Store`/`sms-engine` and the Tauri layer, which currently pass a single `i32`. Fix when a
  non-DST-free market is in scope: replace `tz_offset_min: i32` with an `offset_at(ms) -> i32`
  resolver and compute start/end offsets independently.

## sms-store vs store.ts (in-memory arrays → SQLite)
- Thread list ties on `latest_ts` are ordered by `thread_id DESC` (keyset paging needs a total
  order); TS keeps first-appearance order. No ties in fixtures.
- Stable-sort tie-breaker inside a thread is `ins_seq` (insertion order = TS array order).
- Search: FTS5 trigram only narrows candidates; every hit is verified in Rust with JS lowercase
  semantics, so there are no false positives. FTS is used only for tokens of ≥ 3 chars made of
  ASCII or caseless non-combining chars (`search.rs` `fts_usable`); tokens with cased non-ASCII
  letters (SQLite folds `İ`/final `ς` differently from JS), combining marks or NUL use the exact
  scan, so no false negatives either (test `search_tokens_fts_cannot_represent`). Cost: such
  tokens scan the whole history instead of the index.
- OTP/reminder list ordering among equal timestamps follows thread list order, then `ins_seq`.

## Intentional safety deviations (R2/R3 review, 2026-11-22)
None of these touch `sms-engine` or the fixtures; the TS oracle is unchanged
(`TZ=Asia/Kolkata npx tsx --test src/engine/engine.test.ts`).
- **Ids are never reused.** store.ts allocates `max(id)+1` / `max(threadId)+1`
  (a deleted newest id comes back). sms-store keeps persistent high-water marks
  (`id_alloc`, schema v2), so a stale notification action, send callback or UI
  reference can never address a newer message or thread. First ids are still
  1001 / 101, so a fresh store allocates exactly like store.ts until something
  is deleted.
- **Id range.** Message/thread ids must be in `1..=2^53-1` (JS safe integers:
  the UI and `.mysms.enc` carry them as JS numbers). Out-of-range ids are
  rejected on insert and restore; TS accepts any number. Allocation past the
  bound fails instead of producing a SQLite REAL.
- **Restore merge.** The TS dev path replaces its in-memory state. Rust merges
  atomically: an incoming message is "already present" only when sender
  (case-insensitive), timestamp, body and direction match a stored row; an id
  collision with a different message is kept under a fresh id, and thread /
  archived / reminder references follow the remap. Restoring into an empty
  store keeps every id, so TS-written archives restore identically (fixture
  test `ts_written_backup_restores_through_core_and_is_idempotent`).
- **OTP masking at the IPC edge.** `messages_page` bodies/`otp.code` and weekly
  scan sample bodies are masked by the Tauri core after the engine ran; the
  scan buckets, ids and byte estimates are the engine's (TS-equal) output.
- **Send state** (`queued < sending < sent < unconfirmed < delivered`,
  `failed`) has no TS counterpart; store.ts outgoing rows have no status.
