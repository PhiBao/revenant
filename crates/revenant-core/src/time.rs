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

/// Parse an HTTP date in the IMF-fixdate form, for example `Fri, 25 Feb 2000 19:22:45 GMT`.
///
/// Only this form is accepted (R6.10). Other historic forms, a wrong weekday name, a wrong zone,
/// or an impossible date give `None`. Never panics.
pub fn parse_http_date(s: &str) -> Option<i64> {
    const DAYS: [&str; 7] = ["Mon,", "Tue,", "Wed,", "Thu,", "Fri,", "Sat,", "Sun,"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parts: Vec<&str> = s.trim().split(' ').collect();
    let [weekday, day, month, year, clock, zone] = parts.as_slice() else {
        return None;
    };
    if !DAYS.contains(weekday) || *zone != "GMT" {
        return None;
    }
    if day.len() != 2 || year.len() != 4 {
        return None;
    }
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if !digits(day) || !digits(year) {
        return None;
    }
    let day: u32 = day.parse().ok()?;
    let year: i64 = year.parse().ok()?;
    let month = MONTHS.iter().position(|m| m == month)? as u32 + 1;

    let clock: Vec<&str> = clock.split(':').collect();
    let [h, m, sec] = clock.as_slice() else {
        return None;
    };
    if [h, m, sec].iter().any(|t| t.len() != 2 || !digits(t)) {
        return None;
    }
    let (h, m, sec): (i64, i64, i64) = (h.parse().ok()?, m.parse().ok()?, sec.parse().ok()?);
    if day == 0 || day > days_in_month(year, month) || h > 23 || m > 59 || sec > 59 {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + h * 3_600 + m * 60 + sec)
}

/// Format Unix seconds (UTC) as a 14-digit CDX timestamp.
pub fn format_cdx_timestamp(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
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
    fn formats_what_it_parses() {
        for s in [
            "20000301091452",
            "19991231235959",
            "20000229120000",
            "19700101000000",
        ] {
            let unix = parse_cdx_timestamp(s).expect("valid");
            assert_eq!(format_cdx_timestamp(unix), s);
        }
    }

    #[test]
    fn parses_the_http_dates_seen_on_the_y2k_files() {
        // Real x-archive-orig-last-modified values (Nov 2026 review).
        for (http, cdx) in [
            ("Fri, 25 Feb 2000 19:22:45 GMT", "20000225192245"),
            ("Thu, 02 Mar 2000 01:06:59 GMT", "20000302010659"),
            ("Mon, 27 Dec 1999 19:41:03 GMT", "19991227194103"),
        ] {
            assert_eq!(parse_http_date(http), parse_cdx_timestamp(cdx), "{http}");
        }
    }

    #[test]
    fn rejects_every_other_http_date_form() {
        for bad in [
            "",
            "Friday, 25-Feb-00 19:22:45 GMT",
            "Fri Feb 25 19:22:45 2000",
            "Fri, 25 Feb 2000 19:22:45 UTC",
            "Fri, 25 Feb 2000 19:22:45 +0000",
            "Fri, 5 Feb 2000 19:22:45 GMT",
            "Fri, 25 Feb 00 19:22:45 GMT",
            "Fri, 30 Feb 2000 19:22:45 GMT",
            "Fri, 25 Foo 2000 19:22:45 GMT",
            "Xyz, 25 Feb 2000 19:22:45 GMT",
            "Fri, 25 Feb 2000 24:00:00 GMT",
            "Fri, 25 Feb 2000 19:60:00 GMT",
            "Fri, 25 Feb 2000 19:22 GMT",
            "Fri,  25 Feb 2000 19:22:45 GMT",
            "Fri, 25 Feb 2000 19:22:45 GMT extra",
        ] {
            assert_eq!(parse_http_date(bad), None, "accepted {bad:?}");
        }
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
