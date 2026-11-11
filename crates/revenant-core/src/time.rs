//! Wayback CDX timestamps (`YYYYMMDDhhmmss`) as Unix seconds in UTC.

/// Parse a 14-digit CDX timestamp into Unix seconds (UTC).
///
/// Returns `None` for any malformed or impossible value. Never panics.
pub fn parse_cdx_timestamp(s: &str) -> Option<i64> {
    if s.len() != 14 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: u32 = s[4..6].parse().ok()?;
    let day: u32 = s[6..8].parse().ok()?;
    let hour: i64 = s[8..10].parse().ok()?;
    let minute: i64 = s[10..12].parse().ok()?;
    let second: i64 = s[12..14].parse().ok()?;

    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        _ => 28,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_instants() {
        // 2000-01-01T00:00:00Z is 946684800.
        assert_eq!(parse_cdx_timestamp("20000101000000"), Some(946_684_800));
        // One second before the Year 2000 rollover.
        assert_eq!(parse_cdx_timestamp("19991231235959"), Some(946_684_799));
    }

    #[test]
    fn handles_leap_days() {
        assert!(parse_cdx_timestamp("20000229120000").is_some());
        assert!(parse_cdx_timestamp("19000229120000").is_none()); // 1900 is not a leap year
    }

    #[test]
    fn rejects_malformed_values() {
        for bad in [
            "",
            "1999",
            "1999123123595",
            "19991301000000",
            "19991200000000",
            "19991232000000",
            "19991231240000",
            "1999123123596a",
        ] {
            assert_eq!(parse_cdx_timestamp(bad), None, "accepted {bad:?}");
        }
    }
}
