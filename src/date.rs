//! Calendar dates and the pluggable "less than six months" window rule.

use std::fmt;

/// A proleptic Gregorian calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Result<Date, String> {
        if !(1..=9999).contains(&year) {
            return Err(format!("year {year} out of range 1..=9999"));
        }
        if !(1..=12).contains(&month) {
            return Err(format!("month {month} out of range"));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(format!("day {day} does not exist in {year}-{month:02}"));
        }
        Ok(Date { year, month, day })
    }

    /// Parses `YYYY-MM-DD`.
    pub fn parse(s: &str) -> Result<Date, String> {
        let parts: Vec<&str> = s.trim().split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
            return Err(format!("date {s:?} is not YYYY-MM-DD"));
        }
        let num = |p: &str| -> Result<u32, String> {
            if !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("date {s:?} is not YYYY-MM-DD"));
            }
            p.parse::<u32>().map_err(|e| e.to_string())
        };
        Date::new(num(parts[0])? as i32, num(parts[1])?, num(parts[2])?)
    }

    /// Days since 1970-01-01 (negative before). Howard Hinnant's `days_from_civil`.
    pub fn day_number(self) -> i64 {
        let y = self.year as i64 - if self.month <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = self.month as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Inverse of [`Date::day_number`].
    pub fn from_day_number(z: i64) -> Date {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + if month <= 2 { 1 } else { 0 }) as i32;
        Date { year, month, day }
    }

    /// Moves forward `months` calendar months, clamping the day to the end of
    /// the target month (Aug 31 + 6 months = Feb 28, or Feb 29 in a leap year).
    pub fn add_months_clamped(self, months: u32) -> Date {
        let index = self.year as i64 * 12 + (self.month as i64 - 1) + months as i64;
        let year = index.div_euclid(12) as i32;
        let month = (index.rem_euclid(12) + 1) as u32;
        let day = self.day.min(days_in_month(year, month));
        Date { year, month, day }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Decides whether two trade dates are close enough to be matched.
///
/// A rule is described by the first date that is *outside* the window when
/// counting forward from the earlier trade. Two trades on dates `a <= b` are
/// matchable exactly when `b < window_end(a)`. Order does not matter: a sale
/// before a purchase is matched the same way as a purchase before a sale.
pub trait WindowRule {
    /// The first date, on or after `earlier`, that is no longer inside the window.
    fn window_end(&self, earlier: Date) -> Date;

    /// Short machine-readable name, used in CLI output.
    fn name(&self) -> String;
}

/// Whether trades on dates `a` and `b` (in either order) fall inside `rule`'s window.
pub fn within(rule: &dyn WindowRule, a: Date, b: Date) -> bool {
    let (earlier, later) = if a <= b { (a, b) } else { (b, a) };
    later < rule.window_end(earlier)
}

/// The default rule: less than six calendar months, counted forward from the
/// earlier trade with month-end clamping.
///
/// * Jan 31 -> window ends Jul 31, so Jul 30 matches and Jul 31 does not.
/// * Aug 31 2023 -> window ends Feb 29 2024; Aug 31 2022 -> ends Feb 28 2023.
/// * Feb 29 2024 -> window ends Aug 29 2024.
/// * Same-day trades always match.
///
/// Clamping is only ever applied to the earlier date. Aug 29, Aug 30 and Aug 31
/// of a non-leap year therefore all share the window end Feb 28.
#[derive(Clone, Copy, Debug, Default)]
pub struct SixCalendarMonths;

impl WindowRule for SixCalendarMonths {
    fn window_end(&self, earlier: Date) -> Date {
        earlier.add_months_clamped(6)
    }

    fn name(&self) -> String {
        "six-calendar-months".to_string()
    }
}

/// Alternative rule: the later trade is fewer than `days` days after the earlier one.
#[derive(Clone, Copy, Debug)]
pub struct DayCount {
    pub days: u32,
}

impl WindowRule for DayCount {
    fn window_end(&self, earlier: Date) -> Date {
        Date::from_day_number(earlier.day_number() + self.days as i64)
    }

    fn name(&self) -> String {
        format!("days:{}", self.days)
    }
}
