use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{RavynError, Result};

fn sqlite_i64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| RavynError::Invalid(format!("{label} exceeds SQLite integer range")))
}

fn usize_i64(value: usize, label: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| RavynError::Invalid(format!("{label} exceeds SQLite integer range")))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SegmentRecord {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
    pub completed: bool,
}

pub async fn replace(pool: &SqlitePool, job_id: Uuid, segments: &[SegmentRecord]) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM job_segments WHERE job_id=?")
        .bind(job_id.to_string())
        .execute(&mut *tx)
        .await?;
    for segment in segments {
        sqlx::query("INSERT INTO job_segments(job_id,segment_index,start_byte,end_byte,downloaded_bytes,completed,updated_at) VALUES(?,?,?,?,?,?,?)")
            .bind(job_id.to_string())
            .bind(usize_i64(segment.index, "segment index")?)
            .bind(sqlite_i64(segment.start, "segment start")?)
            .bind(sqlite_i64(segment.end, "segment end")?)
            .bind(sqlite_i64(segment.downloaded, "downloaded bytes")?)
            .bind(if segment.completed { 1_i64 } else { 0_i64 })
            .bind(Utc::now())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool, job_id: Uuid) -> Result<Vec<SegmentRecord>> {
    let rows = sqlx::query("SELECT segment_index,start_byte,end_byte,downloaded_bytes,completed FROM job_segments WHERE job_id=? ORDER BY segment_index")
        .bind(job_id.to_string())
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(row_to_segment).collect()
}

pub async fn list_page(
    pool: &SqlitePool,
    job_id: Uuid,
    offset: u64,
    limit: usize,
) -> Result<Vec<SegmentRecord>> {
    let offset = i64::try_from(offset)
        .map_err(|_| RavynError::Invalid("pagination cursor is too large".into()))?;
    let limit = i64::try_from(limit.clamp(1, 201))
        .map_err(|_| RavynError::Invalid("pagination limit is invalid".into()))?;
    let rows = sqlx::query("SELECT segment_index,start_byte,end_byte,downloaded_bytes,completed FROM job_segments WHERE job_id=? ORDER BY segment_index LIMIT ? OFFSET ?")
        .bind(job_id.to_string())
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(row_to_segment).collect()
}

fn row_to_segment(row: sqlx::sqlite::SqliteRow) -> Result<SegmentRecord> {
    Ok(SegmentRecord {
        index: usize::try_from(row.get::<i64, _>("segment_index"))
            .map_err(|_| RavynError::Internal("invalid segment index in database".into()))?,
        start: u64::try_from(row.get::<i64, _>("start_byte"))
            .map_err(|_| RavynError::Internal("invalid segment start in database".into()))?,
        end: u64::try_from(row.get::<i64, _>("end_byte"))
            .map_err(|_| RavynError::Internal("invalid segment end in database".into()))?,
        downloaded: u64::try_from(row.get::<i64, _>("downloaded_bytes"))
            .map_err(|_| RavynError::Internal("invalid segment progress in database".into()))?,
        completed: row.get::<i64, _>("completed") != 0,
    })
}

pub async fn update(
    pool: &SqlitePool,
    job_id: Uuid,
    index: usize,
    downloaded: u64,
    completed: bool,
) -> Result<()> {
    sqlx::query("UPDATE job_segments SET downloaded_bytes=?,completed=?,updated_at=? WHERE job_id=? AND segment_index=?")
        .bind(sqlite_i64(downloaded, "downloaded bytes")?)
        .bind(if completed { 1_i64 } else { 0_i64 })
        .bind(Utc::now())
        .bind(job_id.to_string())
        .bind(usize_i64(index, "segment index")?)
        .execute(pool)
        .await?;
    Ok(())
}
