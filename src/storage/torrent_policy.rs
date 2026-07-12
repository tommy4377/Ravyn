use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    storage::Repository,
};

#[derive(Debug, Clone, Serialize)]
pub struct TorrentSeedingState {
    pub job_id: Uuid,
    pub torrent_id: String,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub stop_reason: Option<String>,
    pub last_ratio: Option<f64>,
    pub updated_at: DateTime<Utc>,
}

impl Repository {
    pub async fn begin_torrent_seeding(
        &self,
        job_id: Uuid,
        torrent_id: &str,
    ) -> Result<TorrentSeedingState> {
        let now = Utc::now();
        sqlx::query("INSERT INTO torrent_seeding_state(job_id,torrent_id,started_at,stopped_at,stop_reason,last_ratio,updated_at) VALUES(?,?,?,NULL,NULL,NULL,?) ON CONFLICT(job_id) DO UPDATE SET torrent_id=excluded.torrent_id,started_at=CASE WHEN torrent_seeding_state.stopped_at IS NULL THEN torrent_seeding_state.started_at ELSE excluded.started_at END,stopped_at=NULL,stop_reason=NULL,updated_at=excluded.updated_at")
            .bind(job_id.to_string())
            .bind(torrent_id)
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await?;
        self.get_torrent_seeding_state(job_id)
            .await?
            .ok_or_else(|| RavynError::Internal("seeding state was not persisted".into()))
    }

    pub async fn update_torrent_seeding_ratio(
        &self,
        job_id: Uuid,
        ratio: Option<f64>,
    ) -> Result<()> {
        sqlx::query("UPDATE torrent_seeding_state SET last_ratio=?,updated_at=? WHERE job_id=?")
            .bind(ratio)
            .bind(Utc::now())
            .bind(job_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn stop_torrent_seeding(
        &self,
        job_id: Uuid,
        reason: &str,
        ratio: Option<f64>,
    ) -> Result<()> {
        if reason.trim().is_empty() || reason.len() > 256 {
            return Err(RavynError::Invalid(
                "torrent seeding stop reason is invalid".into(),
            ));
        }
        sqlx::query("UPDATE torrent_seeding_state SET stopped_at=?,stop_reason=?,last_ratio=?,updated_at=? WHERE job_id=?")
            .bind(Utc::now())
            .bind(reason)
            .bind(ratio)
            .bind(Utc::now())
            .bind(job_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn get_torrent_seeding_state(
        &self,
        job_id: Uuid,
    ) -> Result<Option<TorrentSeedingState>> {
        sqlx::query("SELECT job_id,torrent_id,started_at,stopped_at,stop_reason,last_ratio,updated_at FROM torrent_seeding_state WHERE job_id=?")
            .bind(job_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(|row| {
                Ok(TorrentSeedingState {
                    job_id: Uuid::parse_str(&row.try_get::<String, _>("job_id")?)
                        .map_err(|error| RavynError::Internal(error.to_string()))?,
                    torrent_id: row.try_get("torrent_id")?,
                    started_at: row.try_get("started_at")?,
                    stopped_at: row.try_get("stopped_at")?,
                    stop_reason: row.try_get("stop_reason")?,
                    last_ratio: row.try_get("last_ratio")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .transpose()
    }
}
