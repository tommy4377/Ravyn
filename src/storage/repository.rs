use chrono::{DateTime, Duration, Utc};
use sqlx::{
    QueryBuilder, Row, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
};
use std::{path::PathBuf, str::FromStr};
use uuid::Uuid;

use crate::{
    adapters::torrent::TorrentSnapshot,
    core::models::{
        CreateJob, DownloadOptions, Job, JobKind, JobOutput, JobStatus, OutputSourceKind,
        OutputState, OutputType,
    },
    error::{RavynError, Result},
    services::{
        cron::CronExpression,
        rules::{Rule, RuleActions, RuleMatcher},
        schedules::{
            ScheduleMissedRunPolicy, ScheduleMode, ScheduleOverlapPolicy, ScheduledSniffOptions,
            next_cron_after,
        },
    },
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TorrentRecord {
    pub job_id: Uuid,
    pub torrent_id: String,
    pub info_hash: Option<String>,
    pub name: Option<String>,
    pub state: String,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub download_speed_bps: u64,
    pub upload_speed_bps: u64,
    pub peers_connected: u64,
    pub seeders: u64,
    pub leechers: u64,
    pub raw: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct JobLogRecord {
    pub id: i64,
    pub job_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub source_module: String,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditRecord {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SecretReference {
    pub id: Uuid,
    pub name: String,
    pub secret_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
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
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn put_secret_reference(
        &self,
        name: &str,
        secret_type: &str,
        secret: String,
    ) -> Result<SecretReference> {
        const TYPES: &[&str] = &[
            "api_token",
            "proxy_credentials",
            "rqbit_credentials",
            "cookies",
            "authentication_header",
            "tls_certificate",
            "private_key",
        ];
        let name = name.trim();
        if name.is_empty() || name.len() > 160 || !TYPES.contains(&secret_type) {
            return Err(RavynError::Invalid("invalid secret name or type".into()));
        }
        let existing = sqlx::query("SELECT id,keyring_account FROM secret_references WHERE name=?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        let (id, account) = match existing {
            Some(row) => (
                row_uuid(&row, "id")?,
                row.try_get::<String, _>("keyring_account")?,
            ),
            None => {
                let id = Uuid::new_v4();
                (id, id.to_string())
            }
        };
        crate::services::secrets::set(account.clone(), secret).await?;
        let now = Utc::now();
        sqlx::query("INSERT INTO secret_references(id,name,secret_type,keyring_account,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(name) DO UPDATE SET secret_type=excluded.secret_type,updated_at=excluded.updated_at")
            .bind(id.to_string()).bind(name).bind(secret_type).bind(account).bind(now).bind(now)
            .execute(&self.pool).await?;
        self.get_secret_reference(id).await
    }

    pub async fn list_secret_references(&self) -> Result<Vec<SecretReference>> {
        sqlx::query(
            "SELECT id,name,secret_type,created_at,updated_at FROM secret_references ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(row_to_secret_reference)
        .collect()
    }

    pub async fn get_secret_reference(&self, id: Uuid) -> Result<SecretReference> {
        sqlx::query(
            "SELECT id,name,secret_type,created_at,updated_at FROM secret_references WHERE id=?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .map(row_to_secret_reference)
        .transpose()?
        .ok_or_else(|| RavynError::NotFound(format!("secret reference {id}")))
    }

    pub async fn resolve_secret_reference(&self, id: Uuid, expected_type: &str) -> Result<String> {
        let row =
            sqlx::query("SELECT secret_type,keyring_account FROM secret_references WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| RavynError::NotFound(format!("secret reference {id}")))?;
        let secret_type: String = row.try_get("secret_type")?;
        if secret_type != expected_type {
            return Err(RavynError::Invalid(format!(
                "secret reference {id} has type {secret_type}, expected {expected_type}"
            )));
        }
        let account: String = row.try_get("keyring_account")?;
        crate::services::secrets::get(account).await
    }

    pub async fn delete_secret_reference(&self, id: Uuid) -> Result<()> {
        let row = sqlx::query("SELECT keyring_account FROM secret_references WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("secret reference {id}")))?;
        let account: String = row.try_get("keyring_account")?;
        crate::services::secrets::delete(account).await?;
        sqlx::query("DELETE FROM secret_references WHERE id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn append_job_log(
        &self,
        job_id: Uuid,
        source_module: &str,
        severity: &str,
        code: &str,
        message: &str,
    ) -> Result<()> {
        if !matches!(severity, "debug" | "info" | "warn" | "error") {
            return Err(RavynError::Invalid("invalid job log severity".into()));
        }
        sqlx::query("INSERT INTO job_logs(job_id,timestamp,source_module,severity,code,message,metadata_json) VALUES(?,?,?,?,?,?,'{}')")
            .bind(job_id.to_string()).bind(Utc::now()).bind(source_module).bind(severity)
            .bind(code).bind(message).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_job_logs(&self, job_id: Uuid, limit: usize) -> Result<Vec<JobLogRecord>> {
        self.get_job(job_id).await?;
        sqlx::query("SELECT id,job_id,timestamp,source_module,severity,code,message,metadata_json FROM job_logs WHERE job_id=? ORDER BY timestamp DESC,id DESC LIMIT ?")
            .bind(job_id.to_string()).bind(i64::try_from(limit.clamp(1, 500)).unwrap_or(500))
            .fetch_all(&self.pool).await?.into_iter().map(row_to_job_log).collect()
    }

    pub async fn append_audit(
        &self,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        outcome: &str,
    ) -> Result<()> {
        self.append_audit_with_metadata(
            action,
            resource_type,
            resource_id,
            outcome,
            serde_json::json!({}),
        )
        .await
    }

    pub async fn append_audit_with_metadata(
        &self,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        outcome: &str,
        metadata: serde_json::Value,
    ) -> Result<()> {
        if !matches!(outcome, "success" | "failure") {
            return Err(RavynError::Invalid("invalid audit outcome".into()));
        }
        if !metadata.is_object() {
            return Err(RavynError::Invalid(
                "audit metadata must be a JSON object".into(),
            ));
        }
        sqlx::query("INSERT INTO audit_log(timestamp,action,resource_type,resource_id,outcome,metadata_json) VALUES(?,?,?,?,?,?)")
            .bind(Utc::now())
            .bind(action)
            .bind(resource_type)
            .bind(resource_id)
            .bind(outcome)
            .bind(serde_json::to_string(&metadata)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_audit(&self, limit: usize) -> Result<Vec<AuditRecord>> {
        sqlx::query("SELECT id,timestamp,action,resource_type,resource_id,outcome,metadata_json FROM audit_log ORDER BY timestamp DESC,id DESC LIMIT ?")
            .bind(i64::try_from(limit.clamp(1, 500)).unwrap_or(500))
            .fetch_all(&self.pool).await?.into_iter().map(row_to_audit).collect()
    }

    pub async fn run_retention(&self, older_than: DateTime<Utc>) -> Result<serde_json::Value> {
        let mut tx = self.pool.begin().await?;
        let logs = sqlx::query("DELETE FROM job_logs WHERE timestamp < ?")
            .bind(older_than)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let executions = sqlx::query(
            "DELETE FROM schedule_executions WHERE completed_at IS NOT NULL AND completed_at < ?",
        )
        .bind(older_than)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let idempotency = sqlx::query("DELETE FROM idempotency_keys WHERE created_at < ?")
            .bind(older_than)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let audit = sqlx::query("DELETE FROM audit_log WHERE timestamp < ?")
            .bind(older_than)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(
            serde_json::json!({"job_logs":logs,"schedule_executions":executions,"idempotency_keys":idempotency,"audit_records":audit}),
        )
    }

    pub async fn load_persistent_settings(
        &self,
    ) -> Result<Option<crate::config::PersistentSettings>> {
        let row = sqlx::query("SELECT settings_json FROM runtime_settings WHERE id=1")
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let json: String = row.try_get("settings_json")?;
            serde_json::from_str(&json).map_err(Into::into)
        })
        .transpose()
    }

    pub async fn save_persistent_settings(
        &self,
        settings: &crate::config::PersistentSettings,
    ) -> Result<()> {
        sqlx::query("INSERT INTO runtime_settings(id,settings_json,updated_at) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET settings_json=excluded.settings_json,updated_at=excluded.updated_at")
            .bind(serde_json::to_string(settings)?)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn reset_persistent_settings(&self) -> Result<()> {
        sqlx::query("DELETE FROM runtime_settings WHERE id=1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_job_actions(&self, job_id: Uuid) -> Result<Vec<JobActionRecord>> {
        self.get_job(job_id).await?;
        sqlx::query("SELECT id,job_id,action_index,action_json,input_path,output_path,state,attempts,error,created_at,updated_at FROM job_actions WHERE job_id=? ORDER BY action_index")
            .bind(job_id.to_string())
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_to_job_action)
            .collect()
    }

    pub async fn list_schedule_executions(
        &self,
        schedule_id: Uuid,
        limit: usize,
    ) -> Result<Vec<ScheduleExecutionRecord>> {
        self.get_schedule(schedule_id).await?;
        sqlx::query("SELECT id,schedule_id,intended_run_at,state,summary_json,error,started_at,completed_at FROM schedule_executions WHERE schedule_id=? ORDER BY started_at DESC,id DESC LIMIT ?")
            .bind(schedule_id.to_string())
            .bind(i64::try_from(limit.clamp(1, 200)).map_err(|_| RavynError::Invalid("execution page limit is invalid".into()))?)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_to_schedule_execution)
            .collect()
    }

    pub async fn get_schedule_execution(&self, id: Uuid) -> Result<ScheduleExecutionRecord> {
        sqlx::query("SELECT id,schedule_id,intended_run_at,state,summary_json,error,started_at,completed_at FROM schedule_executions WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .map(row_to_schedule_execution)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("schedule execution {id}")))
    }

    pub async fn cancel_schedule_execution(&self, id: Uuid) -> Result<ScheduleExecutionRecord> {
        let changed = sqlx::query("UPDATE schedule_executions SET state='cancelled',cancellation_requested=1,completed_at=?,error=COALESCE(error,'cancelled by API') WHERE id=? AND state='running'")
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(&self.pool)
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
            .execute(&self.pool)
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("schedule {id}")));
        }
        self.get_schedule(id).await
    }

    pub async fn register_output(
        &self,
        job: &Job,
        path: &std::path::Path,
        output_type: OutputType,
        source_kind: OutputSourceKind,
    ) -> Result<JobOutput> {
        self.register_output_with_metadata(
            job,
            path,
            output_type,
            source_kind,
            serde_json::Value::Object(Default::default()),
        )
        .await
    }

    pub async fn register_output_with_metadata(
        &self,
        job: &Job,
        path: &std::path::Path,
        output_type: OutputType,
        source_kind: OutputSourceKind,
        metadata_json: serde_json::Value,
    ) -> Result<JobOutput> {
        let metadata = tokio::fs::metadata(path).await?;
        let destination = std::path::Path::new(&job.destination);
        let relative = path.strip_prefix(destination).map_err(|_| {
            RavynError::Invalid(format!(
                "output is outside the job destination: {}",
                path.display()
            ))
        })?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let size = if metadata.is_file() {
            Some(i64::try_from(metadata.len()).map_err(|_| {
                RavynError::Invalid("output size exceeds SQLite integer range".into())
            })?)
        } else {
            None
        };
        let mime_type = inferred_mime_type(path, metadata.is_dir());
        sqlx::query("INSERT INTO job_outputs(id,job_id,output_type,original_path,current_path,relative_path,size_bytes,mime_type,state,source_kind,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, 'ready',?,?,?,?) ON CONFLICT(job_id,original_path) DO UPDATE SET current_path=excluded.current_path,relative_path=excluded.relative_path,size_bytes=excluded.size_bytes,mime_type=excluded.mime_type,output_type=excluded.output_type,state='ready',source_kind=excluded.source_kind,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at")
            .bind(id.to_string())
            .bind(job.id.to_string())
            .bind(output_type_text(output_type))
            .bind(path.to_string_lossy().to_string())
            .bind(path.to_string_lossy().to_string())
            .bind(relative.to_string_lossy().to_string())
            .bind(size)
            .bind(mime_type)
            .bind(output_source_text(source_kind))
            .bind(serde_json::to_string(&metadata_json)?)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        self.get_output_by_original(job.id, path).await
    }

    pub async fn update_output_path(
        &self,
        job: &Job,
        output_id: Uuid,
        current_path: &std::path::Path,
        state: OutputState,
    ) -> Result<()> {
        let relative = relative_output_path(job, current_path)?;
        let metadata = tokio::fs::metadata(current_path).await.ok();
        let size = metadata
            .as_ref()
            .filter(|value| value.is_file())
            .map(|value| i64::try_from(value.len()))
            .transpose()
            .map_err(|_| RavynError::Invalid("output size exceeds SQLite integer range".into()))?;
        let mime_type = inferred_mime_type(
            current_path,
            metadata.as_ref().is_some_and(|value| value.is_dir()),
        );
        sqlx::query("UPDATE job_outputs SET current_path=?,relative_path=?,size_bytes=COALESCE(?,size_bytes),mime_type=COALESCE(?,mime_type),state=?,updated_at=? WHERE id=?")
            .bind(current_path.to_string_lossy().to_string())
            .bind(relative.to_string_lossy().to_string())
            .bind(size)
            .bind(mime_type)
            .bind(output_state_text(state))
            .bind(Utc::now())
            .bind(output_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_output_state(&self, output_id: Uuid, state: OutputState) -> Result<()> {
        sqlx::query("UPDATE job_outputs SET state=?,updated_at=? WHERE id=?")
            .bind(output_state_text(state))
            .bind(Utc::now())
            .bind(output_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_output_checksum(
        &self,
        output_id: Uuid,
        algorithm: &str,
        value: &str,
    ) -> Result<()> {
        if algorithm.trim().is_empty() || value.trim().is_empty() {
            return Err(RavynError::Invalid(
                "checksum algorithm and value must not be empty".into(),
            ));
        }
        sqlx::query(
            "UPDATE job_outputs SET checksum_algorithm=?,checksum_value=?,updated_at=? WHERE id=?",
        )
        .bind(algorithm.to_ascii_lowercase())
        .bind(value.to_ascii_lowercase())
        .bind(Utc::now())
        .bind(output_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn register_derived_output(
        &self,
        job: &Job,
        parent_output_id: Uuid,
        path: &std::path::Path,
        output_type: OutputType,
        action_index: usize,
        metadata: serde_json::Value,
    ) -> Result<JobOutput> {
        let file_metadata = tokio::fs::metadata(path).await?;
        let relative = relative_output_path(job, path)?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let size = if file_metadata.is_file() {
            Some(i64::try_from(file_metadata.len()).map_err(|_| {
                RavynError::Invalid("output size exceeds SQLite integer range".into())
            })?)
        } else {
            None
        };
        let action_index = i64::try_from(action_index)
            .map_err(|_| RavynError::Invalid("post-action index exceeds SQLite range".into()))?;
        sqlx::query("INSERT INTO job_outputs(id,job_id,output_type,original_path,current_path,relative_path,size_bytes,mime_type,state,source_kind,parent_output_id,producing_action_index,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, 'ready','post_process',?,?,?,?,?) ON CONFLICT(job_id,original_path) DO UPDATE SET current_path=excluded.current_path,relative_path=excluded.relative_path,size_bytes=excluded.size_bytes,mime_type=excluded.mime_type,output_type=excluded.output_type,state='ready',source_kind='post_process',parent_output_id=excluded.parent_output_id,producing_action_index=excluded.producing_action_index,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at")
            .bind(id.to_string())
            .bind(job.id.to_string())
            .bind(output_type_text(output_type))
            .bind(path.to_string_lossy().to_string())
            .bind(path.to_string_lossy().to_string())
            .bind(relative.to_string_lossy().to_string())
            .bind(size)
            .bind(inferred_mime_type(path, file_metadata.is_dir()))
            .bind(parent_output_id.to_string())
            .bind(action_index)
            .bind(serde_json::to_string(&metadata)?)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        self.get_output_by_original(job.id, path).await
    }

    async fn get_output_by_original(
        &self,
        job_id: Uuid,
        path: &std::path::Path,
    ) -> Result<JobOutput> {
        sqlx::query("SELECT * FROM job_outputs WHERE job_id=? AND original_path=?")
            .bind(job_id.to_string())
            .bind(path.to_string_lossy().to_string())
            .fetch_optional(&self.pool)
            .await?
            .map(row_to_output)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("output for {}", path.display())))
    }

    pub async fn list_job_outputs(&self, job_id: Uuid) -> Result<Vec<JobOutput>> {
        sqlx::query("SELECT * FROM job_outputs WHERE job_id=? ORDER BY created_at,id")
            .bind(job_id.to_string())
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_to_output)
            .collect()
    }

    pub async fn find_job_output_by_path(
        &self,
        job_id: Uuid,
        path: &std::path::Path,
    ) -> Result<Option<JobOutput>> {
        let path = path.to_string_lossy().to_string();
        sqlx::query(
            "SELECT * FROM job_outputs WHERE job_id=? AND (original_path=? OR current_path=?) ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(job_id.to_string())
        .bind(&path)
        .bind(&path)
        .fetch_optional(&self.pool)
        .await?
        .map(row_to_output)
        .transpose()
    }

    pub async fn get_idempotent_resource(
        &self,
        scope: &str,
        key: &str,
    ) -> Result<Option<(String, String)>> {
        sqlx::query("SELECT request_hash,resource_id FROM idempotency_keys WHERE scope=? AND key=?")
            .bind(scope)
            .bind(key)
            .fetch_optional(&self.pool)
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
            .execute(&self.pool)
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
        .fetch_optional(&self.pool)
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
            .execute(&self.pool)
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
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn upsert_torrent_record(
        &self,
        job_id: Uuid,
        snapshot: &TorrentSnapshot,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO torrent_jobs(job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(job_id) DO UPDATE SET torrent_id=excluded.torrent_id,info_hash=excluded.info_hash,name=excluded.name,state=excluded.state,downloaded_bytes=excluded.downloaded_bytes,uploaded_bytes=excluded.uploaded_bytes,total_bytes=excluded.total_bytes,download_speed_bps=excluded.download_speed_bps,upload_speed_bps=excluded.upload_speed_bps,peers_connected=excluded.peers_connected,seeders=excluded.seeders,leechers=excluded.leechers,raw_json=excluded.raw_json,updated_at=excluded.updated_at",
        )
        .bind(job_id.to_string())
        .bind(&snapshot.torrent_id)
        .bind(&snapshot.info_hash)
        .bind(&snapshot.name)
        .bind(&snapshot.state)
        .bind(snapshot.downloaded_bytes.min(i64::MAX as u64) as i64)
        .bind(snapshot.uploaded_bytes.min(i64::MAX as u64) as i64)
        .bind(snapshot.total_bytes.map(|value| value.min(i64::MAX as u64) as i64))
        .bind(snapshot.download_speed_bps.min(i64::MAX as u64) as i64)
        .bind(snapshot.upload_speed_bps.min(i64::MAX as u64) as i64)
        .bind(snapshot.peers_connected.min(i64::MAX as u64) as i64)
        .bind(snapshot.seeders.min(i64::MAX as u64) as i64)
        .bind(snapshot.leechers.min(i64::MAX as u64) as i64)
        .bind(serde_json::to_string(&snapshot.raw)?)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_torrent_record(&self, job_id: Uuid) -> Result<Option<TorrentRecord>> {
        sqlx::query("SELECT job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,updated_at FROM torrent_jobs WHERE job_id=?")
            .bind(job_id.to_string())
            .fetch_optional(&self.pool)
            .await?
            .map(row_to_torrent_record)
            .transpose()
    }

    pub async fn list_torrent_records(&self) -> Result<Vec<TorrentRecord>> {
        sqlx::query("SELECT job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,updated_at FROM torrent_jobs ORDER BY updated_at DESC")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_to_torrent_record)
            .collect()
    }

    pub async fn delete_torrent_record(&self, job_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM torrent_jobs WHERE job_id=?")
            .bind(job_id.to_string())
            .execute(&self.pool)
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
        let mut tx = self.pool.begin().await?;
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

    pub async fn connect(url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn insert_job(
        &self,
        request: CreateJob,
        default_destination: PathBuf,
    ) -> Result<Job> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let destination = request
            .destination
            .unwrap_or(default_destination)
            .to_string_lossy()
            .to_string();
        sqlx::query("INSERT INTO jobs(id,kind,source,destination,filename,status,priority,speed_limit_bps,expected_sha256,options_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id.to_string()).bind(kind_text(request.kind)).bind(request.source).bind(destination).bind(request.filename)
            .bind(status_text(JobStatus::Queued)).bind(request.priority).bind(request.speed_limit_bps.map(|v| v as i64))
            .bind(request.expected_sha256).bind(serde_json::to_string(&request.options)?).bind(now).bind(now).execute(&self.pool).await?;
        self.get_job(id).await
    }

    pub async fn update_job_fields(
        &self,
        id: Uuid,
        priority: Option<i32>,
        speed_limit_bps: Option<Option<u64>>,
        destination: Option<&std::path::Path>,
        filename: Option<&str>,
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
        query.push(" WHERE id=").push_bind(id.to_string());
        let changed = query.build().execute(&self.pool).await?.rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("job {id}")));
        }
        self.get_job(id).await
    }

    pub async fn get_job(&self, id: Uuid) -> Result<Job> {
        let row = sqlx::query(&(JOB_SELECT.to_owned() + " WHERE id=?"))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(row_to_job)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(id.to_string()))
    }

    pub async fn list_jobs(&self) -> Result<Vec<Job>> {
        sqlx::query(&(JOB_SELECT.to_owned() + " ORDER BY created_at DESC"))
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_to_job)
            .collect()
    }

    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn integrity_check(&self) -> Result<String> {
        Ok(sqlx::query("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await?
            .try_get(0)?)
    }

    pub async fn verify_database_file(path: &std::path::Path) -> Result<String> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
            .read_only(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let result: String = sqlx::query("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await?
            .try_get(0)?;
        pool.close().await;
        Ok(result)
    }

    pub async fn database_version(&self) -> Result<i64> {
        Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(version) FROM _sqlx_migrations WHERE success=1",
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or_default())
    }

    pub async fn backup_to(&self, destination: &std::path::Path) -> Result<()> {
        if tokio::fs::try_exists(destination).await? {
            return Err(RavynError::Conflict(format!(
                "backup destination already exists: {}",
                destination.display()
            )));
        }
        sqlx::query("VACUUM INTO ?")
            .bind(destination.to_string_lossy().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn job_status_counts(&self) -> Result<Vec<(String, i64)>> {
        sqlx::query("SELECT status,COUNT(*) AS count FROM jobs GROUP BY status")
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| Ok((row.try_get("status")?, row.try_get("count")?)))
            .collect()
    }

    pub async fn operational_metrics(&self) -> Result<(i64, i64, i64, i64)> {
        let row = sqlx::query(
            "SELECT
                (SELECT COUNT(*) FROM jobs WHERE status='queued') AS queue_depth,
                (SELECT COALESCE(SUM(downloaded_bytes),0) FROM jobs) AS bytes_transferred,
                (SELECT COUNT(*) FROM job_outputs WHERE state='ready') AS output_count,
                (SELECT COUNT(*) FROM jobs WHERE status='failed') AS failure_count",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((
            row.try_get("queue_depth")?,
            row.try_get("bytes_transferred")?,
            row.try_get("output_count")?,
            row.try_get("failure_count")?,
        ))
    }

    pub async fn list_jobs_page(&self, filter: JobListFilter) -> Result<Vec<Job>> {
        let limit = filter.limit.clamp(1, 200);
        let cursor = if let Some(id) = filter.cursor {
            sqlx::query("SELECT created_at FROM jobs WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
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
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(row_to_job)
            .collect()
    }

    /// Atomically claims the highest-priority queued job in one SQLite statement.
    pub async fn claim_next_queued(&self) -> Result<Option<Job>> {
        let now = Utc::now();
        let sql = format!(
            "UPDATE jobs SET status='downloading', available_at=NULL, started_at=COALESCE(started_at, ?), updated_at=? WHERE id=(SELECT id FROM jobs WHERE status='queued' AND (available_at IS NULL OR available_at<=?) ORDER BY priority DESC, created_at ASC LIMIT 1) AND status='queued' RETURNING {}",
            JOB_COLUMNS
        );
        sqlx::query(&sql)
            .bind(now)
            .bind(now)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?
            .map(row_to_job)
            .transpose()
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_segments(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM job_segments WHERE job_id=?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
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
        .execute(&self.pool)
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
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;
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
            .execute(&self.pool)
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
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn set_status(&self, id: Uuid, status: JobStatus, error: Option<&str>) -> Result<()> {
        let now = Utc::now();
        sqlx::query("UPDATE jobs SET status=?,error=?,available_at=CASE WHEN ?='queued' THEN NULL ELSE available_at END,updated_at=?,started_at=CASE WHEN ?='downloading' AND started_at IS NULL THEN ? ELSE started_at END,completed_at=CASE WHEN ?='completed' THEN ? ELSE completed_at END WHERE id=?")
            .bind(status_text(status)).bind(error).bind(status_text(status)).bind(now).bind(status_text(status)).bind(now).bind(status_text(status)).bind(now).bind(id.to_string()).execute(&self.pool).await?;
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
        let changed = query.execute(&self.pool).await?.rows_affected();
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
            .execute(&self.pool)
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
        .fetch_optional(&self.pool)
        .await?
        .map(row_to_job)
        .transpose()
    }

    pub async fn recover_interrupted(&self) -> Result<()> {
        sqlx::query("UPDATE jobs SET status='queued',error='Recovered after restart',updated_at=? WHERE status IN ('probing','downloading','verifying','post_processing')")
            .bind(Utc::now()).execute(&self.pool).await?;
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

    pub async fn list_rules(&self) -> Result<Vec<Rule>> {
        let rows = sqlx::query("SELECT id,name,enabled,priority,matcher_json,actions_json FROM rules WHERE enabled=1 ORDER BY priority DESC")
            .fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                Ok(Rule {
                    id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
                        .map_err(|e| RavynError::Internal(e.to_string()))?,
                    name: row.try_get("name")?,
                    enabled: row.try_get::<i64, _>("enabled")? != 0,
                    priority: row.try_get("priority")?,
                    matcher: serde_json::from_str::<RuleMatcher>(
                        &row.try_get::<String, _>("matcher_json")?,
                    )?,
                    actions: serde_json::from_str::<RuleActions>(
                        &row.try_get::<String, _>("actions_json")?,
                    )?,
                })
            })
            .collect()
    }

    /// Atomically leases one due schedule so multiple Ravyn instances cannot run it twice.
    pub async fn claim_due_schedule(
        &self,
        lease: std::time::Duration,
    ) -> Result<Option<ScheduleClaim>> {
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
            .fetch_optional(&self.pool)
            .await?;
        let Some(schedule) = row.map(Schedule::from_row).transpose()? else {
            return Ok(None);
        };
        if schedule.overlap_policy == ScheduleOverlapPolicy::Replace {
            sqlx::query("UPDATE schedule_executions SET cancellation_requested=1,error=COALESCE(error,'replaced by a newer overlapping execution') WHERE schedule_id=? AND state='running'")
                .bind(schedule.id.to_string())
                .execute(&self.pool)
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
            .execute(&self.pool)
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
            .execute(&self.pool)
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
            .execute(&self.pool)
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
            .execute(&self.pool)
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
        .fetch_optional(&self.pool)
        .await?
        .is_some_and(|value| value != 0))
    }

    pub async fn advance_skipped_schedules(&self) -> Result<u64> {
        let now = Utc::now();
        let sql = "SELECT id,enabled,source,kind,destination,mode,automation_json,interval_seconds,cron_expression,next_run_at,timezone_offset_minutes,timezone_name,overlap_policy,missed_run_policy,max_catch_up_runs,catch_up_runs,paused_until,options_json,last_run_at,failure_count,last_error,created_at,updated_at FROM schedules s WHERE s.enabled=1 AND s.next_run_at<=? AND s.claim_token IS NULL AND ((s.missed_run_policy='skip') OR (s.overlap_policy='skip' AND EXISTS(SELECT 1 FROM schedule_executions se WHERE se.schedule_id=s.id AND se.state='running'))) ORDER BY s.next_run_at LIMIT 256";
        let schedules = sqlx::query(sql)
            .bind(now)
            .fetch_all(&self.pool)
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
                .fetch_one(&self.pool)
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
                .execute(&self.pool)
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
            .execute(&self.pool)
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

fn relative_output_path(job: &Job, path: &std::path::Path) -> Result<PathBuf> {
    let destination = std::path::Path::new(&job.destination);
    if let Ok(relative) = path.strip_prefix(destination) {
        return Ok(relative.to_path_buf());
    }
    path.file_name().map(PathBuf::from).ok_or_else(|| {
        RavynError::Invalid(format!(
            "cannot derive a relative output path for {}",
            path.display()
        ))
    })
}

fn inferred_mime_type(path: &std::path::Path, directory: bool) -> Option<&'static str> {
    if directory {
        return Some("inode/directory");
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" => Some("video/mp4"),
        "mkv" => Some("video/x-matroska"),
        "webm" => Some("video/webm"),
        "mov" => Some("video/quicktime"),
        "avi" => Some("video/x-msvideo"),
        "mp3" => Some("audio/mpeg"),
        "m4a" => Some("audio/mp4"),
        "aac" => Some("audio/aac"),
        "flac" => Some("audio/flac"),
        "opus" => Some("audio/opus"),
        "wav" => Some("audio/wav"),
        "srt" => Some("application/x-subrip"),
        "vtt" => Some("text/vtt"),
        "json" => Some("application/json"),
        "txt" | "description" => Some("text/plain"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "gif" => Some("image/gif"),
        "pdf" => Some("application/pdf"),
        "zip" => Some("application/zip"),
        "7z" => Some("application/x-7z-compressed"),
        "rar" => Some("application/vnd.rar"),
        "gz" => Some("application/gzip"),
        "tar" => Some("application/x-tar"),
        "torrent" => Some("application/x-bittorrent"),
        _ => None,
    }
}

pub(crate) fn row_to_output(row: SqliteRow) -> Result<JobOutput> {
    let parse_uuid = |column: &str| -> Result<Uuid> {
        Uuid::parse_str(row.try_get::<String, _>(column)?.as_str())
            .map_err(|error| RavynError::Internal(format!("invalid output {column}: {error}")))
    };
    let size_bytes = row
        .try_get::<Option<i64>, _>("size_bytes")?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| RavynError::Internal("negative output size in database".into()))?;
    Ok(JobOutput {
        id: parse_uuid("id")?,
        job_id: parse_uuid("job_id")?,
        output_type: parse_output_type(&row.try_get::<String, _>("output_type")?)?,
        original_path: PathBuf::from(row.try_get::<String, _>("original_path")?),
        current_path: PathBuf::from(row.try_get::<String, _>("current_path")?),
        relative_path: PathBuf::from(row.try_get::<String, _>("relative_path")?),
        size_bytes,
        mime_type: row.try_get("mime_type")?,
        checksum_algorithm: row.try_get("checksum_algorithm")?,
        checksum_value: row.try_get("checksum_value")?,
        state: parse_output_state(&row.try_get::<String, _>("state")?)?,
        source_kind: parse_output_source(&row.try_get::<String, _>("source_kind")?)?,
        parent_output_id: row
            .try_get::<Option<String>, _>("parent_output_id")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|error| {
                RavynError::Internal(format!("invalid parent output UUID: {error}"))
            })?,
        producing_action_index: row
            .try_get::<Option<i64>, _>("producing_action_index")?
            .map(usize::try_from)
            .transpose()
            .map_err(|_| RavynError::Internal("invalid output action index".into()))?,
        metadata: serde_json::from_str(&row.try_get::<String, _>("metadata_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_uuid(row: &SqliteRow, column: &str) -> Result<Uuid> {
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

pub(crate) fn row_to_job_log(row: SqliteRow) -> Result<JobLogRecord> {
    Ok(JobLogRecord {
        id: row.try_get("id")?,
        job_id: row_uuid(&row, "job_id")?,
        timestamp: row.try_get("timestamp")?,
        source_module: row.try_get("source_module")?,
        severity: row.try_get("severity")?,
        code: row.try_get("code")?,
        message: row.try_get("message")?,
        metadata: serde_json::from_str(&row.try_get::<String, _>("metadata_json")?)?,
    })
}

pub(crate) fn row_to_audit(row: SqliteRow) -> Result<AuditRecord> {
    Ok(AuditRecord {
        id: row.try_get("id")?,
        timestamp: row.try_get("timestamp")?,
        action: row.try_get("action")?,
        resource_type: row.try_get("resource_type")?,
        resource_id: row.try_get("resource_id")?,
        outcome: row.try_get("outcome")?,
        metadata: serde_json::from_str(&row.try_get::<String, _>("metadata_json")?)?,
    })
}

pub(crate) fn row_to_secret_reference(row: SqliteRow) -> Result<SecretReference> {
    Ok(SecretReference {
        id: row_uuid(&row, "id")?,
        name: row.try_get("name")?,
        secret_type: row.try_get("secret_type")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
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

fn output_type_text(value: OutputType) -> &'static str {
    match value {
        OutputType::Primary => "primary",
        OutputType::Video => "video",
        OutputType::Audio => "audio",
        OutputType::Subtitle => "subtitle",
        OutputType::Thumbnail => "thumbnail",
        OutputType::Metadata => "metadata",
        OutputType::TorrentFile => "torrent_file",
        OutputType::ExtractedFile => "extracted_file",
        OutputType::ConvertedFile => "converted_file",
        OutputType::Archive => "archive",
        OutputType::Directory => "directory",
        OutputType::Temporary => "temporary",
        OutputType::Other => "other",
    }
}

fn output_state_text(value: OutputState) -> &'static str {
    match value {
        OutputState::Planned => "planned",
        OutputState::Creating => "creating",
        OutputState::Ready => "ready",
        OutputState::Failed => "failed",
        OutputState::Deleted => "deleted",
        OutputState::Moved => "moved",
        OutputState::Replaced => "replaced",
    }
}

fn output_source_text(value: OutputSourceKind) -> &'static str {
    match value {
        OutputSourceKind::Http => "http",
        OutputSourceKind::Media => "media",
        OutputSourceKind::Torrent => "torrent",
        OutputSourceKind::PostProcess => "post_process",
    }
}

fn parse_output_type(value: &str) -> Result<OutputType> {
    match value {
        "primary" => Ok(OutputType::Primary),
        "video" => Ok(OutputType::Video),
        "audio" => Ok(OutputType::Audio),
        "subtitle" => Ok(OutputType::Subtitle),
        "thumbnail" => Ok(OutputType::Thumbnail),
        "metadata" => Ok(OutputType::Metadata),
        "torrent_file" => Ok(OutputType::TorrentFile),
        "extracted_file" => Ok(OutputType::ExtractedFile),
        "converted_file" => Ok(OutputType::ConvertedFile),
        "archive" => Ok(OutputType::Archive),
        "directory" => Ok(OutputType::Directory),
        "temporary" => Ok(OutputType::Temporary),
        "other" => Ok(OutputType::Other),
        other => Err(RavynError::Internal(format!("invalid output type {other}"))),
    }
}

fn parse_output_state(value: &str) -> Result<OutputState> {
    match value {
        "planned" => Ok(OutputState::Planned),
        "creating" => Ok(OutputState::Creating),
        "ready" => Ok(OutputState::Ready),
        "failed" => Ok(OutputState::Failed),
        "deleted" => Ok(OutputState::Deleted),
        "moved" => Ok(OutputState::Moved),
        "replaced" => Ok(OutputState::Replaced),
        other => Err(RavynError::Internal(format!(
            "invalid output state {other}"
        ))),
    }
}

fn parse_output_source(value: &str) -> Result<OutputSourceKind> {
    match value {
        "http" => Ok(OutputSourceKind::Http),
        "media" => Ok(OutputSourceKind::Media),
        "torrent" => Ok(OutputSourceKind::Torrent),
        "post_process" => Ok(OutputSourceKind::PostProcess),
        other => Err(RavynError::Internal(format!(
            "invalid output source {other}"
        ))),
    }
}

pub(crate) fn row_to_torrent_record(row: SqliteRow) -> Result<TorrentRecord> {
    fn non_negative(value: i64) -> u64 {
        u64::try_from(value).unwrap_or_default()
    }
    Ok(TorrentRecord {
        job_id: Uuid::parse_str(&row.try_get::<String, _>("job_id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        torrent_id: row.try_get("torrent_id")?,
        info_hash: row.try_get("info_hash")?,
        name: row.try_get("name")?,
        state: row.try_get("state")?,
        downloaded_bytes: non_negative(row.try_get("downloaded_bytes")?),
        uploaded_bytes: non_negative(row.try_get("uploaded_bytes")?),
        total_bytes: row
            .try_get::<Option<i64>, _>("total_bytes")?
            .map(non_negative),
        download_speed_bps: non_negative(row.try_get("download_speed_bps")?),
        upload_speed_bps: non_negative(row.try_get("upload_speed_bps")?),
        peers_connected: non_negative(row.try_get("peers_connected")?),
        seeders: non_negative(row.try_get("seeders")?),
        leechers: non_negative(row.try_get("leechers")?),
        raw: serde_json::from_str(&row.try_get::<String, _>("raw_json")?)?,
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

#[cfg(test)]
mod resume_identity_tests {
    use super::*;
    use crate::{
        core::models::{DownloadOptions, DuplicatePolicy},
        storage::segments::{self, SegmentRecord},
    };

    async fn repository() -> (tempfile::TempDir, Repository) {
        let temp = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", temp.path().join("test.sqlite3").display());
        let repository = Repository::connect(&url).await.unwrap();
        (temp, repository)
    }

    async fn job(repository: &Repository) -> Job {
        repository
            .insert_job(
                CreateJob {
                    kind: JobKind::Http,
                    source: "https://example.test/file.bin".into(),
                    destination: Some(PathBuf::from("downloads")),
                    filename: Some("file.bin".into()),
                    priority: 0,
                    speed_limit_bps: None,
                    expected_sha256: None,
                    duplicate_policy: DuplicatePolicy::Allow,
                    options: DownloadOptions::default(),
                },
                PathBuf::from("downloads"),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn changed_validator_invalidates_segments_even_when_job_progress_is_zero() {
        let (_temp, repository) = repository().await;
        let job = job(&repository).await;
        assert!(
            !repository
                .set_remote_identity(
                    job.id,
                    "https://example.test/file.bin",
                    Some("\"v1\""),
                    None,
                    Some(1024),
                    None,
                )
                .await
                .unwrap()
        );
        repository
            .set_transfer_mode(job.id, "segmented")
            .await
            .unwrap();
        segments::replace(
            repository.pool(),
            job.id,
            &[SegmentRecord {
                index: 0,
                start: 0,
                end: 1023,
                downloaded: 512,
                completed: false,
            }],
        )
        .await
        .unwrap();

        let reset = repository
            .set_remote_identity(
                job.id,
                "https://example.test/file.bin",
                Some("\"v2\""),
                None,
                Some(1024),
                Some(1024),
            )
            .await
            .unwrap();
        assert!(reset);
        assert!(
            segments::list(repository.pool(), job.id)
                .await
                .unwrap()
                .is_empty()
        );
        let refreshed = repository.get_job(job.id).await.unwrap();
        assert_eq!(refreshed.downloaded_bytes, 0);
        assert_eq!(refreshed.transfer_mode, "none");
    }

    #[tokio::test]
    async fn missing_segmented_partial_file_invalidates_resume_state() {
        let (_temp, repository) = repository().await;
        let job = job(&repository).await;
        repository
            .set_remote_identity(
                job.id,
                "https://example.test/file.bin",
                Some("\"v1\""),
                None,
                Some(1024),
                None,
            )
            .await
            .unwrap();
        repository
            .set_transfer_mode(job.id, "segmented")
            .await
            .unwrap();
        segments::replace(
            repository.pool(),
            job.id,
            &[SegmentRecord {
                index: 0,
                start: 0,
                end: 1023,
                downloaded: 128,
                completed: false,
            }],
        )
        .await
        .unwrap();

        assert!(
            repository
                .set_remote_identity(
                    job.id,
                    "https://example.test/file.bin",
                    Some("\"v1\""),
                    None,
                    Some(1024),
                    None,
                )
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn idempotency_records_round_trip() {
        let (_temp, repository) = repository().await;
        let job = job(&repository).await;
        repository
            .put_idempotent_resource("create_job", "request-1", "abc", job.id)
            .await
            .unwrap();
        assert_eq!(
            repository
                .get_idempotent_resource("create_job", "request-1")
                .await
                .unwrap(),
            Some(("abc".into(), job.id.to_string()))
        );
    }

    #[tokio::test]
    async fn job_pages_are_bounded_and_cursor_stable() {
        let (_temp, repository) = repository().await;
        for _ in 0..3 {
            job(&repository).await;
        }
        let first = repository
            .list_jobs_page(JobListFilter {
                limit: 1,
                ..JobListFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(first.len(), 2);
        let second = repository
            .list_jobs_page(JobListFilter {
                cursor: Some(first[0].id),
                limit: 1,
                ..JobListFilter::default()
            })
            .await
            .unwrap();
        assert!(!second.is_empty());
        assert_ne!(first[0].id, second[0].id);
    }

    #[tokio::test]
    async fn outputs_are_registered_idempotently_and_confined() {
        let (temp, repository) = repository().await;
        let destination = temp.path().join("downloads");
        tokio::fs::create_dir_all(&destination).await.unwrap();
        let job = repository
            .insert_job(
                CreateJob {
                    kind: JobKind::Http,
                    source: "https://example.test/file.bin".into(),
                    destination: Some(destination.clone()),
                    filename: Some("file.bin".into()),
                    priority: 0,
                    speed_limit_bps: None,
                    expected_sha256: None,
                    duplicate_policy: DuplicatePolicy::Allow,
                    options: DownloadOptions::default(),
                },
                destination.clone(),
            )
            .await
            .unwrap();
        let path = destination.join("file.bin");
        tokio::fs::write(&path, b"ravyn").await.unwrap();
        let first = repository
            .register_output(&job, &path, OutputType::Primary, OutputSourceKind::Http)
            .await
            .unwrap();
        let second = repository
            .register_output(&job, &path, OutputType::Primary, OutputSourceKind::Http)
            .await
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.relative_path, PathBuf::from("file.bin"));
        assert_eq!(first.size_bytes, Some(5));
        assert_eq!(first.mime_type.as_deref(), None);
        repository
            .set_output_checksum(first.id, "sha256", "0123456789abcdef")
            .await
            .unwrap();
        let converted = destination.join("file.txt");
        tokio::fs::write(&converted, b"ravyn converted")
            .await
            .unwrap();
        let derived = repository
            .register_derived_output(
                &job,
                first.id,
                &converted,
                OutputType::ConvertedFile,
                0,
                serde_json::json!({"action": "convert_media"}),
            )
            .await
            .unwrap();
        assert_eq!(derived.parent_output_id, Some(first.id));
        assert_eq!(derived.producing_action_index, Some(0));
        assert_eq!(derived.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(repository.list_job_outputs(job.id).await.unwrap().len(), 2);
        assert!(
            repository
                .register_output(
                    &job,
                    temp.path(),
                    OutputType::Directory,
                    OutputSourceKind::Http,
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn persistent_settings_round_trip_and_reset() {
        use clap::Parser;

        let (_temp, repository) = repository().await;
        let config = crate::config::Config::parse_from(["ravyn"]);
        let mut settings = crate::config::PersistentSettings::from_config(&config);
        settings.max_active = 7;
        repository
            .save_persistent_settings(&settings)
            .await
            .unwrap();
        assert_eq!(
            repository
                .load_persistent_settings()
                .await
                .unwrap()
                .unwrap()
                .max_active,
            7
        );
        repository.reset_persistent_settings().await.unwrap();
        assert!(
            repository
                .load_persistent_settings()
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn online_backup_preserves_database_integrity() {
        let (temp, repository) = repository().await;
        job(&repository).await;
        let backup = temp.path().join("backup.sqlite3");
        repository.backup_to(&backup).await.unwrap();
        let backup_repository = Repository::connect(&format!("sqlite://{}", backup.display()))
            .await
            .unwrap();
        assert_eq!(backup_repository.integrity_check().await.unwrap(), "ok");
        assert_eq!(backup_repository.list_jobs().await.unwrap().len(), 1);
    }
}
