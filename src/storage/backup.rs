//! Online backup, integrity verification, retention, and diagnostics.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::{
    error::{RavynError, Result},
    storage::Repository,
};

impl Repository {
    pub async fn run_retention(&self, older_than: DateTime<Utc>) -> Result<serde_json::Value> {
        let mut tx = self.pool().begin().await?;
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
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(self.pool()).await?;
        Ok(())
    }

    pub async fn integrity_check(&self) -> Result<String> {
        Ok(sqlx::query("PRAGMA integrity_check")
            .fetch_one(self.pool())
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
        .fetch_one(self.pool())
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
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn job_status_counts(&self) -> Result<Vec<(String, i64)>> {
        sqlx::query("SELECT status,COUNT(*) AS count FROM jobs GROUP BY status")
            .fetch_all(self.pool())
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
        .fetch_one(self.pool())
        .await?;
        Ok((
            row.try_get("queue_depth")?,
            row.try_get("bytes_transferred")?,
            row.try_get("output_count")?,
            row.try_get("failure_count")?,
        ))
    }
}
