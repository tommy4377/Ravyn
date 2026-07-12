//! Job logs and the administrative audit trail.

use chrono::{DateTime, Utc};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    storage::{Repository, jobs::row_uuid},
};

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

impl Repository {
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
            .bind(code).bind(message).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn list_job_logs(&self, job_id: Uuid, limit: usize) -> Result<Vec<JobLogRecord>> {
        self.get_job(job_id).await?;
        sqlx::query("SELECT id,job_id,timestamp,source_module,severity,code,message,metadata_json FROM job_logs WHERE job_id=? ORDER BY timestamp DESC,id DESC LIMIT ?")
            .bind(job_id.to_string()).bind(i64::try_from(limit.clamp(1, 500)).unwrap_or(500))
            .fetch_all(self.pool()).await?.into_iter().map(row_to_job_log).collect()
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
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn list_audit(&self, limit: usize) -> Result<Vec<AuditRecord>> {
        sqlx::query("SELECT id,timestamp,action,resource_type,resource_id,outcome,metadata_json FROM audit_log ORDER BY timestamp DESC,id DESC LIMIT ?")
            .bind(i64::try_from(limit.clamp(1, 500)).unwrap_or(500))
            .fetch_all(self.pool()).await?.into_iter().map(row_to_audit).collect()
    }
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
