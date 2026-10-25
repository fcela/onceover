//! Date utilities: civil-from-days (Howard Hinnant's algorithm), ISO
//! formatting, and weekday math. No external time crate.

/// Convert days since 1970-01-01 to (year, month, day).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

/// ISO "YYYY-MM-DD" from days since epoch.
pub fn iso_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    if y < 0 {
        format!("-{:04}-{:02}-{:02}", -y, m, d)
    } else {
        format!("{:04}-{:02}-{:02}", y, m, d)
    }
}

/// ISO "YYYY-MM-DD HH:MM:SS" from microseconds since epoch.
pub fn iso_datetime(us: i64) -> String {
    let days = us.div_euclid(86_400_000_000);
    let rem = us.rem_euclid(86_400_000_000);
    let secs = rem / 1_000_000;
    let (h, mi, s) = (secs / 3_600, (secs / 60) % 60, secs % 60);
    format!("{} {:02}:{:02}:{:02}", iso_date(days), h, mi, s)
}

/// Weekday of days-since-epoch: 0=Sunday .. 6=Saturday.
pub fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

pub fn is_weekend(days: i64) -> bool {
    let w = weekday(days);
    w == 0 || w == 6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_weekday() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(weekday(0), 4); // Thursday
        assert_eq!(iso_date(19_723), "2024-01-01");
        assert!(!is_weekend(4)); // 1970-01-01 Thu
        assert!(is_weekend(3)); // 1970-01-03 Sat
    }

    #[test]
    fn datetime_format() {
        assert_eq!(iso_datetime(0), "1970-01-01 00:00:00");
        // 1 day + 1h + 1m + 1s
        let us = 86_400_000_000 + 3_600_000_000 + 60_000_000 + 1_000_000;
        assert_eq!(iso_datetime(us), "1970-01-02 01:01:01");
    }
}
