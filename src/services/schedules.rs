use std::path::PathBuf;

use chrono::{DateTime, Duration, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::{
    core::models::{DownloadOptions, JobKind},
    error::{RavynError, Result},
    services::{cron::CronExpression, imports::ImportDefaults, sniffer::SniffRequest},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMode {
    #[default]
    Download,
    SniffResources,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleOverlapPolicy {
    Skip,
    #[default]
    Queue,
    Replace,
    AllowParallel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMissedRunPolicy {
    Skip,
    #[default]
    RunOnce,
    CatchUp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduledSniffOptions {
    pub include_links: bool,
    pub include_images: bool,
    pub include_media: bool,
    pub include_scripts: bool,
    pub include_styles: bool,
    pub extensions: Vec<String>,
    pub only_new: bool,
    pub max_resources: Option<usize>,
    pub import_defaults: ImportDefaults,
}

impl Default for ScheduledSniffOptions {
    fn default() -> Self {
        Self {
            include_links: false,
            include_images: true,
            include_media: true,
            include_scripts: false,
            include_styles: false,
            extensions: Vec::new(),
            only_new: true,
            max_resources: None,
            import_defaults: ImportDefaults::default(),
        }
    }
}

impl ScheduledSniffOptions {
    pub fn request(&self, url: String) -> SniffRequest {
        SniffRequest {
            url,
            include_links: self.include_links,
            include_images: self.include_images,
            include_media: self.include_media,
            include_scripts: self.include_scripts,
            include_styles: self.include_styles,
            extensions: self.extensions.clone(),
            only_new: self.only_new,
            remember: false,
            max_resources: self.max_resources,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleInput {
    pub enabled: bool,
    pub source: String,
    pub kind: JobKind,
    pub destination: PathBuf,
    pub mode: ScheduleMode,
    pub automation: Option<ScheduledSniffOptions>,
    pub interval_seconds: Option<i64>,
    pub cron_expression: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub timezone_offset_minutes: i32,
    pub timezone_name: Option<String>,
    pub overlap_policy: ScheduleOverlapPolicy,
    pub missed_run_policy: ScheduleMissedRunPolicy,
    pub max_catch_up_runs: u16,
    pub paused_until: Option<DateTime<Utc>>,
    pub options: DownloadOptions,
}

impl Default for ScheduleInput {
    fn default() -> Self {
        Self {
            enabled: true,
            source: String::new(),
            kind: JobKind::Http,
            destination: PathBuf::new(),
            mode: ScheduleMode::Download,
            automation: None,
            interval_seconds: None,
            cron_expression: None,
            next_run_at: None,
            timezone_offset_minutes: 0,
            timezone_name: None,
            overlap_policy: ScheduleOverlapPolicy::Queue,
            missed_run_policy: ScheduleMissedRunPolicy::RunOnce,
            max_catch_up_runs: 1,
            paused_until: None,
            options: DownloadOptions::default(),
        }
    }
}

impl ScheduleInput {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
        if self.source.trim().is_empty() {
            return Err(RavynError::Invalid(
                "schedule source may not be empty".into(),
            ));
        }
        if self.destination.as_os_str().is_empty() {
            return Err(RavynError::Invalid(
                "schedule destination may not be empty".into(),
            ));
        }
        if !(-840..=840).contains(&self.timezone_offset_minutes) {
            return Err(RavynError::Invalid(
                "schedule timezone offset must be between -840 and 840 minutes".into(),
            ));
        }
        let timezone_name = self.timezone_name.as_deref().map(str::trim);
        if let Some(name) = timezone_name {
            if name.is_empty() || name.len() > 64 || name.parse::<Tz>().is_err() {
                return Err(RavynError::Invalid(
                    "schedule timezone_name must be a valid IANA time-zone name".into(),
                ));
            }
        }
        if !(1..=100).contains(&self.max_catch_up_runs) {
            return Err(RavynError::Invalid(
                "max_catch_up_runs must be between 1 and 100".into(),
            ));
        }
        match self.mode {
            ScheduleMode::Download if self.automation.is_some() => {
                return Err(RavynError::Invalid(
                    "download schedules may not contain sniff automation options".into(),
                ));
            }
            ScheduleMode::SniffResources if self.automation.is_none() => {
                return Err(RavynError::Invalid(
                    "sniff schedules require automation options".into(),
                ));
            }
            _ => {}
        }
        if self.interval_seconds.is_some() && self.cron_expression.is_some() {
            return Err(RavynError::Invalid(
                "a schedule may use an interval or cron expression, not both".into(),
            ));
        }
        let mut next = if let Some(seconds) = self.interval_seconds {
            if seconds <= 0 {
                return Err(RavynError::Invalid(
                    "schedule interval must be greater than zero".into(),
                ));
            }
            self.next_run_at
                .filter(|value| *value > now)
                .unwrap_or_else(|| now + Duration::seconds(seconds))
        } else if let Some(expression) = self.cron_expression.as_deref() {
            let cron = CronExpression::parse(expression)?;
            match self.next_run_at {
                Some(value) if value > now => value,
                _ => next_cron_after(&cron, now, self.timezone_offset_minutes, timezone_name)?,
            }
        } else {
            match self.next_run_at {
                Some(value) if value > now => value,
                _ => {
                    return Err(RavynError::Invalid(
                        "one-shot schedules require a future next_run_at value".into(),
                    ));
                }
            }
        };
        if let Some(paused_until) = self.paused_until {
            if paused_until > next {
                next = paused_until;
            }
        }
        Ok(next)
    }
}

pub fn next_cron_after(
    cron: &CronExpression,
    after_utc: DateTime<Utc>,
    timezone_offset_minutes: i32,
    timezone_name: Option<&str>,
) -> Result<DateTime<Utc>> {
    if let Some(name) = timezone_name {
        let timezone = name.parse::<Tz>().map_err(|_| {
            RavynError::Invalid("schedule timezone_name is not a valid IANA zone".into())
        })?;
        let local_after = after_utc.with_timezone(&timezone).naive_local().and_utc();
        let mut candidate = cron.next_after(local_after)?;
        // Ambiguous wall times run once at their earliest occurrence. A wall
        // time skipped by a DST jump is skipped, matching cron's calendar-day
        // semantics rather than silently shifting it by an hour.
        for _ in 0..=366 {
            match timezone.from_local_datetime(&candidate.naive_utc()) {
                chrono::LocalResult::Single(value) => return Ok(value.with_timezone(&Utc)),
                chrono::LocalResult::Ambiguous(earlier, later) => {
                    let value = earlier.min(later).with_timezone(&Utc);
                    if value > after_utc {
                        return Ok(value);
                    }
                }
                chrono::LocalResult::None => {}
            }
            candidate = cron.next_after(candidate)?;
        }
        return Err(RavynError::Invalid(
            "cron expression produced no valid local time within one year".into(),
        ));
    }
    let offset = Duration::minutes(i64::from(timezone_offset_minutes));
    let local_after = after_utc + offset;
    Ok(cron.next_after(local_after)? - offset)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn input() -> ScheduleInput {
        ScheduleInput {
            source: "https://example.com/file.zip".into(),
            destination: PathBuf::from("downloads"),
            ..ScheduleInput::default()
        }
    }

    #[test]
    fn rejects_past_one_shot_schedules() {
        let now = Utc::now();
        let mut schedule = input();
        schedule.next_run_at = Some(now - Duration::seconds(1));
        assert!(schedule.validate(now).is_err());
    }

    #[test]
    fn accepts_future_one_shot_schedules() {
        let now = Utc::now();
        let expected = now + Duration::minutes(5);
        let mut schedule = input();
        schedule.next_run_at = Some(expected);
        assert_eq!(schedule.validate(now).unwrap(), expected);
    }

    #[test]
    fn requires_sniff_options_for_sniff_schedules() {
        let now = Utc::now();
        let mut schedule = input();
        schedule.mode = ScheduleMode::SniffResources;
        schedule.next_run_at = Some(now + Duration::minutes(5));
        assert!(schedule.validate(now).is_err());
    }

    #[test]
    fn applies_fixed_timezone_offset_to_cron() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-12T06:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut schedule = input();
        schedule.cron_expression = Some("0 9 * * *".into());
        schedule.timezone_offset_minutes = 120;
        let next = schedule.validate(now).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-07-12T07:00:00+00:00");
    }

    #[test]
    fn skips_nonexistent_dst_wall_time() {
        let now = DateTime::parse_from_rfc3339("2026-03-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let cron = CronExpression::parse("30 2 * * *").unwrap();
        let next = next_cron_after(&cron, now, 0, Some("Europe/Rome")).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-03-30T00:30:00+00:00");
    }

    #[test]
    fn ambiguous_dst_wall_time_uses_earliest_occurrence() {
        let now = DateTime::parse_from_rfc3339("2026-10-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let cron = CronExpression::parse("30 2 * * *").unwrap();
        let next = next_cron_after(&cron, now, 0, Some("Europe/Rome")).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-10-25T00:30:00+00:00");
    }
}
