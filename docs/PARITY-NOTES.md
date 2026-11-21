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

## sms-store vs store.ts (in-memory arrays → SQLite)
- Thread list ties on `latest_ts` are ordered by `thread_id DESC` (keyset paging needs a total
  order); TS keeps first-appearance order. No ties in fixtures.
- Stable-sort tie-breaker inside a thread is `ins_seq` (insertion order = TS array order).
- Search: FTS5 trigram only narrows candidates (tokens ≥ 3 chars); every hit is verified in Rust
  with JS lowercase semantics, so there are no false positives. A false negative is possible only
  where SQLite's trigram case folding differs from JS `toLowerCase()` on non-ASCII text.
- OTP/reminder list ordering among equal timestamps follows thread list order, then `ins_seq`.
