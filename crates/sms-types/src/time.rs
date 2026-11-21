//! Civil-date helpers for local-time rules (JS `new Date(y, m, d)` /
//! `getFullYear()` etc.) evaluated with an explicit fixed UTC offset in minutes.

pub const DAY_MS: i64 = 86_400_000;

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month 1-12, day) for days since epoch.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Local calendar date (year, month 1-12, day) of a UTC instant.
pub fn local_date(ms: i64, tz_offset_min: i32) -> (i64, i64, i64) {
    civil_from_days((ms + i64::from(tz_offset_min) * 60_000).div_euclid(DAY_MS))
}
