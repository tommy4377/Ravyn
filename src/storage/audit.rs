//! Job logs and the administrative audit trail.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
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
    pub previous_hash: Option<String>,
    pub entry_hash: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditChainStatus {
    pub valid: bool,
    pub chained_entries: usize,
    pub head: Option<String>,
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
        let timestamp = Utc::now();
        let metadata_json = serde_json::to_string(&metadata)?;
        let mut transaction = self.pool().begin().await?;
        sqlx::query("UPDATE audit_chain_head SET hash = hash WHERE id = 1")
            .execute(&mut *transaction)
            .await?;
        let previous_hash = sqlx::query_scalar::<_, Option<String>>(
            "SELECT hash FROM audit_chain_head WHERE id = 1",
        )
        .fetch_one(&mut *transaction)
        .await?;
        let entry_hash = audit_entry_hash(
            timestamp,
            action,
            resource_type,
            resource_id,
            outcome,
            &metadata_json,
            previous_hash.as_deref(),
        )?;
        sqlx::query("INSERT INTO audit_log(timestamp,action,resource_type,resource_id,outcome,metadata_json,previous_hash,entry_hash) VALUES(?,?,?,?,?,?,?,?)")
            .bind(timestamp)
            .bind(action)
            .bind(resource_type)
            .bind(resource_id)
            .bind(outcome)
            .bind(metadata_json)
            .bind(previous_hash)
            .bind(&entry_hash)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE audit_chain_head SET hash = ? WHERE id = 1")
            .bind(entry_hash)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn verify_audit_chain(&self) -> Result<AuditChainStatus> {
        let mut expected_previous = sqlx::query_scalar::<_, Option<String>>(
            "SELECT anchor_hash FROM audit_chain_head WHERE id = 1",
        )
        .fetch_one(self.pool())
        .await?;
        let mut rows = sqlx::query(
            "SELECT timestamp,action,resource_type,resource_id,outcome,metadata_json,previous_hash,entry_hash FROM audit_log WHERE entry_hash IS NOT NULL ORDER BY id ASC",
        )
        .fetch(self.pool());
        let mut valid = true;
        let mut chained_entries = 0usize;
        while let Some(row) = rows.try_next().await? {
            chained_entries = chained_entries
                .checked_add(1)
                .ok_or_else(|| RavynError::Internal("audit chain entry count overflowed".into()))?;
            let timestamp: DateTime<Utc> = row.try_get("timestamp")?;
            let action: String = row.try_get("action")?;
            let resource_type: String = row.try_get("resource_type")?;
            let resource_id: Option<String> = row.try_get("resource_id")?;
            let outcome: String = row.try_get("outcome")?;
            let metadata_json: String = row.try_get("metadata_json")?;
            let previous_hash: Option<String> = row.try_get("previous_hash")?;
            let entry_hash: String = row.try_get("entry_hash")?;
            let computed = audit_entry_hash(
                timestamp,
                &action,
                &resource_type,
                resource_id.as_deref(),
                &outcome,
                &metadata_json,
                expected_previous.as_deref(),
            )?;
            if previous_hash != expected_previous || entry_hash != computed {
                valid = false;
                break;
            }
            expected_previous = Some(entry_hash);
        }
        drop(rows);
        let head = sqlx::query_scalar::<_, Option<String>>(
            "SELECT hash FROM audit_chain_head WHERE id = 1",
        )
        .fetch_one(self.pool())
        .await?;
        valid &= head == expected_previous;
        Ok(AuditChainStatus {
            valid,
            chained_entries,
            head,
        })
    }

    pub async fn list_audit(&self, limit: usize) -> Result<Vec<AuditRecord>> {
        sqlx::query("SELECT id,timestamp,action,resource_type,resource_id,outcome,metadata_json,previous_hash,entry_hash FROM audit_log ORDER BY timestamp DESC,id DESC LIMIT ?")
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

fn audit_entry_hash(
    timestamp: DateTime<Utc>,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    outcome: &str,
    metadata_json: &str,
    previous_hash: Option<&str>,
) -> Result<String> {
    let canonical = serde_json::to_vec(&(
        timestamp.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        action,
        resource_type,
        resource_id,
        outcome,
        metadata_json,
        previous_hash,
    ))?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
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
        previous_hash: row.try_get("previous_hash")?,
        entry_hash: row.try_get("entry_hash")?,
    })
}
