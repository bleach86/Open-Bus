//! The host's clock: dates and weekdays from seconds, leap days and all;
//! POSIX TZ rules' offsets either side of their changes, north and south
//! and odd; and a TZif file's, America/Chicago's, where the system has it.

#[cfg(test)]
mod tests {
    use crate::host::clock::{at, offset, rule_offset, DateTime};

    /// Seconds from 1970's start to `year`-`month`-`day` `hour`:`minute`, UTC.
    fn utc(year: i64, month: u32, day: u32, hour: i64, minute: i64) -> i64 {
        // Counted forward from a known day: 2000-01-01 was 946,684,800.
        let mut seconds = 946_684_800;
        let mut when = at(seconds);
        while (when.year, when.month, when.day) != (year, month, day) {
            seconds += if (when.year, when.month) < (year, month) || ((when.year, when.month) == (year, month) && when.day < day) { 86_400 } else { -86_400 };
            when = at(seconds);
        }
        seconds + hour * 3600 + minute * 60
    }

    #[test]
    fn a_date_and_its_weekday_from_seconds() {
        let date = |seconds| {
            let when: DateTime = at(seconds);
            (when.year, when.month, when.day, when.weekday, when.hour, when.minute, when.second)
        };
        assert_eq!(date(0), (1970, 1, 1, 4, 0, 0, 0), "1970 began on a Thursday");
        assert_eq!(date(951_782_400), (2000, 2, 29, 2, 0, 0, 0), "2000's leap day, a Tuesday");
        assert_eq!(date(1_700_000_000), (2023, 11, 14, 2, 22, 13, 20));
        assert_eq!(date(4_102_444_799), (2099, 12, 31, 4, 23, 59, 59));
        assert_eq!(date(-1), (1969, 12, 31, 3, 23, 59, 59));
    }

    #[test]
    fn a_rules_offset_either_side_of_its_changes() {
        let chicago = "CST6CDT,M3.2.0,M11.1.0";
        let (cst, cdt) = (-6 * 3600, -5 * 3600);
        assert_eq!(rule_offset(chicago, utc(2026, 1, 15, 12, 0)), Some(cst), "January");
        assert_eq!(rule_offset(chicago, utc(2026, 7, 1, 12, 0)), Some(cdt), "July");
        // The second Sunday of March 2026, the 8th, 2am CST: 8am UTC.
        assert_eq!(rule_offset(chicago, utc(2026, 3, 8, 7, 59)), Some(cst), "before summer time");
        assert_eq!(rule_offset(chicago, utc(2026, 3, 8, 8, 0)), Some(cdt), "summer time");
        // The first Sunday of November 2026, the 1st, 2am CDT: 7am UTC.
        assert_eq!(rule_offset(chicago, utc(2026, 11, 1, 6, 59)), Some(cdt), "before its end");
        assert_eq!(rule_offset(chicago, utc(2026, 11, 1, 7, 0)), Some(cst), "standard time again");

        // The south's summer across the year's end: the first Sunday of
        // October to the first of April, at 3am summer time.
        let sydney = "AEST-10AEDT,M10.1.0,M4.1.0/3";
        assert_eq!(rule_offset(sydney, utc(2026, 1, 15, 0, 0)), Some(11 * 3600), "Sydney's January");
        assert_eq!(rule_offset(sydney, utc(2026, 7, 15, 0, 0)), Some(10 * 3600), "Sydney's July");
        assert_eq!(rule_offset("<+0530>-5:30", 0), Some(5 * 3600 + 1800), "half an hour, its name quoted");
        assert_eq!(rule_offset("GMT0", 0), Some(0));
    }

    #[test]
    fn a_tzif_files_offset() {
        let Ok(zone) = std::fs::read("/usr/share/zoneinfo/America/Chicago") else {
            eprintln!("clock test: no America/Chicago in /usr/share/zoneinfo");
            return;
        };
        assert_eq!(offset(&zone, utc(2026, 1, 15, 12, 0)), Some(-6 * 3600), "January");
        assert_eq!(offset(&zone, utc(2026, 7, 1, 12, 0)), Some(-5 * 3600), "July");
        assert_eq!(offset(&zone, utc(1960, 1, 1, 12, 0)), Some(-6 * 3600), "before its first change listed");
        assert_eq!(offset(&zone, utc(2050, 7, 1, 12, 0)), Some(-5 * 3600), "past its last: its rule's");
        assert_eq!(offset(b"not a zone", 0), None);
    }
}
