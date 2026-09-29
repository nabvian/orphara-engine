//! Minimal, dependency-free date handling: validation, ISO formatting, and
//! day arithmetic over the proleptic Gregorian calendar. Enough for the
//! pipeline's episode windows and lookbacks; no time zones, no clock.

/// Days since 1970-01-01 for a valid `(year, month, day)`, else `None`.
pub fn to_epoch_day(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    if d > days_in_month(y, m) {
        return None;
    }
    // Howard Hinnant's days-from-civil algorithm.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap(y) { 29 } else { 28 },
        _ => 0,
    }
}

/// `YYYY-MM-DD` (no validation beyond field ranges via `to_epoch_day`).
pub fn iso(y: i64, m: i64, d: i64) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse an ISO `YYYY-MM-DD` prefix to an epoch day.
pub fn epoch_from_iso(s: &str) -> Option<i64> {
    let s = s.get(..10).unwrap_or(s);
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    to_epoch_day(y, m, d)
}

/// Signed day difference `a - b` for two ISO dates, or `None` if unparseable.
pub fn days_between(a: &str, b: &str) -> Option<i64> {
    Some(epoch_from_iso(a)? - epoch_from_iso(b)?)
}

/// ISO date `interval` days after the given ISO date.
pub fn plus_days_iso(iso_date: &str, interval: i64) -> Option<String> {
    let ed = epoch_from_iso(iso_date)? + interval;
    Some(iso_from_epoch(ed))
}

/// Inverse of `to_epoch_day`.
pub fn iso_from_epoch(z: i64) -> String {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    iso(y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_diff() {
        let e = to_epoch_day(2026, 5, 4).unwrap();
        assert_eq!(iso_from_epoch(e), "2026-05-04");
        assert_eq!(days_between("2026-05-05", "2026-05-01"), Some(4));
        assert_eq!(plus_days_iso("2026-01-01", 56).as_deref(), Some("2026-02-26"));
    }

    #[test]
    fn rejects_impossible() {
        assert_eq!(to_epoch_day(2026, 2, 30), None);
        assert_eq!(to_epoch_day(2026, 13, 1), None);
    }
}
