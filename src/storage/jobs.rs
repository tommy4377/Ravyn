//! Job rows: creation, lifecycle transitions, resume identity, actions,
//! and idempotency records.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Row, Sqlite, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::{CreateJob, DownloadOptions, Job, JobKind, JobStatus},
    error::{RavynError, Result},
    storage::Repository,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct JobActionRecord {
    pub id: Uuid,
    pub job_id: Uuid,
    pub action_index: usize,
    pub action: crate::core::models::PostAction,
    pub input_path: PathBuf,
    pub output_path: Option<PathBuf>,
    pub state: String,
    pub attempts: u64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct JobListFilter {
    pub cursor: Option<Uuid>,
    pub status: Option<JobStatus>,
    pub kind: Option<JobKind>,
    pub search: Option<String>,
    pub limit: usize,
}

impl Repository {
    pub async fn list_job_actions(&self, job_id: Uuid) -> Result<Vec<JobActionRecord>> {
        self.get_job(job_id).await?;
        sqlx::query("SELECT id,job_id,action_index,action_json,input_path,output_path,state,attempts,error,created_at,updated_at FROM job_actions WHERE job_id=? ORDER BY action_index")
            .bind(job_id.to_string())
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_job_action)
            .collect()
    }
    pub async fn get_idempotent_resource(
        &self,
        scope: &str,
        key: &str,
    ) -> Result<Option<(String, String)>> {
        sqlx::query("SELECT request_hash,resource_id FROM idempotency_keys WHERE scope=? AND key=?")
            .bind(scope)
            .bind(key)
            .fetch_optional(self.pool())
            .await?
            .map(|row| Ok((row.try_get("request_hash")?, row.try_get("resource_id")?)))
            .transpose()
    }

    pub async fn put_idempotent_resource(
        &self,
        scope: &str,
        key: &str,
        request_hash: &str,
        resource_id: Uuid,
    ) -> Result<()> {
        sqlx::query("INSERT INTO idempotency_keys(scope,key,request_hash,resource_id,created_at) VALUES(?,?,?,?,?)")
            .bind(scope)
            .bind(key)
            .bind(request_hash)
            .bind(resource_id.to_string())
            .bind(Utc::now())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn begin_job_action(
        &self,
        job_id: Uuid,
        action_index: usize,
        action: &crate::core::models::PostAction,
        input: &std::path::Path,
    ) -> Result<Option<PathBuf>> {
        if let Some(row) = sqlx::query(
            "SELECT state,output_path FROM job_actions WHERE job_id=? AND action_index=?",
        )
        .bind(job_id.to_string())
        .bind(action_index as i64)
        .fetch_optional(self.pool())
        .await?
        {
            let state: String = row.try_get("state")?;
            if state == "completed" {
                return Ok(row
                    .try_get::<Option<String>, _>("output_path")?
                    .map(PathBuf::from));
            }
        }
        let now = Utc::now();
        sqlx::query("INSERT INTO job_actions(id,job_id,action_index,action_json,input_path,state,attempts,created_at,updated_at) VALUES(?,?,?,?,?,'running',1,?,?) ON CONFLICT(job_id,action_index) DO UPDATE SET action_json=excluded.action_json,input_path=excluded.input_path,state='running',attempts=job_actions.attempts+1,error=NULL,updated_at=excluded.updated_at")
            .bind(Uuid::new_v4().to_string())
            .bind(job_id.to_string())
            .bind(action_index as i64)
            .bind(serde_json::to_string(action)?)
            .bind(input.to_string_lossy().to_string())
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await?;
        Ok(None)
    }

    pub async fn finish_job_action(
        &self,
        job_id: Uuid,
        action_index: usize,
        output: std::result::Result<&std::path::Path, &str>,
    ) -> Result<()> {
        let (state, output_path, error) = match output {
            Ok(path) => ("completed", Some(path.to_string_lossy().to_string()), None),
            Err(error) => ("failed", None, Some(error)),
        };
        sqlx::query("UPDATE job_actions SET state=?,output_path=?,error=?,updated_at=? WHERE job_id=? AND action_index=?")
            .bind(state)
            .bind(output_path)
            .bind(error)
            .bind(Utc::now())
            .bind(job_id.to_string())
            .bind(action_index as i64)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    /// Reconciles the remote validators with every durable resume signal.
    ///
    /// A reset is required whenever persisted segment state, transfer mode, job
    /// progress, or the partial file cannot be proven to belong to the current
    /// remote representation. The database reset and identity update happen in
    /// one transaction; the caller removes the partial file after commit.
    pub async fn set_remote_identity(
        &self,
        id: Uuid,
        final_url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        total: Option<u64>,
        partial_len: Option<u64>,
    ) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let previous = sqlx::query(
            "SELECT final_url,etag,last_modified,total_bytes,downloaded_bytes,transfer_mode FROM jobs WHERE id=?",
        )
        .bind(id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| RavynError::NotFound(format!("job {id}")))?;

        let segment_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM job_segments WHERE job_id=?")
                .bind(id.to_string())
                .fetch_one(&mut *tx)
                .await?
                .max(0) as u64;

        let old_final = previous.try_get::<Option<String>, _>("final_url")?;
        let old_etag = previous.try_get::<Option<String>, _>("etag")?;
        let old_modified = previous.try_get::<Option<String>, _>("last_modified")?;
        let old_total = previous
            .try_get::<Option<i64>, _>("total_bytes")?
            .and_then(|value| u64::try_from(value).ok());
        let downloaded =
            u64::try_from(previous.try_get::<i64, _>("downloaded_bytes")?).unwrap_or_default();
        let transfer_mode = previous.try_get::<String, _>("transfer_mode")?;
        let partial_bytes = partial_len.unwrap_or_default();

        let has_resume_state = downloaded > 0
            || segment_count > 0
            || partial_bytes > 0
            || matches!(transfer_mode.as_str(), "single" | "segmented");

        let validator_changed = match (old_etag.as_deref(), etag) {
            (Some(old), Some(new)) => old != new,
            (None, None) => old_modified.as_deref() != last_modified,
            _ => true,
        };
        let total_changed = (old_total.is_some() || total.is_some()) && old_total != total;
        let identity_changed = has_resume_state
            && (old_final.as_deref() != Some(final_url) || validator_changed || total_changed);

        let state_incompatible = match transfer_mode.as_str() {
            "segmented" => segment_count == 0 || total.is_none() || partial_len != total,
            "single" => segment_count > 0 || total.is_some_and(|expected| partial_bytes > expected),
            "none" => segment_count > 0 || partial_bytes > 0 || downloaded > 0,
            "complete" => segment_count > 0 || partial_bytes > 0,
            _ => true,
        };
        let reset_required = identity_changed || state_incompatible;

        if reset_required {
            sqlx::query("DELETE FROM job_segments WHERE job_id=?")
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE jobs SET downloaded_bytes=0,transfer_mode='none',available_at=NULL WHERE id=?",
            )
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("UPDATE jobs SET final_url=?,etag=?,last_modified=?,total_bytes=?,updated_at=? WHERE id=?")
            .bind(final_url)
            .bind(etag)
            .bind(last_modified)
            .bind(total.map(|value| value.min(i64::MAX as u64) as i64))
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(reset_required)
    }
    pub async fn insert_job(
        &self,
        request: CreateJob,
        default_destination: PathBuf,
    ) -> Result<Job> {
        let started = std::time::Instant::now();
        let now = Utc::now();
        let id = Uuid::new_v4();
        let speed_limit_bps = request
            .speed_limit_bps
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RavynError::Invalid("speed limit exceeds SQLite integer range".into()))?;
        let destination = request
            .destination
            .unwrap_or(default_destination)
            .to_string_lossy()
            .to_string();
        sqlx::query("INSERT INTO jobs(id,kind,source,destination,filename,status,priority,speed_limit_bps,expected_sha256,options_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id.to_string()).bind(kind_text(request.kind)).bind(request.source).bind(destination).bind(request.filename)
            .bind(status_text(JobStatus::Queued)).bind(request.priority).bind(speed_limit_bps)
            .bind(request.expected_sha256).bind(serde_json::to_string(&request.options)?).bind(now).bind(now).execute(self.pool()).await?;
        let job = self.get_job(id).await;
        self.observe_query("insert_job", started);
        job
    }

    pub async fn update_job_fields(
        &self,
        id: Uuid,
        priority: Option<i32>,
        speed_limit_bps: Option<Option<u64>>,
        destination: Option<&std::path::Path>,
        filename: Option<&str>,
        options: Option<&DownloadOptions>,
    ) -> Result<Job> {
        let mut query = QueryBuilder::<Sqlite>::new("UPDATE jobs SET updated_at=");
        query.push_bind(Utc::now());
        if let Some(priority) = priority {
            query.push(",priority=").push_bind(priority);
        }
        if let Some(speed) = speed_limit_bps {
            let speed = speed.map(i64::try_from).transpose().map_err(|_| {
                RavynError::Invalid("speed limit exceeds SQLite integer range".into())
            })?;
            query.push(",speed_limit_bps=").push_bind(speed);
        }
        if let Some(destination) = destination {
            query
                .push(",destination=")
                .push_bind(destination.to_string_lossy().to_string());
        }
        if let Some(filename) = filename {
            query.push(",filename=").push_bind(filename);
        }
        if let Some(options) = options {
            query
                .push(",options_json=")
                .push_bind(serde_json::to_string(options)?);
        }
        query.push(" WHERE id=").push_bind(id.to_string());
        let changed = query.build().execute(self.pool()).await?.rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("job {id}")));
        }
        self.get_job(id).await
    }

    pub async fn get_job(&self, id: Uuid) -> Result<Job> {
        let row = sqlx::query(&(JOB_SELECT.to_owned() + " WHERE id=?"))
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        row.map(row_to_job)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(id.to_string()))
    }

    pub async fn list_jobs(&self) -> Result<Vec<Job>> {
        sqlx::query(&(JOB_SELECT.to_owned() + " ORDER BY created_at DESC"))
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_job)
            .collect()
    }
    pub async fn list_jobs_page(&self, filter: JobListFilter) -> Result<Vec<Job>> {
        let limit = filter.limit.clamp(1, 200);
        let cursor = if let Some(id) = filter.cursor {
            sqlx::query("SELECT created_at FROM jobs WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(self.pool())
                .await?
                .map(|row| row.try_get::<DateTime<Utc>, _>("created_at"))
                .transpose()?
                .map(|created_at| (created_at, id.to_string()))
        } else {
            None
        };

        let mut query = QueryBuilder::<Sqlite>::new(JOB_SELECT);
        query.push(" WHERE 1=1");
        if let Some(status) = filter.status {
            query.push(" AND status=").push_bind(status_text(status));
        }
        if let Some(kind) = filter.kind {
            query.push(" AND kind=").push_bind(kind_text(kind));
        }
        if let Some(search) = filter.search.filter(|value| !value.trim().is_empty()) {
            query
                .push(" AND (source LIKE ")
                .push_bind(format!("%{}%", search.trim()))
                .push(" OR filename LIKE ")
                .push_bind(format!("%{}%", search.trim()))
                .push(")");
        }
        if let Some((created_at, id)) = cursor {
            query
                .push(" AND (created_at < ")
                .push_bind(created_at)
                .push(" OR (created_at = ")
                .push_bind(created_at)
                .push(" AND id < ")
                .push_bind(id)
                .push("))");
        }
        query
            .push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind((limit + 1) as i64);
        query
            .build()
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_job)
            .collect()
    }

    /// Atomically claims the highest-priority queued job in one SQLite statement.
    pub async fn claim_next_queued(&self) -> Result<Option<Job>> {
        let started = std::time::Instant::now();
        let now = Utc::now();
        let sql = format!(
            "UPDATE jobs SET status='downloading', available_at=NULL, started_at=COALESCE(started_at, ?), updated_at=? WHERE id=(SELECT id FROM jobs WHERE status='queued' AND (available_at IS NULL OR available_at<=?) ORDER BY priority DESC, created_at ASC LIMIT 1) AND status='queued' RETURNING {}",
            JOB_COLUMNS
        );
        let result = sqlx::query(&sql)
            .bind(now)
            .bind(now)
            .bind(now)
            .fetch_optional(self.pool())
            .await?
            .map(row_to_job)
            .transpose();
        self.observe_query("claim_next_queued", started);
        result
    }

    /// Returns a transiently unavailable job to the queue without allowing an
    /// immediate reclaim loop.
    pub async fn defer_job(
        &self,
        id: Uuid,
        delay: std::time::Duration,
        reason: &str,
    ) -> Result<()> {
        let available_at = Utc::now()
            + chrono::Duration::from_std(delay)
                .map_err(|error| RavynError::Internal(error.to_string()))?;
        sqlx::query(
            "UPDATE jobs SET status='queued',available_at=?,error=?,updated_at=? WHERE id=?",
        )
        .bind(available_at)
        .bind(reason)
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn clear_segments(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM job_segments WHERE job_id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Atomically marks a job completed from a verified local cache object.
    pub async fn complete_from_local_cache(&self, id: Uuid, size_bytes: u64) -> Result<()> {
        let size_bytes = size_bytes.min(i64::MAX as u64) as i64;
        let now = Utc::now();
        let changed = sqlx::query(
            "UPDATE jobs SET status='completed',downloaded_bytes=?,total_bytes=?,\
             transfer_mode='complete',error=NULL,started_at=COALESCE(started_at,?),\
             completed_at=?,updated_at=? WHERE id=?",
        )
        .bind(size_bytes)
        .bind(size_bytes)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("job {id}")));
        }
        Ok(())
    }

    pub async fn update_progress(
        &self,
        id: Uuid,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE jobs SET downloaded_bytes=?, total_bytes=COALESCE(?, total_bytes), updated_at=? WHERE id=?",
        )
        .bind(downloaded_bytes.min(i64::MAX as u64) as i64)
        .bind(total_bytes.map(|value| value.min(i64::MAX as u64) as i64))
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn update_progress_batch(
        &self,
        updates: &[crate::core::models::ProgressSnapshot],
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let started = std::time::Instant::now();
        let now = Utc::now();
        let mut tx = self.pool().begin().await?;
        for update in updates {
            sqlx::query(
                "UPDATE jobs SET downloaded_bytes=?,total_bytes=COALESCE(?,total_bytes),updated_at=? WHERE id=?",
            )
            .bind(update.downloaded_bytes.min(i64::MAX as u64) as i64)
            .bind(update.total_bytes.map(|value| value.min(i64::MAX as u64) as i64))
            .bind(now)
            .bind(update.job_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.observe_query("update_progress_batch", started);
        Ok(())
    }

    pub async fn set_transfer_mode(&self, id: Uuid, mode: &str) -> Result<()> {
        if !matches!(mode, "none" | "single" | "segmented" | "complete") {
            return Err(RavynError::Invalid(format!("invalid transfer mode {mode}")));
        }
        sqlx::query("UPDATE jobs SET transfer_mode=?,updated_at=? WHERE id=?")
            .bind(mode)
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn update_job_routing(
        &self,
        id: Uuid,
        destination: &std::path::Path,
        speed_limit_bps: Option<u64>,
        options: &DownloadOptions,
    ) -> Result<()> {
        sqlx::query("UPDATE jobs SET destination=?,speed_limit_bps=?,options_json=?,updated_at=? WHERE id=?")
            .bind(destination.to_string_lossy().to_string())
            .bind(speed_limit_bps.map(|value| value.min(i64::MAX as u64) as i64))
            .bind(serde_json::to_string(options)?)
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn set_status(&self, id: Uuid, status: JobStatus, error: Option<&str>) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE jobs SET status=?,error=?,available_at=CASE WHEN ?='queued' THEN NULL ELSE available_at END,updated_at=?,started_at=CASE WHEN ?='downloading' AND started_at IS NULL THEN ? ELSE started_at END,completed_at=CASE WHEN ?='completed' THEN ? ELSE completed_at END WHERE id=?")
            .bind(status_text(status)).bind(error).bind(status_text(status)).bind(now).bind(status_text(status)).bind(now).bind(status_text(status)).bind(now).bind(id.to_string()).execute(self.pool()).await?;
        Ok(())
    }

    /// Performs a guarded state transition and rejects invalid lifecycle changes.
    pub async fn transition_status(
        &self,
        id: Uuid,
        allowed_from: &[JobStatus],
        target: JobStatus,
        error: Option<&str>,
    ) -> Result<()> {
        self.get_job(id).await?;
        let allowed = allowed_from
            .iter()
            .map(|status| status_text(*status))
            .collect::<Vec<_>>();
        let placeholders = std::iter::repeat_n("?", allowed.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE jobs SET status=?,error=?,available_at=CASE WHEN ?='queued' THEN NULL ELSE available_at END,updated_at=? WHERE id=? AND status IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql)
            .bind(status_text(target))
            .bind(error)
            .bind(status_text(target))
            .bind(Utc::now())
            .bind(id.to_string());
        for status in allowed {
            query = query.bind(status);
        }
        let changed = query.execute(self.pool()).await?.rows_affected();
        if changed == 0 {
            let current = self.get_job(id).await?;
            return Err(RavynError::Conflict(format!(
                "cannot transition job from {:?} to {:?}",
                current.status, target
            )));
        }
        Ok(())
    }

    pub async fn delete_job(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM jobs WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn find_duplicate(&self, source: &str, destination: &str) -> Result<Option<Job>> {
        sqlx::query(
            &(JOB_SELECT.to_owned()
                + " WHERE source=? AND destination=? ORDER BY created_at DESC LIMIT 1"),
        )
        .bind(source)
        .bind(destination)
        .fetch_optional(self.pool())
        .await?
        .map(row_to_job)
        .transpose()
    }

    pub async fn recover_interrupted(&self) -> Result<()> {
        sqlx::query("UPDATE jobs SET status='queued',error='Recovered after restart',updated_at=? WHERE status IN ('probing','downloading','verifying','post_processing')")
            .bind(Utc::now()).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn attach_tags(&self, job_id: Uuid, tags: &[String]) -> Result<()> {
        if tags.is_empty() {
            return Ok(());
        }
        let mut merged = self.list_job_tags(job_id).await?;
        merged.extend(tags.iter().cloned());
        self.replace_job_tags(job_id, &merged).await?;
        Ok(())
    }
}

pub(crate) fn row_uuid(row: &SqliteRow, column: &str) -> Result<Uuid> {
    Uuid::parse_str(row.try_get::<String, _>(column)?.as_str())
        .map_err(|error| RavynError::Internal(format!("invalid {column} UUID: {error}")))
}

fn row_to_job_action(row: SqliteRow) -> Result<JobActionRecord> {
    Ok(JobActionRecord {
        id: row_uuid(&row, "id")?,
        job_id: row_uuid(&row, "job_id")?,
        action_index: usize::try_from(row.try_get::<i64, _>("action_index")?)
            .map_err(|_| RavynError::Internal("invalid action index in database".into()))?,
        action: serde_json::from_str(&row.try_get::<String, _>("action_json")?)?,
        input_path: PathBuf::from(row.try_get::<String, _>("input_path")?),
        output_path: row
            .try_get::<Option<String>, _>("output_path")?
            .map(PathBuf::from),
        state: row.try_get("state")?,
        attempts: u64::try_from(row.try_get::<i64, _>("attempts")?)
            .map_err(|_| RavynError::Internal("invalid action attempt count".into()))?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

const JOB_COLUMNS: &str = "id,kind,source,destination,filename,status,priority,total_bytes,downloaded_bytes,speed_limit_bps,expected_sha256,error,transfer_mode,options_json,created_at,updated_at,started_at,completed_at";
const JOB_SELECT: &str = "SELECT id,kind,source,destination,filename,status,priority,total_bytes,downloaded_bytes,speed_limit_bps,expected_sha256,error,transfer_mode,options_json,created_at,updated_at,started_at,completed_at FROM jobs";

fn row_to_job(row: SqliteRow) -> Result<Job> {
    Ok(Job {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|e| RavynError::Internal(e.to_string()))?,
        kind: parse_kind(&row.try_get::<String, _>("kind")?)?,
        source: row.try_get("source")?,
        destination: row.try_get("destination")?,
        filename: row.try_get("filename")?,
        status: parse_status(&row.try_get::<String, _>("status")?)?,
        priority: row.try_get("priority")?,
        total_bytes: row.try_get("total_bytes")?,
        downloaded_bytes: row.try_get("downloaded_bytes")?,
        speed_limit_bps: row.try_get("speed_limit_bps")?,
        expected_sha256: row.try_get("expected_sha256")?,
        error: row.try_get("error")?,
        transfer_mode: row.try_get("transfer_mode")?,
        options_json: serde_json::from_str(&row.try_get::<String, _>("options_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}
pub(crate) fn kind_text(v: JobKind) -> &'static str {
    match v {
        JobKind::Http => "http",
        JobKind::Media => "media",
        JobKind::Torrent => "torrent",
    }
}
fn status_text(v: JobStatus) -> &'static str {
    match v {
        JobStatus::Queued => "queued",
        JobStatus::Probing => "probing",
        JobStatus::Downloading => "downloading",
        JobStatus::Paused => "paused",
        JobStatus::Verifying => "verifying",
        JobStatus::PostProcessing => "post_processing",
        JobStatus::Seeding => "seeding",
        JobStatus::Completed => "completed",
        JobStatus::Partial => "partial",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}
pub(crate) fn parse_kind(v: &str) -> Result<JobKind> {
    match v {
        "http" => Ok(JobKind::Http),
        "media" => Ok(JobKind::Media),
        "torrent" => Ok(JobKind::Torrent),
        _ => Err(RavynError::Internal(format!("unknown job kind {v}"))),
    }
}
fn parse_status(v: &str) -> Result<JobStatus> {
    match v {
        "queued" => Ok(JobStatus::Queued),
        "probing" => Ok(JobStatus::Probing),
        "downloading" => Ok(JobStatus::Downloading),
        "paused" => Ok(JobStatus::Paused),
        "verifying" => Ok(JobStatus::Verifying),
        "post_processing" => Ok(JobStatus::PostProcessing),
        "seeding" => Ok(JobStatus::Seeding),
        "completed" => Ok(JobStatus::Completed),
        "partial" => Ok(JobStatus::Partial),
        "failed" => Ok(JobStatus::Failed),
        "cancelled" => Ok(JobStatus::Cancelled),
        _ => Err(RavynError::Internal(format!("unknown job status {v}"))),
    }
}
