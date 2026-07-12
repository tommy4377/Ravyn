//! Durable schedule records, atomic claims, and execution history.

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::{CreateJob, DownloadOptions, JobKind},
    error::{RavynError, Result},
    services::{
        cron::CronExpression,
        schedules::{
            ScheduleMissedRunPolicy, ScheduleMode, ScheduleOverlapPolicy, ScheduledSniffOptions,
            next_cron_after,
        },
    },
    storage::{
        Repository,
        jobs::{parse_kind, row_uuid},
    },
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScheduleExecutionRecord {
    pub id: Uuid,
    pub schedule_id: Uuid,
    pub intended_run_at: DateTime<Utc>,
    pub state: String,
    pub summary: Option<serde_json::Value>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct ScheduleClaim {
    pub schedule: Schedule,
    pub token: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Schedule {
    pub id: Uuid,
    pub enabled: bool,
    pub source: String,
    pub kind: JobKind,
    pub destination: PathBuf,
    pub mode: ScheduleMode,
    pub automation: Option<ScheduledSniffOptions>,
    pub interval_seconds: Option<i64>,
    pub cron_expression: Option<String>,
    pub next_run_at: DateTime<Utc>,
    pub timezone_offset_minutes: i32,
    pub timezone_name: Option<String>,
    pub overlap_policy: ScheduleOverlapPolicy,
    pub missed_run_policy: ScheduleMissedRunPolicy,
    pub max_catch_up_runs: u16,
    pub catch_up_runs: u16,
    pub paused_until: Option<DateTime<Utc>>,
    pub options: DownloadOptions,
    pub last_run_at: Option<DateTime<Utc>>,
    pub failure_count: i64,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Schedule {
    pub(crate) fn from_row(row: SqliteRow) -> Result<Self> {
        Ok(Self {
            id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
                .map_err(|e| RavynError::Internal(e.to_string()))?,
            enabled: row.try_get::<i64, _>("enabled")? != 0,
            source: row.try_get("source")?,
            kind: parse_kind(&row.try_get::<String, _>("kind")?)?,
            destination: PathBuf::from(row.try_get::<String, _>("destination")?),
            mode: parse_schedule_mode(&row.try_get::<String, _>("mode")?)?,
            automation: serde_json::from_str::<Option<ScheduledSniffOptions>>(
                &row.try_get::<String, _>("automation_json")?,
            )?,
            interval_seconds: row.try_get("interval_seconds")?,
            cron_expression: row.try_get("cron_expression")?,
            next_run_at: row.try_get("next_run_at")?,
            timezone_offset_minutes: row.try_get("timezone_offset_minutes")?,
            timezone_name: row.try_get("timezone_name")?,
            overlap_policy: parse_schedule_overlap_policy(
                &row.try_get::<String, _>("overlap_policy")?,
            )?,
            missed_run_policy: parse_schedule_missed_run_policy(
                &row.try_get::<String, _>("missed_run_policy")?,
            )?,
            max_catch_up_runs: u16::try_from(row.try_get::<i64, _>("max_catch_up_runs")?).map_err(
                |_| RavynError::Internal("invalid max_catch_up_runs in database".into()),
            )?,
            catch_up_runs: u16::try_from(row.try_get::<i64, _>("catch_up_runs")?)
                .map_err(|_| RavynError::Internal("invalid catch_up_runs in database".into()))?,
            paused_until: row.try_get("paused_until")?,
            options: serde_json::from_str(&row.try_get::<String, _>("options_json")?)?,
            last_run_at: row.try_get("last_run_at")?,
            failure_count: row.try_get("failure_count")?,
            last_error: row.try_get("last_error")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
    pub fn to_create_job(&self) -> CreateJob {
        CreateJob {
            kind: self.kind,
            source: self.source.clone(),
            destination: Some(self.destination.clone()),
            filename: None,
            priority: 0,
            speed_limit_bps: None,
            expected_sha256: None,
            duplicate_policy: Default::default(),
            options: self.options.clone(),
        }
    }
}

impl Repository {
    pub async fn list_schedule_executions(
        &self,
        schedule_id: Uuid,
        limit: usize,
    ) -> Result<Vec<ScheduleExecutionRecord>> {
        self.get_schedule(schedule_id).await?;
        sqlx::query("SELECT id,schedule_id,intended_run_at,state,summary_json,error,started_at,completed_at FROM schedule_executions WHERE schedule_id=? ORDER BY started_at DESC,id DESC LIMIT ?")
            .bind(schedule_id.to_string())
            .bind(i64::try_from(limit.clamp(1, 200)).map_err(|_| RavynError::Invalid("execution page limit is invalid".into()))?)
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_schedule_execution)
            .collect()
    }

    pub async fn get_schedule_execution(&self, id: Uuid) -> Result<ScheduleExecutionRecord> {
        sqlx::query("SELECT id,schedule_id,intended_run_at,state,summary_json,error,started_at,completed_at FROM schedule_executions WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_schedule_execution)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("schedule execution {id}")))
    }

    pub async fn cancel_schedule_execution(&self, id: Uuid) -> Result<ScheduleExecutionRecord> {
        let changed = sqlx::query("UPDATE schedule_executions SET state='cancelled',cancellation_requested=1,completed_at=?,error=COALESCE(error,'cancelled by API') WHERE id=? AND state='running'")
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            let existing = self.get_schedule_execution(id).await?;
            return Err(RavynError::Conflict(format!(
                "schedule execution is already {}",
                existing.state
            )));
        }
        self.get_schedule_execution(id).await
    }

    pub async fn set_schedule_enabled(&self, id: Uuid, enabled: bool) -> Result<Schedule> {
        let changed = sqlx::query("UPDATE schedules SET enabled=?,updated_at=? WHERE id=?")
            .bind(if enabled { 1_i64 } else { 0_i64 })
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("schedule {id}")));
        }
        self.get_schedule(id).await
    }
    pub async fn claim_due_schedule(
        &self,
        lease: std::time::Duration,
    ) -> Result<Option<ScheduleClaim>> {
        let started = std::time::Instant::now();
        let now = Utc::now();
        let lease_until = now
            + Duration::from_std(lease).map_err(|error| RavynError::Internal(error.to_string()))?;
        let token = Uuid::new_v4().to_string();
        let sql = "UPDATE schedules SET claim_token=?,claim_until=?,updated_at=? WHERE id=(SELECT s.id FROM schedules s WHERE s.enabled=1 AND s.next_run_at<=? AND (s.paused_until IS NULL OR s.paused_until<=?) AND (s.claim_until IS NULL OR s.claim_until<?) AND (s.overlap_policy IN ('allow_parallel','replace') OR NOT EXISTS(SELECT 1 FROM schedule_executions se WHERE se.schedule_id=s.id AND se.state='running')) ORDER BY s.next_run_at ASC LIMIT 1) RETURNING id,enabled,source,kind,destination,mode,automation_json,interval_seconds,cron_expression,next_run_at,timezone_offset_minutes,timezone_name,overlap_policy,missed_run_policy,max_catch_up_runs,catch_up_runs,paused_until,options_json,last_run_at,failure_count,last_error,created_at,updated_at";
        let row = sqlx::query(sql)
            .bind(&token)
            .bind(lease_until)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(now)
            .fetch_optional(self.pool())
            .await;
        self.observe_query("claim_due_schedule", started);
        let Some(schedule) = row?.map(Schedule::from_row).transpose()? else {
            return Ok(None);
        };
        if schedule.overlap_policy == ScheduleOverlapPolicy::Replace {
            sqlx::query("UPDATE schedule_executions SET cancellation_requested=1,error=COALESCE(error,'replaced by a newer overlapping execution') WHERE schedule_id=? AND state='running'")
                .bind(schedule.id.to_string())
                .execute(self.pool())
                .await?;
        }
        Ok(Some(ScheduleClaim { schedule, token }))
    }

    pub async fn complete_schedule_claim(&self, claim: &ScheduleClaim) -> Result<()> {
        let now = Utc::now();
        let (next_run, catch_up_runs) = match claim.schedule.missed_run_policy {
            ScheduleMissedRunPolicy::CatchUp => {
                let candidate = next_schedule_time(&claim.schedule, claim.schedule.next_run_at)?;
                match candidate {
                    Some(candidate)
                        if candidate <= now
                            && claim.schedule.catch_up_runs.saturating_add(1)
                                < claim.schedule.max_catch_up_runs =>
                    {
                        (
                            Some(candidate),
                            claim.schedule.catch_up_runs.saturating_add(1),
                        )
                    }
                    _ => (next_schedule_time(&claim.schedule, now)?, 0),
                }
            }
            ScheduleMissedRunPolicy::Skip | ScheduleMissedRunPolicy::RunOnce => {
                (next_schedule_time(&claim.schedule, now)?, 0)
            }
        };
        let changed = sqlx::query("UPDATE schedules SET enabled=?,next_run_at=COALESCE(?,next_run_at),catch_up_runs=?,last_run_at=?,failure_count=0,last_error=NULL,claim_token=NULL,claim_until=NULL,updated_at=? WHERE id=? AND claim_token=?")
            .bind(if next_run.is_some() { 1_i64 } else { 0_i64 })
            .bind(next_run)
            .bind(i64::from(catch_up_runs))
            .bind(now)
            .bind(now)
            .bind(claim.schedule.id.to_string())
            .bind(&claim.token)
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::Conflict(
                "schedule lease was lost before completion".into(),
            ));
        }
        Ok(())
    }

    pub async fn renew_schedule_claim(
        &self,
        claim: &ScheduleClaim,
        lease: std::time::Duration,
    ) -> Result<()> {
        let now = Utc::now();
        let lease_until = now
            + Duration::from_std(lease).map_err(|error| RavynError::Internal(error.to_string()))?;
        let changed = sqlx::query("UPDATE schedules SET claim_until=?,updated_at=? WHERE id=? AND claim_token=? AND claim_until>=?")
            .bind(lease_until)
            .bind(now)
            .bind(claim.schedule.id.to_string())
            .bind(&claim.token)
            .bind(now)
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::Conflict("schedule lease was lost".into()));
        }
        Ok(())
    }

    pub async fn begin_schedule_execution(&self, claim: &ScheduleClaim) -> Result<Option<Uuid>> {
        let id = Uuid::new_v4();
        let changed = sqlx::query("INSERT OR IGNORE INTO schedule_executions(id,schedule_id,intended_run_at,claim_token,state,started_at) VALUES(?,?,?,?, 'running', ?)")
            .bind(id.to_string())
            .bind(claim.schedule.id.to_string())
            .bind(claim.schedule.next_run_at)
            .bind(&claim.token)
            .bind(Utc::now())
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok((changed == 1).then_some(id))
    }

    pub async fn finish_schedule_execution(
        &self,
        execution_id: Uuid,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        if !matches!(state, "completed" | "failed" | "lease_lost" | "cancelled") {
            return Err(RavynError::Invalid(
                "invalid schedule execution state".into(),
            ));
        }
        sqlx::query("UPDATE schedule_executions SET state=?,error=?,completed_at=? WHERE id=?")
            .bind(state)
            .bind(error)
            .bind(Utc::now())
            .bind(execution_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn schedule_execution_cancellation_requested(
        &self,
        execution_id: Uuid,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT cancellation_requested FROM schedule_executions WHERE id=?",
        )
        .bind(execution_id.to_string())
        .fetch_optional(self.pool())
        .await?
        .is_some_and(|value| value != 0))
    }

    pub async fn advance_skipped_schedules(&self) -> Result<u64> {
        let now = Utc::now();
        let sql = "SELECT id,enabled,source,kind,destination,mode,automation_json,interval_seconds,cron_expression,next_run_at,timezone_offset_minutes,timezone_name,overlap_policy,missed_run_policy,max_catch_up_runs,catch_up_runs,paused_until,options_json,last_run_at,failure_count,last_error,created_at,updated_at FROM schedules s WHERE s.enabled=1 AND s.next_run_at<=? AND s.claim_token IS NULL AND ((s.missed_run_policy='skip') OR (s.overlap_policy='skip' AND EXISTS(SELECT 1 FROM schedule_executions se WHERE se.schedule_id=s.id AND se.state='running'))) ORDER BY s.next_run_at LIMIT 256";
        let schedules = sqlx::query(sql)
            .bind(now)
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(Schedule::from_row)
            .collect::<Result<Vec<_>>>()?;
        let mut advanced = 0_u64;
        for schedule in schedules {
            let overlapping = schedule.overlap_policy == ScheduleOverlapPolicy::Skip
                && sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM schedule_executions WHERE schedule_id=? AND state='running'",
                )
                .bind(schedule.id.to_string())
                .fetch_one(self.pool())
                .await?
                    > 0;
            // Scheduler ticks are intentionally coarse, so a run that is only
            // a few seconds late is still a normal due run. The skip policy is
            // applied only after a bounded grace period, which distinguishes a
            // genuine restart/downtime miss from ordinary scheduling jitter.
            let genuinely_missed = schedule.missed_run_policy == ScheduleMissedRunPolicy::Skip
                && schedule.next_run_at + Duration::seconds(30) < now;
            if !overlapping && !genuinely_missed {
                continue;
            }
            let next_run = next_schedule_time(&schedule, now)?;
            let reason = if overlapping {
                "scheduled run skipped because a previous execution is still running"
            } else {
                "missed scheduled run skipped by policy"
            };
            let changed = sqlx::query("UPDATE schedules SET enabled=?,next_run_at=COALESCE(?,next_run_at),catch_up_runs=0,last_error=?,updated_at=? WHERE id=? AND claim_token IS NULL")
                .bind(if next_run.is_some() { 1_i64 } else { 0_i64 })
                .bind(next_run)
                .bind(reason)
                .bind(now)
                .bind(schedule.id.to_string())
                .execute(self.pool())
                .await?
                .rows_affected();
            advanced = advanced.saturating_add(changed);
        }
        Ok(advanced)
    }

    pub async fn release_schedule_claim(&self, claim: &ScheduleClaim, error: &str) -> Result<()> {
        let now = Utc::now();
        let exponent = u32::try_from(claim.schedule.failure_count.max(0))
            .unwrap_or_default()
            .min(8);
        let delay_seconds = 15_i64.saturating_mul(1_i64 << exponent).min(3_600);
        sqlx::query("UPDATE schedules SET failure_count=failure_count+1,last_error=?,next_run_at=?,claim_token=NULL,claim_until=NULL,updated_at=? WHERE id=? AND claim_token=?")
            .bind(error)
            .bind(now + Duration::seconds(delay_seconds))
            .bind(now)
            .bind(claim.schedule.id.to_string())
            .bind(&claim.token)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

fn next_schedule_time(schedule: &Schedule, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
    if let Some(seconds) = schedule.interval_seconds {
        if seconds <= 0 {
            return Err(RavynError::Invalid(
                "schedule interval must be greater than zero".into(),
            ));
        }
        return Ok(Some(after + Duration::seconds(seconds)));
    }
    if let Some(expression) = schedule.cron_expression.as_deref() {
        let cron = CronExpression::parse(expression)?;
        return Ok(Some(next_cron_after(
            &cron,
            after,
            schedule.timezone_offset_minutes,
            schedule.timezone_name.as_deref(),
        )?));
    }
    Ok(None)
}

pub(crate) fn row_to_schedule_execution(row: SqliteRow) -> Result<ScheduleExecutionRecord> {
    Ok(ScheduleExecutionRecord {
        id: row_uuid(&row, "id")?,
        schedule_id: row_uuid(&row, "schedule_id")?,
        intended_run_at: row.try_get("intended_run_at")?,
        state: row.try_get("state")?,
        summary: row
            .try_get::<Option<String>, _>("summary_json")?
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        error: row.try_get("error")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}

fn parse_schedule_mode(value: &str) -> Result<ScheduleMode> {
    match value {
        "download" => Ok(ScheduleMode::Download),
        "sniff_resources" => Ok(ScheduleMode::SniffResources),
        _ => Err(RavynError::Internal(format!(
            "unknown schedule mode {value}"
        ))),
    }
}

fn parse_schedule_overlap_policy(value: &str) -> Result<ScheduleOverlapPolicy> {
    match value {
        "skip" => Ok(ScheduleOverlapPolicy::Skip),
        "queue" => Ok(ScheduleOverlapPolicy::Queue),
        "replace" => Ok(ScheduleOverlapPolicy::Replace),
        "allow_parallel" => Ok(ScheduleOverlapPolicy::AllowParallel),
        _ => Err(RavynError::Internal(format!(
            "unknown schedule overlap policy {value}"
        ))),
    }
}

fn parse_schedule_missed_run_policy(value: &str) -> Result<ScheduleMissedRunPolicy> {
    match value {
        "skip" => Ok(ScheduleMissedRunPolicy::Skip),
        "run_once" => Ok(ScheduleMissedRunPolicy::RunOnce),
        "catch_up" => Ok(ScheduleMissedRunPolicy::CatchUp),
        _ => Err(RavynError::Internal(format!(
            "unknown schedule missed-run policy {value}"
        ))),
    }
}
