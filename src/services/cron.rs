use std::str::FromStr;

use chrono::{DateTime, Datelike, Days, TimeZone, Utc, Weekday};

use crate::error::{RavynError, Result};

/// Parsed UTC cron expression supporting five-field and six-field syntax.
///
/// Five fields are interpreted as `minute hour day-of-month month day-of-week`.
/// Six fields add seconds at the beginning. Ravyn intentionally evaluates cron
/// schedules in UTC so persisted jobs behave identically on every platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpression {
    seconds: Vec<u32>,
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
    day_of_month_any: bool,
    day_of_week_any: bool,
}

impl CronExpression {
    pub fn parse(expression: &str) -> Result<Self> {
        expression.parse()
    }

    /// Returns the first matching UTC instant strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>> {
        let start_date = after.date_naive();
        let max_days = 366_u64 * 8;

        for offset in 0..=max_days {
            let Some(date) = start_date.checked_add_days(Days::new(offset)) else {
                break;
            };
            if !self.months.contains(&date.month()) || !self.day_matches(date) {
                continue;
            }

            for &hour in &self.hours {
                for &minute in &self.minutes {
                    for &second in &self.seconds {
                        let Some(naive) = date.and_hms_opt(hour, minute, second) else {
                            continue;
                        };
                        let candidate = Utc.from_utc_datetime(&naive);
                        if candidate > after {
                            return Ok(candidate);
                        }
                    }
                }
            }
        }

        Err(RavynError::Invalid(
            "cron expression has no occurrence within the next eight years".into(),
        ))
    }

    fn day_matches(&self, date: chrono::NaiveDate) -> bool {
        let day_of_month = self.days_of_month.contains(&date.day());
        let day_of_week = self.days_of_week.contains(&weekday_number(date.weekday()));

        match (self.day_of_month_any, self.day_of_week_any) {
            (true, true) => true,
            (true, false) => day_of_week,
            (false, true) => day_of_month,
            (false, false) => day_of_month || day_of_week,
        }
    }
}

impl FromStr for CronExpression {
    type Err = RavynError;

    fn from_str(expression: &str) -> Result<Self> {
        let fields = expression.split_whitespace().collect::<Vec<_>>();
        let (seconds, minute_index) = match fields.len() {
            5 => (vec![0], 0),
            6 => (parse_field(fields[0], 0, 59, FieldNames::None)?, 1),
            _ => {
                return Err(RavynError::Invalid(
                    "cron expressions must contain five or six fields".into(),
                ));
            }
        };

        let day_of_month_any = fields[minute_index + 2] == "*";
        let day_of_week_any = fields[minute_index + 4] == "*";
        Ok(Self {
            seconds,
            minutes: parse_field(fields[minute_index], 0, 59, FieldNames::None)?,
            hours: parse_field(fields[minute_index + 1], 0, 23, FieldNames::None)?,
            days_of_month: parse_field(fields[minute_index + 2], 1, 31, FieldNames::None)?,
            months: parse_field(fields[minute_index + 3], 1, 12, FieldNames::Month)?,
            days_of_week: parse_field(fields[minute_index + 4], 0, 7, FieldNames::Weekday)?
                .into_iter()
                .map(|value| if value == 7 { 0 } else { value })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            day_of_month_any,
            day_of_week_any,
        })
    }
}

#[derive(Clone, Copy)]
enum FieldNames {
    None,
    Month,
    Weekday,
}

fn parse_field(input: &str, min: u32, max: u32, names: FieldNames) -> Result<Vec<u32>> {
    let mut values = std::collections::BTreeSet::new();
    for part in input.split(',') {
        if part.is_empty() {
            return Err(RavynError::Invalid("empty cron field component".into()));
        }
        let (range_part, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step = step
                    .parse::<u32>()
                    .map_err(|_| RavynError::Invalid(format!("invalid cron step `{step}`")))?;
                if step == 0 {
                    return Err(RavynError::Invalid(
                        "cron steps must be greater than zero".into(),
                    ));
                }
                (range, step)
            }
            None => (part, 1),
        };

        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((start, end)) = range_part.split_once('-') {
            (
                parse_value(start, names, min, max)?,
                parse_value(end, names, min, max)?,
            )
        } else {
            let value = parse_value(range_part, names, min, max)?;
            (value, if step > 1 { max } else { value })
        };

        if start > end {
            return Err(RavynError::Invalid(format!(
                "cron range starts after it ends: {range_part}"
            )));
        }
        let mut value = start;
        while value <= end {
            values.insert(value);
            let Some(next) = value.checked_add(step) else {
                break;
            };
            value = next;
        }
    }

    if values.is_empty() {
        return Err(RavynError::Invalid("cron field matched no values".into()));
    }
    Ok(values.into_iter().collect())
}

fn parse_value(input: &str, names: FieldNames, min: u32, max: u32) -> Result<u32> {
    let upper = input.to_ascii_uppercase();
    let named = match names {
        FieldNames::None => None,
        FieldNames::Month => match upper.as_str() {
            "JAN" => Some(1),
            "FEB" => Some(2),
            "MAR" => Some(3),
            "APR" => Some(4),
            "MAY" => Some(5),
            "JUN" => Some(6),
            "JUL" => Some(7),
            "AUG" => Some(8),
            "SEP" => Some(9),
            "OCT" => Some(10),
            "NOV" => Some(11),
            "DEC" => Some(12),
            _ => None,
        },
        FieldNames::Weekday => match upper.as_str() {
            "SUN" => Some(0),
            "MON" => Some(1),
            "TUE" => Some(2),
            "WED" => Some(3),
            "THU" => Some(4),
            "FRI" => Some(5),
            "SAT" => Some(6),
            _ => None,
        },
    };
    let value = match named {
        Some(value) => value,
        None => input
            .parse::<u32>()
            .map_err(|_| RavynError::Invalid(format!("invalid cron value `{input}`")))?,
    };
    if !(min..=max).contains(&value) {
        return Err(RavynError::Invalid(format!(
            "cron value {value} is outside {min}..={max}"
        )));
    }
    Ok(value)
}

fn weekday_number(day: Weekday) -> u32 {
    match day {
        Weekday::Sun => 0,
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Timelike};

    use super::*;

    #[test]
    fn supports_five_field_expressions() {
        let expression = CronExpression::parse("*/15 * * * *").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 7, 12, 10, 7, 0).unwrap();
        assert_eq!(
            expression.next_after(after).unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 12, 10, 15, 0).unwrap()
        );
    }

    #[test]
    fn supports_names_and_seconds() {
        let expression = CronExpression::parse("30 0 9 * JAN MON").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let next = expression.next_after(after).unwrap();
        assert_eq!(next.weekday(), Weekday::Mon);
        assert_eq!((next.hour(), next.minute(), next.second()), (9, 0, 30));
    }

    #[test]
    fn rejects_zero_steps() {
        assert!(CronExpression::parse("*/0 * * * *").is_err());
    }

    #[test]
    fn applies_steps_from_an_explicit_start() {
        let expression = CronExpression::parse("5/20 * * * *").unwrap();
        let after = Utc.with_ymd_and_hms(2026, 7, 12, 10, 6, 0).unwrap();
        assert_eq!(
            expression.next_after(after).unwrap(),
            Utc.with_ymd_and_hms(2026, 7, 12, 10, 25, 0).unwrap()
        );
    }
}
