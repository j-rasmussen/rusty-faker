//! Faker's `date_time` provider for naive datetimes.
//!
//! Faker treats naive datetimes as UTC when converting to timestamps (`calendar.timegm`)
//! and uses the local wall-clock time as "now"; this module does the same. Timezone-aware
//! variants are handled by the Python layer.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Timelike};

use crate::error::{Error, Result, invalid};
use crate::generator::Generator;

const MICROS: i64 = 1_000_000;
/// 0001-01-01T00:00:00 as a Unix timestamp.
const MIN_TIMESTAMP: i64 = -62_135_596_800;
const MAX_YEAR: i32 = 9999;

/// Faker's `DateParseType`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DateTimeValue<'a> {
    /// Unix timestamp (Python `int`).
    Timestamp(i64),
    /// A naive datetime, interpreted as UTC.
    DateTime(NaiveDateTime),
    Date(NaiveDate),
    /// Offset from now, in seconds (Python `timedelta`).
    Delta(f64),
    /// `now`, `today`, or a relative string such as `-30y` or `+2d`.
    Str(&'a str),
}

/// Which "now" Faker reads. Naive calls use the local wall clock and treat it as UTC;
/// timezone-aware calls (`datetime.now(tzinfo)`) use the real instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// `datetime.now()`.
    NaiveLocal,
    /// `datetime.now(tzinfo)`.
    Utc,
}

fn now_naive() -> NaiveDateTime {
    Local::now().naive_local()
}

/// `timegm(datetime.now(tz).timetuple())` for the given clock.
pub fn now_timestamp(clock: Clock) -> i64 {
    match clock {
        Clock::NaiveLocal => floor_timestamp(now_naive()),
        Clock::Utc => chrono::Utc::now().timestamp(),
    }
}

fn floor_timestamp(dt: NaiveDateTime) -> i64 {
    dt.and_utc().timestamp()
}

/// Float seconds to microseconds, rounding like `timedelta(seconds=...)`.
fn seconds_to_micros(seconds: f64) -> Result<i64> {
    if !seconds.is_finite() {
        return Err(Error::Overflow("cannot convert float to timedelta".into()));
    }
    let whole = seconds.trunc();
    let frac_micros = ((seconds - whole) * 1e6).round_ties_even();
    let whole_micros = (whole as i128) * i128::from(MICROS);
    i64::try_from(whole_micros + frac_micros as i128)
        .map_err(|_| Error::Overflow("timedelta out of range".into()))
}

fn check_year(dt: NaiveDateTime) -> Result<NaiveDateTime> {
    if (1..=MAX_YEAR).contains(&dt.year()) {
        return Ok(dt);
    }
    Err(Error::Overflow("date value out of range".into()))
}

fn epoch() -> NaiveDateTime {
    chrono::DateTime::UNIX_EPOCH.naive_utc()
}

/// `datetime(1970, 1, 1) + timedelta(seconds=ts)`.
pub fn timestamp_to_naive(ts: f64) -> Result<NaiveDateTime> {
    let micros = seconds_to_micros(ts)?;
    let dt = epoch()
        .checked_add_signed(TimeDelta::microseconds(micros))
        .ok_or_else(|| Error::Overflow("date value out of range".into()))?;
    check_year(dt)
}

fn add_micros(dt: NaiveDateTime, micros: i64) -> Result<NaiveDateTime> {
    let dt = dt
        .checked_add_signed(TimeDelta::microseconds(micros))
        .ok_or_else(|| Error::Overflow("date value out of range".into()))?;
    check_year(dt)
}

/// Parses Faker's relative date strings (`+1y2M3w4d5h6m7s`, each part optional) into seconds.
/// Like `re.match`, only a prefix has to match.
pub fn parse_date_string(value: &str) -> Result<f64> {
    let mut rest = value;
    let mut found = false;
    let (mut days, mut seconds) = (0.0_f64, 0.0_f64);
    for (unit, scale_days, scale_seconds) in [
        ('y', 365.24, 0.0),
        ('M', 30.42, 0.0),
        ('w', 7.0, 0.0),
        ('d', 1.0, 0.0),
        ('h', 0.0, 3600.0),
        ('m', 0.0, 60.0),
        ('s', 0.0, 1.0),
    ] {
        let Some((amount, after)) = signed_amount(rest, unit) else {
            continue;
        };
        found = true;
        days += amount * scale_days;
        seconds += amount * scale_seconds;
        rest = after;
    }
    if !found {
        return Err(Error::Parse(format!("Can't parse date string `{value}`")));
    }
    Ok(days * 86_400.0 + seconds)
}

/// Matches `[+-]\d+<unit>` at the start of `s`.
fn signed_amount(s: &str, unit: char) -> Option<(f64, &str)> {
    let sign = match s.as_bytes().first()? {
        b'+' => 1.0,
        b'-' => -1.0,
        _ => return None,
    };
    let digits = s[1..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || !s[1 + digits..].starts_with(unit) {
        return None;
    }
    let amount: f64 = s[1..1 + digits].parse().ok()?;
    Some((sign * amount, &s[1 + digits + unit.len_utf8()..]))
}

/// Faker's `_parse_date_time`: a value to an integer timestamp, relative to the local clock.
pub fn parse_date_time(value: DateTimeValue<'_>) -> Result<i64> {
    parse_date_time_with(value, Clock::NaiveLocal)
}

/// Faker's `_parse_date_time(value, tzinfo)`: relative values resolve against `clock`.
pub fn parse_date_time_with(value: DateTimeValue<'_>, clock: Clock) -> Result<i64> {
    match value {
        DateTimeValue::Timestamp(ts) => Ok(ts),
        DateTimeValue::DateTime(dt) => Ok(floor_timestamp(dt)),
        DateTimeValue::Date(date) => Ok(floor_timestamp(date.and_time(NaiveTime::MIN))),
        DateTimeValue::Delta(seconds) => shift_now(seconds, clock),
        DateTimeValue::Str("now") => Ok(now_timestamp(clock)),
        DateTimeValue::Str(s) => shift_now(parse_date_string(s)?, clock),
    }
}

/// `timegm((datetime.now(tz) + timedelta(seconds=...)).timetuple())`.
fn shift_now(seconds: f64, clock: Clock) -> Result<i64> {
    let micros = seconds_to_micros(seconds)?;
    match clock {
        Clock::NaiveLocal => Ok(floor_timestamp(add_micros(now_naive(), micros)?)),
        // An aware `now` carries no sub-second part into the timestamp, so seconds add directly.
        Clock::Utc => Ok(now_timestamp(clock) + micros.div_euclid(MICROS)),
    }
}

/// Faker's `_parse_date`: a value to a date.
pub fn parse_date(value: DateTimeValue<'_>) -> Result<NaiveDate> {
    let today = Local::now().date_naive();
    let add_days = |days: i64| {
        today
            .checked_add_signed(TimeDelta::days(days))
            .ok_or_else(|| Error::Overflow("date value out of range".into()))
    };
    match value {
        DateTimeValue::DateTime(dt) => Ok(dt.date()),
        DateTimeValue::Date(date) => Ok(date),
        DateTimeValue::Delta(seconds) => add_days(whole_days(seconds)?),
        DateTimeValue::Str("today" | "now") => Ok(today),
        DateTimeValue::Str(s) => add_days(whole_days(parse_date_string(s)?)?),
        DateTimeValue::Timestamp(days) => add_days(days),
    }
}

/// `timedelta(seconds=...).days`: floor of whole days.
fn whole_days(seconds: f64) -> Result<i64> {
    Ok(seconds_to_micros(seconds)?.div_euclid(86_400 * MICROS))
}

fn change_year(date: NaiveDate, diff: i64) -> Result<NaiveDate> {
    let year =
        i32::try_from(i64::from(date.year()) + diff).map_err(|_| invalid("year out of range"))?;
    if year < 1 {
        return Err(invalid(format!("year {year} is out of range")));
    }
    date.with_year(year)
        .or_else(|| NaiveDate::from_ymd_opt(year, 3, 1))
        .ok_or_else(|| invalid(format!("year {year} is out of range")))
}

fn next_month_start(date: NaiveDate) -> Result<NaiveDate> {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| Error::Overflow("date value out of range".into()))
}

fn ymd(year: i32, month: u32, day: u32) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| Error::Overflow("date value out of range".into()))
}

/// A period relative to now for the `*_this_*` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Period {
    Century,
    Decade,
    Year,
    Month,
}

impl Generator {
    fn rand_seconds(&mut self, start: f64, end: f64) -> Result<f64> {
        if start > end {
            return Err(invalid(
                "empty range for _rand_seconds: start datetime must be before than end datetime",
            ));
        }
        Ok(self.uniform(start, end))
    }

    /// Timestamp between `start` (default: epoch) and `end` (default: now).
    pub fn unix_time(
        &mut self,
        end: Option<DateTimeValue<'_>>,
        start: Option<DateTimeValue<'_>>,
    ) -> Result<f64> {
        let start = start.map(parse_date_time).transpose()?.unwrap_or(0);
        let end = end.map_or_else(|| Ok(now_timestamp(Clock::NaiveLocal)), parse_date_time)?;
        self.rand_seconds(start as f64, end as f64)
    }

    /// Duration between now and `end` (default: now, i.e. zero).
    pub fn time_delta(&mut self, end: Option<DateTimeValue<'_>>) -> Result<TimeDelta> {
        let start = parse_date_time(DateTimeValue::Str("now"))?;
        let end = end.map_or_else(|| Ok(now_timestamp(Clock::NaiveLocal)), parse_date_time)?;
        let seconds = (end - start) as f64;
        let ts = self.rand_seconds(seconds.min(0.0), seconds.max(0.0))?;
        Ok(TimeDelta::microseconds(seconds_to_micros(ts)?))
    }

    /// Datetime between the epoch and `end` (default: now).
    pub fn date_time(&mut self, end: Option<DateTimeValue<'_>>) -> Result<NaiveDateTime> {
        let ts = self.unix_time(end, None)?;
        timestamp_to_naive(ts)
    }

    /// Datetime between 0001-01-01 (or `start`) and `end` (default: now).
    pub fn date_time_ad(
        &mut self,
        end: Option<DateTimeValue<'_>>,
        start: Option<DateTimeValue<'_>>,
    ) -> Result<NaiveDateTime> {
        let start = start
            .map(parse_date_time)
            .transpose()?
            .unwrap_or(MIN_TIMESTAMP);
        let end = end.map_or_else(|| Ok(now_timestamp(Clock::NaiveLocal)), parse_date_time)?;
        let ts = self.rand_seconds(start as f64, end as f64)?;
        timestamp_to_naive(ts)
    }

    /// ISO 8601 string; `timespec` is `auto`, `hours`, `minutes`, `seconds`, `milliseconds` or `microseconds`.
    pub fn iso8601(
        &mut self,
        end: Option<DateTimeValue<'_>>,
        sep: &str,
        timespec: &str,
    ) -> Result<String> {
        let dt = self.date_time(end)?;
        isoformat(dt, sep, timespec)
    }

    /// Date string formatted with a `strftime` pattern.
    pub fn date(&mut self, pattern: &str, end: Option<DateTimeValue<'_>>) -> Result<String> {
        let dt = self.date_time(end)?;
        strftime(&dt, pattern)
    }

    pub fn date_object(&mut self, end: Option<DateTimeValue<'_>>) -> Result<NaiveDate> {
        Ok(self.date_time(end)?.date())
    }

    /// Time string formatted with a `strftime` pattern.
    pub fn time(&mut self, pattern: &str, end: Option<DateTimeValue<'_>>) -> Result<String> {
        let dt = self.date_time(end)?;
        strftime(&dt.time(), pattern)
    }

    pub fn time_object(&mut self, end: Option<DateTimeValue<'_>>) -> Result<NaiveTime> {
        Ok(self.date_time(end)?.time())
    }

    /// Timestamp for `date_time_between`, resolving relative bounds against `clock`.
    pub fn date_time_between_ts(
        &mut self,
        start: DateTimeValue<'_>,
        end: DateTimeValue<'_>,
        clock: Clock,
    ) -> Result<f64> {
        let start = parse_date_time_with(start, clock)?;
        let end = parse_date_time_with(end, clock)?;
        if end - start <= 1 {
            return Ok(start as f64 + self.unit());
        }
        self.rand_seconds(start as f64, end as f64)
    }

    /// Timestamp for `date_time_between_dates`; absent bounds mean now on `clock`.
    pub fn date_time_between_dates_ts(
        &mut self,
        start: Option<DateTimeValue<'_>>,
        end: Option<DateTimeValue<'_>>,
        clock: Clock,
    ) -> Result<f64> {
        let now = || Ok(now_timestamp(clock));
        let start = start.map_or_else(now, parse_date_time)?;
        let end = end.map_or_else(now, parse_date_time)?;
        self.rand_seconds(start as f64, end as f64)
    }

    /// Datetime between two values (Faker defaults: `-30y` and `now`).
    pub fn date_time_between(
        &mut self,
        start: DateTimeValue<'_>,
        end: DateTimeValue<'_>,
    ) -> Result<NaiveDateTime> {
        let ts = self.date_time_between_ts(start, end, Clock::NaiveLocal)?;
        timestamp_to_naive(ts)
    }

    /// Date between two values (Faker defaults: `-30y` and `today`).
    pub fn date_between(
        &mut self,
        start: DateTimeValue<'_>,
        end: DateTimeValue<'_>,
    ) -> Result<NaiveDate> {
        let start = parse_date(start)?;
        let end = parse_date(end)?;
        self.date_between_dates(
            Some(DateTimeValue::Date(start)),
            Some(DateTimeValue::Date(end)),
        )
    }

    pub fn future_datetime(&mut self, end: DateTimeValue<'_>) -> Result<NaiveDateTime> {
        self.date_time_between(DateTimeValue::Str("+1s"), end)
    }

    pub fn future_date(&mut self, end: DateTimeValue<'_>) -> Result<NaiveDate> {
        self.date_between(DateTimeValue::Str("+1d"), end)
    }

    pub fn past_datetime(&mut self, start: DateTimeValue<'_>) -> Result<NaiveDateTime> {
        self.date_time_between(start, DateTimeValue::Str("-1s"))
    }

    pub fn past_date(&mut self, start: DateTimeValue<'_>) -> Result<NaiveDate> {
        self.date_between(start, DateTimeValue::Str("-1d"))
    }

    /// Datetime between two values, each defaulting to now.
    pub fn date_time_between_dates(
        &mut self,
        start: Option<DateTimeValue<'_>>,
        end: Option<DateTimeValue<'_>>,
    ) -> Result<NaiveDateTime> {
        let ts = self.date_time_between_dates_ts(start, end, Clock::NaiveLocal)?;
        timestamp_to_naive(ts)
    }

    pub fn date_between_dates(
        &mut self,
        start: Option<DateTimeValue<'_>>,
        end: Option<DateTimeValue<'_>>,
    ) -> Result<NaiveDate> {
        Ok(self.date_time_between_dates(start, end)?.date())
    }

    pub fn date_time_this_century(
        &mut self,
        before_now: bool,
        after_now: bool,
    ) -> Result<NaiveDateTime> {
        self.date_time_this(Period::Century, before_now, after_now)
    }

    pub fn date_time_this_decade(
        &mut self,
        before_now: bool,
        after_now: bool,
    ) -> Result<NaiveDateTime> {
        self.date_time_this(Period::Decade, before_now, after_now)
    }

    pub fn date_time_this_year(
        &mut self,
        before_now: bool,
        after_now: bool,
    ) -> Result<NaiveDateTime> {
        self.date_time_this(Period::Year, before_now, after_now)
    }

    pub fn date_time_this_month(
        &mut self,
        before_now: bool,
        after_now: bool,
    ) -> Result<NaiveDateTime> {
        self.date_time_this(Period::Month, before_now, after_now)
    }

    pub fn date_this_century(
        &mut self,
        before_today: bool,
        after_today: bool,
    ) -> Result<NaiveDate> {
        self.date_this(Period::Century, before_today, after_today)
    }

    pub fn date_this_decade(&mut self, before_today: bool, after_today: bool) -> Result<NaiveDate> {
        self.date_this(Period::Decade, before_today, after_today)
    }

    pub fn date_this_year(&mut self, before_today: bool, after_today: bool) -> Result<NaiveDate> {
        self.date_this(Period::Year, before_today, after_today)
    }

    pub fn date_this_month(&mut self, before_today: bool, after_today: bool) -> Result<NaiveDate> {
        self.date_this(Period::Month, before_today, after_today)
    }

    fn date_time_this(
        &mut self,
        period: Period,
        before: bool,
        after: bool,
    ) -> Result<NaiveDateTime> {
        let now = now_naive();
        let year = now.year();
        let (start, end) = match period {
            Period::Century => {
                let start = ymd(year - year.rem_euclid(100), 1, 1)?;
                (start, ymd((start.year() + 100).min(MAX_YEAR), 1, 1)?)
            }
            Period::Decade => {
                let start = ymd(year - year.rem_euclid(10), 1, 1)?;
                (start, ymd((start.year() + 10).min(MAX_YEAR), 1, 1)?)
            }
            Period::Year => (ymd(year, 1, 1)?, ymd(year + 1, 1, 1)?),
            Period::Month => {
                let start = ymd(year, now.month(), 1)?;
                (start, next_month_start(start)?)
            }
        };
        let start = DateTimeValue::DateTime(start.and_time(NaiveTime::MIN));
        let end = DateTimeValue::DateTime(end.and_time(NaiveTime::MIN));
        let now_value = DateTimeValue::DateTime(now);
        match (before, after) {
            (true, true) => self.date_time_between_dates(Some(start), Some(end)),
            (false, true) => self.date_time_between_dates(Some(now_value), Some(end)),
            (true, false) => self.date_time_between_dates(Some(start), Some(now_value)),
            (false, false) => Ok(now),
        }
    }

    fn date_this(&mut self, period: Period, before: bool, after: bool) -> Result<NaiveDate> {
        let today = Local::now().date_naive();
        let year = today.year();
        let (start, end) = match period {
            Period::Century => {
                let start = ymd(year - year.rem_euclid(100), 1, 1)?;
                (start, ymd(start.year() + 100, 1, 1)?)
            }
            Period::Decade => {
                let start = ymd(year - year.rem_euclid(10), 1, 1)?;
                (start, ymd(start.year() + 10, 1, 1)?)
            }
            Period::Year => (ymd(year, 1, 1)?, ymd(year + 1, 1, 1)?),
            Period::Month => {
                let start = ymd(year, today.month(), 1)?;
                (start, next_month_start(start)?)
            }
        };
        let (start, end, today_value) = (
            DateTimeValue::Date(start),
            DateTimeValue::Date(end),
            DateTimeValue::Date(today),
        );
        match (before, after) {
            (true, true) => self.date_between_dates(Some(start), Some(end)),
            (false, true) => self.date_between_dates(Some(today_value), Some(end)),
            (true, false) => self.date_between_dates(Some(start), Some(today_value)),
            (false, false) => Ok(today),
        }
    }

    pub fn am_pm(&mut self) -> Result<&'static str> {
        Ok(if self.date_time(None)?.hour() < 12 {
            "AM"
        } else {
            "PM"
        })
    }

    pub fn day_of_month(&mut self) -> Result<String> {
        Ok(format!("{:02}", self.date_time(None)?.day()))
    }

    pub fn day_of_week(&mut self) -> Result<String> {
        self.date("%A", None)
    }

    pub fn month(&mut self) -> Result<String> {
        Ok(format!("{:02}", self.date_time(None)?.month()))
    }

    pub fn month_name(&mut self) -> Result<String> {
        self.date("%B", None)
    }

    pub fn year(&mut self) -> Result<String> {
        Ok(format!("{:04}", self.date_time(None)?.year()))
    }

    /// Roman-numeral century such as `XIV`.
    pub fn century(&mut self) -> &'static str {
        self.pick(&self.locale_data().date_time.centuries)
    }

    /// IANA timezone name of a random country.
    pub fn timezone(&mut self) -> &'static str {
        let countries = self.locale_data().date_time.countries;
        let country = &countries[self.index(countries.len())];
        match country.timezones.len() {
            0 => "UTC",
            n => country.timezones[self.index(n)],
        }
    }

    /// Date of birth for someone aged `minimum_age` to `maximum_age` today.
    pub fn date_of_birth(&mut self, minimum_age: i64, maximum_age: i64) -> Result<NaiveDate> {
        if maximum_age < 0 {
            return Err(invalid(
                "maximum_age must be greater than or equal to zero.",
            ));
        }
        if minimum_age < 0 {
            return Err(invalid(
                "minimum_age must be greater than or equal to zero.",
            ));
        }
        if minimum_age > maximum_age {
            return Err(invalid(
                "minimum_age must be less than or equal to maximum_age.",
            ));
        }
        self.date_of_birth_from(Local::now().date_naive(), minimum_age, maximum_age)
    }

    /// `date_of_birth` relative to a given "today"; the bindings pass today in `tzinfo`.
    pub fn date_of_birth_from(
        &mut self,
        today: NaiveDate,
        minimum_age: i64,
        maximum_age: i64,
    ) -> Result<NaiveDate> {
        if maximum_age < 0 {
            return Err(invalid(
                "maximum_age must be greater than or equal to zero.",
            ));
        }
        if minimum_age < 0 {
            return Err(invalid(
                "minimum_age must be greater than or equal to zero.",
            ));
        }
        if minimum_age > maximum_age {
            return Err(invalid(
                "minimum_age must be less than or equal to maximum_age.",
            ));
        }
        let start = change_year(today, -(maximum_age + 1))?;
        let end = change_year(today, -minimum_age)?;
        let dob = self
            .date_time_ad(
                Some(DateTimeValue::Date(end)),
                Some(DateTimeValue::Date(start)),
            )?
            .date();
        if dob != start {
            return Ok(dob);
        }
        dob.succ_opt()
            .ok_or_else(|| Error::Overflow("date value out of range".into()))
    }
}

/// Formats with a `strftime` pattern, rejecting invalid specifiers instead of panicking.
fn strftime<T: FormatWith>(value: &T, pattern: &str) -> Result<String> {
    value.format_checked(pattern)
}

/// Formatting helper implemented for chrono's naive date/time types.
trait FormatWith {
    fn format_checked(&self, pattern: &str) -> Result<String>;
}

macro_rules! impl_format_with {
    ($($ty:ty),+) => {$(
        impl FormatWith for $ty {
            fn format_checked(&self, pattern: &str) -> Result<String> {
                use chrono::format::{Item, StrftimeItems};
                use std::fmt::Write as _;
                let items: Vec<Item<'_>> = StrftimeItems::new(pattern).collect();
                if items.iter().any(|item| matches!(item, Item::Error)) {
                    return Err(invalid(format!("invalid format string {pattern:?}")));
                }
                let mut out = String::with_capacity(pattern.len() + 16);
                write!(out, "{}", self.format_with_items(items.into_iter())).map_err(|_| invalid(format!("cannot format with {pattern:?}")))?;
                Ok(out)
            }
        }
    )+};
}

impl_format_with!(NaiveDateTime, NaiveTime, NaiveDate);

/// Python's `datetime.isoformat(sep, timespec)` for naive datetimes.
pub fn isoformat(dt: NaiveDateTime, sep: &str, timespec: &str) -> Result<String> {
    let date = dt.format("%Y-%m-%d");
    let (h, m, s, us) = (dt.hour(), dt.minute(), dt.second(), dt.nanosecond() / 1000);
    let time = match timespec {
        "auto" if us == 0 => format!("{h:02}:{m:02}:{s:02}"),
        "auto" | "microseconds" => format!("{h:02}:{m:02}:{s:02}.{us:06}"),
        "hours" => format!("{h:02}"),
        "minutes" => format!("{h:02}:{m:02}"),
        "seconds" => format!("{h:02}:{m:02}:{s:02}"),
        "milliseconds" => format!("{h:02}:{m:02}:{s:02}.{:03}", us / 1000),
        _ => return Err(invalid("Unknown timespec value")),
    };
    Ok(format!("{date}{sep}{time}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> Generator {
        Generator::seeded("en_US", 10).unwrap()
    }

    #[test]
    fn date_strings_parse_like_faker() {
        assert_eq!(parse_date_string("+1d").unwrap(), 86_400.0);
        assert_eq!(
            parse_date_string("-2w+3h").unwrap(),
            -2.0 * 604_800.0 + 3.0 * 3600.0
        );
        // Every part needs its own sign, so "3h" is ignored here.
        assert_eq!(parse_date_string("-2w3h").unwrap(), -2.0 * 604_800.0);
        assert_eq!(parse_date_string("+1y").unwrap(), 365.24 * 86_400.0);
        assert_eq!(parse_date_string("+5sjunk").unwrap(), 5.0);
        assert!(matches!(parse_date_string("5d"), Err(Error::Parse(_))));
        assert!(matches!(parse_date_string("soon"), Err(Error::Parse(_))));
    }

    #[test]
    fn ranges_hold() {
        let mut g = g();
        let now = floor_timestamp(now_naive());
        for _ in 0..500 {
            let dt = g.date_time(None).unwrap();
            assert!(floor_timestamp(dt) <= now && dt.year() >= 1970);
            let past = g.past_date(DateTimeValue::Str("-30d")).unwrap();
            assert!(past < Local::now().date_naive());
            let future = g.future_datetime(DateTimeValue::Str("+30d")).unwrap();
            assert!(future > now_naive() - TimeDelta::seconds(1));
            let dob = g.date_of_birth(18, 30).unwrap();
            let age = Local::now().date_naive().years_since(dob).unwrap();
            assert!((18..=30).contains(&age), "{dob}");
            let this_year = g.date_this_year(true, false).unwrap();
            assert_eq!(this_year.year(), Local::now().year());
        }
        // An end before the start yields `start + random()`, like Faker.
        let odd = g
            .date_time_between(DateTimeValue::Str("now"), DateTimeValue::Str("-1d"))
            .unwrap();
        assert!((floor_timestamp(odd) - now).abs() <= 2);
        assert!(
            g.date_time_between_dates(
                Some(DateTimeValue::Str("now")),
                Some(DateTimeValue::Str("-1d"))
            )
            .is_err()
        );
        assert!(g.date_of_birth(5, 1).is_err());
    }

    #[test]
    fn aware_clock_resolves_against_utc() {
        let mut g = g();
        let utc_now = chrono::Utc::now().timestamp();
        assert!(
            (parse_date_time_with(DateTimeValue::Str("now"), Clock::Utc).unwrap() - utc_now).abs()
                <= 1
        );
        let in_two_days = parse_date_time_with(DateTimeValue::Str("+2d"), Clock::Utc).unwrap();
        assert!((in_two_days - (utc_now + 2 * 86_400)).abs() <= 1);
        let ts = g
            .date_time_between_ts(
                DateTimeValue::Str("-1d"),
                DateTimeValue::Str("now"),
                Clock::Utc,
            )
            .unwrap();
        assert!((utc_now - 86_401) as f64 <= ts && ts <= utc_now as f64);
        let pinned = g
            .date_time_between_dates_ts(
                Some(DateTimeValue::Timestamp(42)),
                Some(DateTimeValue::Timestamp(42)),
                Clock::Utc,
            )
            .unwrap();
        assert_eq!(pinned, 42.0);
    }

    #[test]
    fn formatting() {
        let dt = NaiveDate::from_ymd_opt(2001, 2, 3)
            .unwrap()
            .and_hms_micro_opt(4, 5, 6, 7)
            .unwrap();
        assert_eq!(
            isoformat(dt, "T", "auto").unwrap(),
            "2001-02-03T04:05:06.000007"
        );
        assert_eq!(isoformat(dt, " ", "minutes").unwrap(), "2001-02-03 04:05");
        assert!(isoformat(dt, "T", "bogus").is_err());
        assert_eq!(
            dt.format_checked("%A %B %p").unwrap(),
            "Saturday February AM"
        );
        assert!(dt.format_checked("%Q").is_err());
        let mut g = g();
        assert_eq!(g.year().unwrap().len(), 4);
        assert!(["AM", "PM"].contains(&g.am_pm().unwrap()));
        assert!(g.timezone().contains('/'));
    }
}
