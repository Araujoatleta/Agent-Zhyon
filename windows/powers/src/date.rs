//! Plain calendar dates (no time zone, no clock: the app passes today in), and the Brazilian calendar the money runs
//! on: the national holidays, Carnival, Good Friday and Corpus Christi (from Easter), B3's own closed days (24 and 31
//! December), business days, and the last business day of a month (a DARF is due then).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Date {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

/// Days from 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = (if m <= 2 { y - 1 } else { y }) as i64;
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

impl Date {
    /// A valid date, or None (31 February, month 13).
    pub fn new(y: i32, m: u32, d: u32) -> Option<Date> {
        ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) && (1900..=2200).contains(&y)).then_some(Date { y, m, d })
    }

    /// "2026-10-21", "21/10/2026", "20261021", "21/10/26", "2026-10-21T18:00:00", "21/10/2026 18:00". None for
    /// anything else.
    pub fn parse(text: &str) -> Option<Date> {
        let s = text.trim();
        let s = s.split(['T', ' ']).next().unwrap_or("");
        if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
            return Date::new(s[..4].parse().ok()?, s[4..6].parse().ok()?, s[6..].parse().ok()?);
        }
        let parts: Vec<&str> = s.split(['-', '/', '.']).collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        let n: Vec<i64> = parts.iter().filter_map(|p| p.parse().ok()).collect();
        if n.len() != 3 {
            return None;
        }
        if parts[0].len() == 4 {
            Date::new(n[0] as i32, n[1] as u32, n[2] as u32)
        } else {
            let y = if parts[2].len() == 2 { 2000 + n[2] } else { n[2] };
            Date::new(y as i32, n[1] as u32, n[0] as u32)
        }
    }

    /// Days since 1970-01-01.
    pub fn days(self) -> i64 {
        days_from_civil(self.y, self.m, self.d)
    }

    pub fn from_days(days: i64) -> Date {
        let (y, m, d) = civil_from_days(days);
        Date { y, m, d }
    }

    pub fn add_days(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }

    /// The same day `n` months later (or earlier), on the month's last day when it has no such day.
    pub fn add_months(self, n: i32) -> Date {
        let total = self.y * 12 + (self.m as i32 - 1) + n;
        let y = total.div_euclid(12);
        let m = total.rem_euclid(12) as u32 + 1;
        Date { y, m, d: self.d.min(days_in_month(y, m)) }
    }

    /// Days from `self` to `other` (negative when other is before).
    pub fn until(self, other: Date) -> i64 {
        other.days() - self.days()
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> u32 {
        ((self.days() + 3).rem_euclid(7)) as u32
    }

    pub fn is_weekend(self) -> bool {
        self.weekday() >= 5
    }

    /// "2026-10".
    pub fn month_key(self) -> String {
        format!("{:04}-{:02}", self.y, self.m)
    }

    /// "21/10/2026".
    pub fn br(self) -> String {
        format!("{:02}/{:02}/{:04}", self.d, self.m, self.y)
    }

    /// "21/10".
    pub fn short(self) -> String {
        format!("{:02}/{:02}", self.d, self.m)
    }

    pub fn first_of_month(self) -> Date {
        Date { d: 1, ..self }
    }

    pub fn last_of_month(self) -> Date {
        Date { d: days_in_month(self.y, self.m), ..self }
    }

    /// Whole months from `self` to `later` (negative when it is before), counting a month only once its day is reached.
    pub fn months_until(self, later: Date) -> i32 {
        let mut n = (later.y - self.y) * 12 + later.m as i32 - self.m as i32;
        if n > 0 && later.d < self.d {
            n -= 1;
        } else if n < 0 && later.d > self.d {
            n += 1;
        }
        n
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Date, D::Error> {
        let text = String::deserialize(d)?;
        Date::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("not a date: {text}")))
    }
}

/// Easter Sunday (the Anonymous Gregorian algorithm).
pub fn easter(y: i32) -> Date {
    let a = y % 19;
    let b = y / 100;
    let c = y % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    Date { y, m: month as u32, d: day as u32 }
}

/// The days the banks close nationally (DARF due dates move past them), and with `exchange` the ones B3 closes too.
pub fn holidays(y: i32, exchange: bool) -> Vec<Date> {
    let e = easter(y);
    let mut out = vec![
        Date { y, m: 1, d: 1 },
        e.add_days(-48), // Carnival Monday
        e.add_days(-47), // Carnival Tuesday
        e.add_days(-2),  // Good Friday
        Date { y, m: 4, d: 21 },
        Date { y, m: 5, d: 1 },
        e.add_days(60), // Corpus Christi
        Date { y, m: 9, d: 7 },
        Date { y, m: 10, d: 12 },
        Date { y, m: 11, d: 2 },
        Date { y, m: 11, d: 15 },
        Date { y, m: 12, d: 25 },
    ];
    if y >= 2024 {
        out.push(Date { y, m: 11, d: 20 }); // Black Consciousness Day, national since 2024
    }
    if exchange {
        out.push(Date { y, m: 12, d: 24 });
        out.push(Date { y, m: 12, d: 31 });
    }
    out.sort();
    out
}

/// A day the banks (or, with `exchange`, B3) open.
pub fn is_business_day(date: Date, exchange: bool) -> bool {
    !date.is_weekend() && !holidays(date.y, exchange).contains(&date)
}

/// The business day before `date` (B3's calendar): yesterday's quotes when today is Monday are Friday's.
pub fn previous_business_day(date: Date, exchange: bool) -> Date {
    let mut d = date.add_days(-1);
    while !is_business_day(d, exchange) {
        d = d.add_days(-1);
    }
    d
}

pub fn next_business_day(date: Date, exchange: bool) -> Date {
    let mut d = date.add_days(1);
    while !is_business_day(d, exchange) {
        d = d.add_days(1);
    }
    d
}

/// The last business day of a month: when the DARF of the month before is due.
pub fn last_business_day(y: i32, m: u32) -> Date {
    let mut d = Date { y, m, d: days_in_month(y, m) };
    while !is_business_day(d, false) {
        d = d.add_days(-1);
    }
    d
}

/// Business days between two dates, `from` excluded and `to` included (B3's calendar).
pub fn business_days_between(from: Date, to: Date) -> i64 {
    if to <= from {
        return 0;
    }
    let mut n = 0;
    let mut d = from;
    while d < to {
        d = d.add_days(1);
        if is_business_day(d, true) {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    #[test]
    fn dates_read_every_way_they_are_written() {
        assert_eq!(d("2026-10-21"), Date { y: 2026, m: 10, d: 21 });
        assert_eq!(d("21/10/2026"), d("2026-10-21"));
        assert_eq!(d("20261021"), d("2026-10-21"));
        assert_eq!(d("21/10/26"), d("2026-10-21"));
        assert_eq!(d("2026-10-21T18:00:00"), d("2026-10-21"));
        assert_eq!(d("21/10/2026 18:00"), d("2026-10-21"));
        assert!(Date::parse("31/02/2026").is_none());
        assert!(Date::parse("2026-13-01").is_none());
        assert!(Date::parse("seis").is_none());
        assert!(Date::parse("").is_none());
        assert_eq!(serde_json::to_string(&d("2026-01-05")).unwrap(), "\"2026-01-05\"");
        assert_eq!(serde_json::from_str::<Date>("\"05/01/2026\"").unwrap(), d("2026-01-05"));
    }

    #[test]
    fn arithmetic_and_weekdays() {
        assert_eq!(Date::from_days(0), d("1970-01-01"));
        assert_eq!(d("2026-10-21").add_days(11), d("2026-11-01"));
        assert_eq!(d("2026-03-01").add_days(-1), d("2026-02-28"));
        assert_eq!(d("2024-03-01").add_days(-1), d("2024-02-29"));
        assert_eq!(d("2026-01-31").add_months(1), d("2026-02-28"));
        assert_eq!(d("2026-11-15").add_months(3), d("2027-02-15"));
        assert_eq!(d("2026-01-15").add_months(-2), d("2025-11-15"));
        assert_eq!(d("2026-10-21").weekday(), 2, "a Wednesday");
        assert!(d("2026-10-24").is_weekend());
        assert_eq!(d("2026-10-01").until(d("2026-10-21")), 20);
        assert_eq!(d("2026-01-31").months_until(d("2026-03-30")), 1);
        assert_eq!(d("2026-01-15").months_until(d("2026-04-15")), 3);
        assert_eq!(d("2026-10-21").br(), "21/10/2026");
        assert_eq!(d("2026-10-21").month_key(), "2026-10");
    }

    #[test]
    fn the_brazilian_calendar() {
        assert_eq!(easter(2026), d("2026-04-05"));
        assert_eq!(easter(2027), d("2027-03-28"));
        let h = holidays(2026, true);
        assert!(h.contains(&d("2026-02-16")) && h.contains(&d("2026-02-17")), "Carnival");
        assert!(h.contains(&d("2026-04-03")), "Good Friday");
        assert!(h.contains(&d("2026-06-04")), "Corpus Christi");
        assert!(h.contains(&d("2026-11-20")));
        assert!(!is_business_day(d("2026-12-24"), true) && is_business_day(d("2026-12-24"), false));
        assert_eq!(previous_business_day(d("2026-10-19"), true), d("2026-10-16"), "Monday's day before is Friday");
        assert_eq!(previous_business_day(d("2026-11-03"), true), d("2026-10-30"), "All Souls' Day and the weekend");
        assert_eq!(next_business_day(d("2026-04-02"), true), d("2026-04-06"));
        assert_eq!(last_business_day(2026, 10), d("2026-10-30"));
        assert_eq!(last_business_day(2026, 5), d("2026-05-29"), "the 31st is a Sunday");
        assert_eq!(business_days_between(d("2026-10-16"), d("2026-10-21")), 3);
    }
}
