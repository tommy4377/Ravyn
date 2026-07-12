use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    adapters::torrent::TorrentSnapshot,
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
        if let Some(metrics) = self.metrics_handle() {
            metrics.seeding_stopped(reason);
        }
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

impl Repository {
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
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn get_torrent_record(&self, job_id: Uuid) -> Result<Option<TorrentRecord>> {
        sqlx::query("SELECT job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,updated_at FROM torrent_jobs WHERE job_id=?")
            .bind(job_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_torrent_record)
            .transpose()
    }

    pub async fn list_torrent_records(&self) -> Result<Vec<TorrentRecord>> {
        sqlx::query("SELECT job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,updated_at FROM torrent_jobs ORDER BY updated_at DESC")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_torrent_record)
            .collect()
    }

    pub async fn delete_torrent_record(&self, job_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM torrent_jobs WHERE job_id=?")
            .bind(job_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
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
