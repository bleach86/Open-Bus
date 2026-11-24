//! The host's clock: the date and time now, where the host is, for a
//! machine's clock card. Its zone's offset from UTC is from the TZif file
//! the system keeps, /etc/localtime: the offset of the last change of it
//! listed before now; or, past those, by the rule at the file's end (as
//! `CST6CDT,M3.2.0,M11.1.0`: standard time 6 hours west, summer time from
//! the second Sunday of March to the first of November). Without the file,
//! UTC. A web page has neither the file nor the system's time: it says
//! what the time is there, `set_local`, for now to give.

#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Seconds since a moment of no importance, for measuring what's between
/// two of them.
#[cfg(not(target_arch = "wasm32"))]
pub fn seconds() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

// A web page has no Instant: its own clock, for the same.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function seconds() { return performance.now() / 1000; }")]
extern "C" {
    pub fn seconds() -> f64;
}

/// How far into its second the host's time is, 0 to 1: for a clock set to
/// `now`, which has whole seconds, to turn over as the host's does.
#[cfg(not(target_arch = "wasm32"))]
pub fn into_second() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.subsec_nanos() as f64 / 1e9)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function into_second() { return (Date.now() % 1000) / 1000; }")]
extern "C" {
    pub fn into_second() -> f64;
}

/// A date and time: the day of the week 0 Sunday to 6 Saturday.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static LOCAL_SECONDS: Cell<i64> = const { Cell::new(0) };
}

/// The time where the page is: `seconds` after 1970 began, there.
#[cfg(target_arch = "wasm32")]
pub fn set_local(seconds: i64) {
    LOCAL_SECONDS.set(seconds);
}

/// Now, where the page is.
#[cfg(target_arch = "wasm32")]
pub fn now() -> DateTime {
    at(LOCAL_SECONDS.get())
}

/// Now, where the host is.
#[cfg(not(target_arch = "wasm32"))]
pub fn now() -> DateTime {
    let utc = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    let zone = std::fs::read("/etc/localtime").unwrap_or_default();
    at(utc + offset(&zone, utc).unwrap_or(0))
}

/// The date and time `seconds` after 1970 began.
pub fn at(seconds: i64) -> DateTime {
    let (days, second_of_day) = (
        seconds.div_euclid(86_400),
        seconds.rem_euclid(86_400) as u32,
    );
    let (year, month, day) = civil(days);
    DateTime {
        year,
        month,
        day,
        weekday: (days + 4).rem_euclid(7) as u32, // 1970 began on a Thursday
        hour: second_of_day / 3600,
        minute: second_of_day / 60 % 60,
        second: second_of_day % 60,
    }
}

/// The year, month and day `days` after 1970 began (Howard Hinnant's
/// civil_from_days: years from March, so February's leap day falls last).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + (month <= 2) as i64;
    (year, month, day)
}

/// The days from 1970's start to `year`, `month`'s first day.
fn days_from_civil(year: i64, month: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_index = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_index + 2) / 5;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// A TZif file's offset from UTC, in seconds, at `utc` seconds; none if
/// it isn't one.
pub fn offset(zone: &[u8], utc: i64) -> Option<i64> {
    let counts = |at: usize| -> Option<[usize; 6]> {
        let header = zone.get(at..at + 44)?;
        if &header[..4] != b"TZif" {
            return None;
        }
        Some(std::array::from_fn(|n| {
            u32::from_be_bytes(header[20 + 4 * n..24 + 4 * n].try_into().unwrap()) as usize
        }))
    };
    let [isut, isstd, leaps, times, types, chars] = counts(0)?;
    // Version 2 on: a second block, its times 64 bits; then the rule.
    let (at, width) = if zone[4] >= b'2' {
        (
            44 + times * 5 + types * 6 + chars + leaps * 8 + isstd + isut,
            8,
        )
    } else {
        (0, 4)
    };
    let [isut, isstd, leaps, times, types, chars] = counts(at)?;
    let data = zone.get(at + 44..)?;
    let time = |n: usize| -> i64 {
        let bytes = &data[n * width..n * width + width];
        if width == 8 {
            i64::from_be_bytes(bytes.try_into().unwrap())
        } else {
            i32::from_be_bytes(bytes.try_into().unwrap()) as i64
        }
    };
    let type_offset = |kind: usize| -> Option<i64> {
        let info = data.get(times * width + times + kind * 6..)?;
        Some(i32::from_be_bytes(info[..4].try_into().unwrap()) as i64)
    };
    let last = (0..times).take_while(|&n| time(n) <= utc).last();
    // Past the last change listed, version 2's rule, if it has one.
    if width == 8 && (times == 0 || last == Some(times - 1)) {
        let footer = data.get(times * 9 + types * 6 + chars + leaps * 12 + isstd + isut..)?;
        let rule = String::from_utf8_lossy(footer);
        if let Some(offset) = rule
            .trim()
            .lines()
            .next()
            .and_then(|rule| rule_offset(rule, utc))
        {
            return Some(offset);
        }
    }
    match last {
        Some(n) => type_offset(data[times * width + n] as usize),
        None => type_offset(0),
    }
}

/// A POSIX TZ rule's offset at `utc`: `STD offset [DST [offset] ,start
/// [/time] ,end [/time]]`, its offsets hours west, its starts and ends
/// `Mm.w.d`, the month's wth (5, the last) day d; none if it isn't one of
/// those.
pub fn rule_offset(rule: &str, utc: i64) -> Option<i64> {
    let mut rest = rule;
    let name = |rest: &mut &str| -> Option<()> {
        if let Some(quoted) = rest.strip_prefix('<') {
            let end = quoted.find('>')?;
            *rest = &quoted[end + 1..];
        } else {
            let end = rest
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            if end < 3 {
                return None;
            }
            *rest = &rest[end..];
        }
        Some(())
    };
    // A time, [+-]h[:m[:s]], in seconds.
    let time = |rest: &mut &str| -> Option<i64> {
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == ':' || c == '+' || c == '-'))
            .unwrap_or(rest.len());
        let (text, after) = rest.split_at(end);
        *rest = after;
        let (sign, digits) = match text.strip_prefix('-') {
            Some(digits) => (-1, digits),
            None => (1, text.trim_start_matches('+')),
        };
        let mut parts = digits.split(':').map(|part| part.parse::<i64>());
        let hours = parts.next()?.ok()?;
        let minutes = parts.next().unwrap_or(Ok(0)).ok()?;
        let seconds = parts.next().unwrap_or(Ok(0)).ok()?;
        Some(sign * (hours * 3600 + minutes * 60 + seconds))
    };
    name(&mut rest)?;
    let standard = -time(&mut rest)?;
    if rest.is_empty() {
        return Some(standard);
    }
    name(&mut rest)?;
    let summer = if rest.starts_with(',') {
        standard + 3600
    } else {
        -time(&mut rest)?
    };
    // Mm.w.d[/time]: when, in the year `year`, local, standard or summer.
    let change = |spec: &str, year: i64| -> Option<i64> {
        let (date, at) = spec
            .split_once('/')
            .map_or((spec, None), |(date, at)| (date, Some(at)));
        let mut fields = date
            .strip_prefix('M')?
            .split('.')
            .map(|field| field.parse::<i64>().ok());
        let (month, week, weekday) = (fields.next()??, fields.next()??, fields.next()??);
        let first = days_from_civil(year, month as u32);
        let first_weekday = (first + 4).rem_euclid(7);
        let mut day = first + (weekday - first_weekday).rem_euclid(7) + 7 * (week - 1);
        let next_month = days_from_civil(
            year + (month == 12) as i64,
            if month == 12 { 1 } else { month as u32 + 1 },
        );
        while day >= next_month {
            day -= 7; // week 5: the last
        }
        let mut seconds_in = 7200;
        if let Some(mut at) = at {
            seconds_in = time(&mut at)?;
        }
        Some(day * 86_400 + seconds_in)
    };
    let mut changes = rest.strip_prefix(',')?.split(',');
    let (start, end) = (changes.next()?, changes.next()?);
    let year = at(utc + standard).year;
    let (starts, ends) = (change(start, year)? - standard, change(end, year)? - summer);
    let in_summer = if starts < ends {
        utc >= starts && utc < ends
    } else {
        utc >= starts || utc < ends
    };
    Some(if in_summer { summer } else { standard })
}
