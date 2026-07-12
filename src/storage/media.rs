use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::JobOutput,
    error::{RavynError, Result},
};

use super::Repository;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItemDescriptor {
    pub item_key: String,
    pub extractor: Option<String>,
    pub media_id: Option<String>,
    pub title: Option<String>,
    pub webpage_url: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_index: Option<u64>,
    pub playlist_count: Option<u64>,
    pub extension: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItemRecord {
    pub id: Uuid,
    pub job_id: Uuid,
    pub item_key: String,
    pub extractor: Option<String>,
    pub media_id: Option<String>,
    pub title: Option<String>,
    pub webpage_url: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub playlist_index: Option<u64>,
    pub playlist_count: Option<u64>,
    pub extension: Option<String>,
    pub state: String,
    pub output_path: Option<PathBuf>,
    pub output_id: Option<Uuid>,
    pub retry_job_id: Option<Uuid>,
    pub error: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItemOutputRecord {
    pub media_item_id: Uuid,
    pub role: String,
    pub output: JobOutput,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItemSummary {
    pub job_id: Uuid,
    pub total: u64,
    pub planned: u64,
    pub downloading: u64,
    pub completed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub retried: u64,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub declared_playlist_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaArchiveRecord {
    pub extractor: String,
    pub media_id: String,
    pub first_job_id: Option<Uuid>,
    pub last_job_id: Option<Uuid>,
    pub last_output_id: Option<Uuid>,
    pub webpage_url: Option<String>,
    pub downloaded_at: DateTime<Utc>,
    pub metadata: Value,
}

impl Repository {
    pub async fn observe_media_item(
        &self,
        job_id: Uuid,
        descriptor: &MediaItemDescriptor,
    ) -> Result<MediaItemRecord> {
        validate_descriptor(descriptor)?;
        self.get_job(job_id).await?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO media_items(id,job_id,item_key,extractor,media_id,title,webpage_url,playlist_id,playlist_title,playlist_index,playlist_count,extension,state,error,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?, 'planned',NULL,?,?,?) ON CONFLICT(job_id,item_key) DO UPDATE SET extractor=excluded.extractor,media_id=excluded.media_id,title=excluded.title,webpage_url=excluded.webpage_url,playlist_id=excluded.playlist_id,playlist_title=excluded.playlist_title,playlist_index=excluded.playlist_index,playlist_count=excluded.playlist_count,extension=excluded.extension,state=CASE WHEN media_items.state='completed' THEN media_items.state ELSE 'planned' END,error=CASE WHEN media_items.state='completed' THEN media_items.error ELSE NULL END,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at",
        )
        .bind(id.to_string())
        .bind(job_id.to_string())
        .bind(&descriptor.item_key)
        .bind(&descriptor.extractor)
        .bind(&descriptor.media_id)
        .bind(&descriptor.title)
        .bind(&descriptor.webpage_url)
        .bind(&descriptor.playlist_id)
        .bind(&descriptor.playlist_title)
        .bind(optional_u64_to_i64(descriptor.playlist_index, "playlist index")?)
        .bind(optional_u64_to_i64(descriptor.playlist_count, "playlist count")?)
        .bind(&descriptor.extension)
        .bind(serde_json::to_string(&descriptor.metadata)?)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        self.get_media_item_by_key(job_id, &descriptor.item_key)
            .await
    }

    pub async fn begin_media_item(
        &self,
        job_id: Uuid,
        descriptor: &MediaItemDescriptor,
    ) -> Result<MediaItemRecord> {
        validate_descriptor(descriptor)?;
        self.get_job(job_id).await?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO media_items(id,job_id,item_key,extractor,media_id,title,webpage_url,playlist_id,playlist_title,playlist_index,playlist_count,extension,state,error,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?, 'downloading',NULL,?,?,?) ON CONFLICT(job_id,item_key) DO UPDATE SET extractor=excluded.extractor,media_id=excluded.media_id,title=excluded.title,webpage_url=excluded.webpage_url,playlist_id=excluded.playlist_id,playlist_title=excluded.playlist_title,playlist_index=excluded.playlist_index,playlist_count=excluded.playlist_count,extension=excluded.extension,state='downloading',error=NULL,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at",
        )
        .bind(id.to_string())
        .bind(job_id.to_string())
        .bind(&descriptor.item_key)
        .bind(&descriptor.extractor)
        .bind(&descriptor.media_id)
        .bind(&descriptor.title)
        .bind(&descriptor.webpage_url)
        .bind(&descriptor.playlist_id)
        .bind(&descriptor.playlist_title)
        .bind(optional_u64_to_i64(descriptor.playlist_index, "playlist index")?)
        .bind(optional_u64_to_i64(descriptor.playlist_count, "playlist count")?)
        .bind(&descriptor.extension)
        .bind(serde_json::to_string(&descriptor.metadata)?)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        self.get_media_item_by_key(job_id, &descriptor.item_key)
            .await
    }

    pub async fn complete_media_item(
        &self,
        job_id: Uuid,
        descriptor: &MediaItemDescriptor,
        output_path: &Path,
    ) -> Result<MediaItemRecord> {
        validate_descriptor(descriptor)?;
        let now = Utc::now();
        let item = self.begin_media_item(job_id, descriptor).await?;
        let metadata_json = serde_json::to_string(&descriptor.metadata)?;
        let mut transaction = self.pool().begin().await?;
        sqlx::query("UPDATE media_items SET state='completed',output_path=?,error=NULL,updated_at=? WHERE id=?")
            .bind(output_path.to_string_lossy().to_string())
            .bind(now)
            .bind(item.id.to_string())
            .execute(&mut *transaction)
            .await?;

        if let (Some(extractor), Some(media_id)) = (
            descriptor
                .extractor
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            descriptor
                .media_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        ) {
            sqlx::query("INSERT INTO media_archive(extractor,media_id,first_job_id,last_job_id,last_output_id,webpage_url,downloaded_at,metadata_json) VALUES(?,?,?,?,NULL,?,?,?) ON CONFLICT(extractor,media_id) DO UPDATE SET last_job_id=excluded.last_job_id,webpage_url=COALESCE(excluded.webpage_url,media_archive.webpage_url),downloaded_at=excluded.downloaded_at,metadata_json=excluded.metadata_json")
                .bind(extractor)
                .bind(media_id)
                .bind(job_id.to_string())
                .bind(job_id.to_string())
                .bind(&descriptor.webpage_url)
                .bind(now)
                .bind(metadata_json)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        self.get_media_item(item.id).await
    }

    pub async fn mark_unfinished_media_items_failed(
        &self,
        job_id: Uuid,
        error: &str,
    ) -> Result<u64> {
        let result = sqlx::query("UPDATE media_items SET state='failed',error=?,updated_at=? WHERE job_id=? AND state IN ('planned','downloading')")
            .bind(truncate_error(error))
            .bind(Utc::now())
            .bind(job_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn finalize_media_items_after_success(&self, job_id: Uuid) -> Result<(u64, u64)> {
        let now = Utc::now();
        let mut transaction = self.pool().begin().await?;
        let skipped = sqlx::query("UPDATE media_items SET state='skipped',error=NULL,updated_at=? WHERE job_id=? AND state='planned'")
            .bind(now)
            .bind(job_id.to_string())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        let failed = sqlx::query("UPDATE media_items SET state='failed',error='yt-dlp exited successfully without reporting item completion',updated_at=? WHERE job_id=? AND state='downloading'")
            .bind(now)
            .bind(job_id.to_string())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        transaction.commit().await?;
        Ok((skipped, failed))
    }

    pub async fn link_media_item_output(
        &self,
        job_id: Uuid,
        output_path: &Path,
        output_id: Uuid,
    ) -> Result<()> {
        let path = output_path.to_string_lossy().to_string();
        let now = Utc::now();
        let mut transaction = self.pool().begin().await?;
        let linked = sqlx::query(
            "UPDATE media_items SET output_id=?,updated_at=? WHERE job_id=? AND output_path=?",
        )
        .bind(output_id.to_string())
        .bind(now)
        .bind(job_id.to_string())
        .bind(&path)
        .execute(&mut *transaction)
        .await?;
        if linked.rows_affected() > 0 {
            sqlx::query("UPDATE media_items SET output_path=?,output_id=?,updated_at=? WHERE retry_job_id=?")
                .bind(&path)
                .bind(output_id.to_string())
                .bind(now)
                .bind(job_id.to_string())
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("UPDATE media_archive SET last_output_id=? WHERE last_job_id=? AND EXISTS(SELECT 1 FROM media_items WHERE media_items.job_id=? AND media_items.output_path=? AND media_items.extractor=media_archive.extractor AND media_items.media_id=media_archive.media_id)")
            .bind(output_id.to_string())
            .bind(job_id.to_string())
            .bind(job_id.to_string())
            .bind(path)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn link_media_item_artifact(
        &self,
        job_id: Uuid,
        item_key: &str,
        output_id: Uuid,
        role: &str,
    ) -> Result<()> {
        let role = normalize_media_output_role(role)?;
        let item = self.get_media_item_by_key(job_id, item_key).await?;
        let now = Utc::now();
        let mut transaction = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO media_item_outputs(media_item_id,output_id,role,created_at) VALUES(?,?,?,?) ON CONFLICT(media_item_id,output_id) DO UPDATE SET role=excluded.role",
        )
        .bind(item.id.to_string())
        .bind(output_id.to_string())
        .bind(role)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        if role == "primary" {
            sqlx::query("UPDATE media_items SET output_id=?,updated_at=? WHERE id=?")
                .bind(output_id.to_string())
                .bind(now)
                .bind(item.id.to_string())
                .execute(&mut *transaction)
                .await?;
            if let (Some(extractor), Some(media_id)) =
                (item.extractor.as_deref(), item.media_id.as_deref())
            {
                sqlx::query(
                    "UPDATE media_archive SET last_output_id=? WHERE extractor=? AND media_id=?",
                )
                .bind(output_id.to_string())
                .bind(extractor)
                .bind(media_id)
                .execute(&mut *transaction)
                .await?;
            }
        }

        // A retry job owns its own media-item row, but the resulting artifacts
        // must also be attached to the original failed item in the parent
        // playlist. This keeps auxiliary files and the primary output visible
        // through the parent job and allows the parent status to reconcile.
        if let Some(parent_row) = sqlx::query(
            "SELECT id,extractor,media_id FROM media_items WHERE retry_job_id=? ORDER BY created_at,id LIMIT 1",
        )
        .bind(job_id.to_string())
        .fetch_optional(&mut *transaction)
        .await?
        {
            let parent_item_id: String = parent_row.try_get("id")?;
            sqlx::query(
                "INSERT INTO media_item_outputs(media_item_id,output_id,role,created_at) VALUES(?,?,?,?) ON CONFLICT(media_item_id,output_id) DO UPDATE SET role=excluded.role",
            )
            .bind(&parent_item_id)
            .bind(output_id.to_string())
            .bind(role)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            if role == "primary" {
                sqlx::query("UPDATE media_items SET state='completed',output_id=?,error=NULL,updated_at=? WHERE id=?")
                    .bind(output_id.to_string())
                    .bind(now)
                    .bind(&parent_item_id)
                    .execute(&mut *transaction)
                    .await?;
                let extractor: Option<String> = parent_row.try_get("extractor")?;
                let media_id: Option<String> = parent_row.try_get("media_id")?;
                if let (Some(extractor), Some(media_id)) = (extractor, media_id) {
                    sqlx::query("UPDATE media_archive SET last_output_id=?,last_job_id=?,downloaded_at=? WHERE extractor=? AND media_id=?")
                        .bind(output_id.to_string())
                        .bind(job_id.to_string())
                        .bind(now)
                        .bind(extractor)
                        .bind(media_id)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn list_media_item_outputs(
        &self,
        job_id: Uuid,
        item_id: Uuid,
    ) -> Result<Vec<MediaItemOutputRecord>> {
        self.get_job_media_item(job_id, item_id).await?;
        let rows = sqlx::query(
            "SELECT mio.media_item_id,mio.role,mio.created_at,jo.* FROM media_item_outputs mio JOIN job_outputs jo ON jo.id=mio.output_id WHERE mio.media_item_id=? ORDER BY CASE mio.role WHEN 'primary' THEN 0 ELSE 1 END,mio.created_at,jo.id",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(MediaItemOutputRecord {
                    media_item_id: item_id,
                    role: row.try_get("role")?,
                    created_at: row.try_get("created_at")?,
                    output: super::repository::row_to_output(row)?,
                })
            })
            .collect()
    }

    pub async fn complete_media_retry_parent(&self, retry_job_id: Uuid) -> Result<Option<Uuid>> {
        let mut transaction = self.pool().begin().await?;
        let parent_job_id = sqlx::query_scalar::<_, String>(
            "SELECT job_id FROM media_items WHERE retry_job_id=? ORDER BY created_at,id LIMIT 1",
        )
        .bind(retry_job_id.to_string())
        .fetch_optional(&mut *transaction)
        .await?;
        let updated = sqlx::query("UPDATE media_items SET state='completed',error=NULL,updated_at=? WHERE retry_job_id=? AND output_id IS NOT NULL")
            .bind(Utc::now())
            .bind(retry_job_id.to_string())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        transaction.commit().await?;
        if updated == 0 {
            return Ok(None);
        }
        parent_job_id
            .map(|value| {
                Uuid::parse_str(&value).map_err(|error| RavynError::Internal(error.to_string()))
            })
            .transpose()
    }

    pub async fn mark_media_retry_parent_failed(
        &self,
        retry_job_id: Uuid,
        error: &str,
    ) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE media_items SET state='failed',error=?,updated_at=? WHERE retry_job_id=?",
        )
        .bind(truncate_error(error))
        .bind(Utc::now())
        .bind(retry_job_id.to_string())
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn get_media_item(&self, item_id: Uuid) -> Result<MediaItemRecord> {
        sqlx::query("SELECT * FROM media_items WHERE id=?")
            .bind(item_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_media_item)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("media item {item_id}")))
    }

    pub async fn get_job_media_item(&self, job_id: Uuid, item_id: Uuid) -> Result<MediaItemRecord> {
        sqlx::query("SELECT * FROM media_items WHERE id=? AND job_id=?")
            .bind(item_id.to_string())
            .bind(job_id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_media_item)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("media item {item_id} for job {job_id}")))
    }

    async fn get_media_item_by_key(&self, job_id: Uuid, item_key: &str) -> Result<MediaItemRecord> {
        sqlx::query("SELECT * FROM media_items WHERE job_id=? AND item_key=?")
            .bind(job_id.to_string())
            .bind(item_key)
            .fetch_optional(self.pool())
            .await?
            .map(row_to_media_item)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("media item {item_key}")))
    }

    pub async fn set_media_item_retry_job(&self, item_id: Uuid, retry_job_id: Uuid) -> Result<()> {
        let result = sqlx::query("UPDATE media_items SET retry_job_id=?,updated_at=? WHERE id=?")
            .bind(retry_job_id.to_string())
            .bind(Utc::now())
            .bind(item_id.to_string())
            .execute(self.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(RavynError::NotFound(format!("media item {item_id}")));
        }
        Ok(())
    }

    pub async fn media_item_summary(&self, job_id: Uuid) -> Result<MediaItemSummary> {
        self.get_job(job_id).await?;
        let row = sqlx::query(
            "SELECT COUNT(*) AS total,             COALESCE(SUM(CASE WHEN state='planned' THEN 1 ELSE 0 END),0) AS planned,             COALESCE(SUM(CASE WHEN state='downloading' THEN 1 ELSE 0 END),0) AS downloading,             COALESCE(SUM(CASE WHEN state='completed' THEN 1 ELSE 0 END),0) AS completed,             COALESCE(SUM(CASE WHEN state='failed' THEN 1 ELSE 0 END),0) AS failed,             COALESCE(SUM(CASE WHEN state='skipped' THEN 1 ELSE 0 END),0) AS skipped,             COALESCE(SUM(CASE WHEN retry_job_id IS NOT NULL THEN 1 ELSE 0 END),0) AS retried,             MAX(playlist_id) AS playlist_id,MAX(playlist_title) AS playlist_title,             MAX(playlist_count) AS playlist_count FROM media_items WHERE job_id=?",
        )
        .bind(job_id.to_string())
        .fetch_one(self.pool())
        .await?;
        Ok(MediaItemSummary {
            job_id,
            total: non_negative_i64_to_u64(row.try_get("total")?, "media item count")?,
            planned: non_negative_i64_to_u64(row.try_get("planned")?, "planned item count")?,
            downloading: non_negative_i64_to_u64(
                row.try_get("downloading")?,
                "downloading item count",
            )?,
            completed: non_negative_i64_to_u64(row.try_get("completed")?, "completed item count")?,
            failed: non_negative_i64_to_u64(row.try_get("failed")?, "failed item count")?,
            skipped: non_negative_i64_to_u64(row.try_get("skipped")?, "skipped item count")?,
            retried: non_negative_i64_to_u64(row.try_get("retried")?, "retried item count")?,
            playlist_id: row.try_get("playlist_id")?,
            playlist_title: row.try_get("playlist_title")?,
            declared_playlist_count: optional_i64_to_u64(
                row.try_get("playlist_count")?,
                "playlist count",
            )?,
        })
    }

    pub async fn list_failed_media_items(
        &self,
        job_id: Uuid,
        limit: usize,
    ) -> Result<Vec<MediaItemRecord>> {
        self.get_job(job_id).await?;
        let limit = i64::try_from(limit.clamp(1, 500))
            .map_err(|_| RavynError::Invalid("media retry limit is invalid".into()))?;
        sqlx::query("SELECT * FROM media_items WHERE job_id=? AND state='failed' ORDER BY COALESCE(playlist_index,9223372036854775807),created_at,id LIMIT ?")
            .bind(job_id.to_string())
            .bind(limit)
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_media_item)
            .collect()
    }

    pub async fn list_media_items_page(
        &self,
        job_id: Uuid,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<MediaItemRecord>> {
        self.get_job(job_id).await?;
        let limit = i64::try_from(limit.clamp(1, 201))
            .map_err(|_| RavynError::Invalid("pagination limit is invalid".into()))?;
        let offset = i64::try_from(offset)
            .map_err(|_| RavynError::Invalid("pagination cursor is too large".into()))?;
        let search = search
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{value}%"));
        let rows = if let Some(search) = search {
            sqlx::query("SELECT * FROM media_items WHERE job_id=? AND (COALESCE(title,'') LIKE ? OR COALESCE(media_id,'') LIKE ? OR COALESCE(webpage_url,'') LIKE ? OR state LIKE ?) ORDER BY COALESCE(playlist_index,9223372036854775807),created_at,id LIMIT ? OFFSET ?")
                .bind(job_id.to_string())
                .bind(&search)
                .bind(&search)
                .bind(&search)
                .bind(&search)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.pool())
                .await?
        } else {
            sqlx::query("SELECT * FROM media_items WHERE job_id=? ORDER BY COALESCE(playlist_index,9223372036854775807),created_at,id LIMIT ? OFFSET ?")
                .bind(job_id.to_string())
                .bind(limit)
                .bind(offset)
                .fetch_all(self.pool())
                .await?
        };
        rows.into_iter().map(row_to_media_item).collect()
    }

    pub async fn list_media_archive_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<MediaArchiveRecord>> {
        let limit = i64::try_from(limit.clamp(1, 201))
            .map_err(|_| RavynError::Invalid("pagination limit is invalid".into()))?;
        let offset = i64::try_from(offset)
            .map_err(|_| RavynError::Invalid("pagination cursor is too large".into()))?;
        let search = search
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("%{value}%"));
        let rows = if let Some(search) = search {
            sqlx::query("SELECT * FROM media_archive WHERE extractor LIKE ? OR media_id LIKE ? OR COALESCE(webpage_url,'') LIKE ? ORDER BY downloaded_at DESC,extractor,media_id LIMIT ? OFFSET ?")
                .bind(&search)
                .bind(&search)
                .bind(&search)
                .bind(limit)
                .bind(offset)
                .fetch_all(self.pool())
                .await?
        } else {
            sqlx::query("SELECT * FROM media_archive ORDER BY downloaded_at DESC,extractor,media_id LIMIT ? OFFSET ?")
                .bind(limit)
                .bind(offset)
                .fetch_all(self.pool())
                .await?
        };
        rows.into_iter().map(row_to_media_archive).collect()
    }

    pub async fn remove_media_archive_entry(&self, extractor: &str, media_id: &str) -> Result<()> {
        if !archive_token_is_safe(extractor) || !archive_token_is_safe(media_id) {
            return Err(RavynError::Invalid(
                "media archive extractor and id must be non-empty single-line tokens".into(),
            ));
        }
        let result = sqlx::query("DELETE FROM media_archive WHERE extractor=? AND media_id=?")
            .bind(extractor)
            .bind(media_id)
            .execute(self.pool())
            .await?;
        if result.rows_affected() == 0 {
            return Err(RavynError::NotFound(format!(
                "media archive entry {extractor}:{media_id}"
            )));
        }
        Ok(())
    }

    pub async fn export_media_archive(&self, destination: &Path) -> Result<()> {
        let rows =
            sqlx::query("SELECT extractor,media_id FROM media_archive ORDER BY extractor,media_id")
                .fetch_all(self.pool())
                .await?;
        let mut output = String::new();
        for row in rows {
            let extractor: String = row.try_get("extractor")?;
            let media_id: String = row.try_get("media_id")?;
            if archive_token_is_safe(&extractor) && archive_token_is_safe(&media_id) {
                output.push_str(&extractor);
                output.push(' ');
                output.push_str(&media_id);
                output.push('\n');
            }
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let temporary = destination.with_extension("tmp");
        tokio::fs::write(&temporary, output).await?;
        if tokio::fs::try_exists(destination).await? {
            tokio::fs::remove_file(destination).await?;
        }
        tokio::fs::rename(temporary, destination).await?;
        Ok(())
    }
}

fn normalize_media_output_role(role: &str) -> Result<&str> {
    let role = role.trim();
    match role {
        "primary" | "video" | "audio" | "subtitle" | "thumbnail" | "metadata" | "description"
        | "chapter" | "auxiliary" => Ok(role),
        _ => Err(RavynError::Invalid(format!(
            "unsupported media output role: {role}"
        ))),
    }
}

fn validate_descriptor(descriptor: &MediaItemDescriptor) -> Result<()> {
    if descriptor.item_key.trim().is_empty() || descriptor.item_key.len() > 1024 {
        return Err(RavynError::Invalid(
            "media item key must contain between 1 and 1024 characters".into(),
        ));
    }
    for (name, value, maximum) in [
        ("extractor", descriptor.extractor.as_deref(), 256),
        ("media id", descriptor.media_id.as_deref(), 512),
        ("title", descriptor.title.as_deref(), 4096),
        ("webpage URL", descriptor.webpage_url.as_deref(), 8192),
        ("playlist id", descriptor.playlist_id.as_deref(), 512),
        ("playlist title", descriptor.playlist_title.as_deref(), 4096),
        ("extension", descriptor.extension.as_deref(), 32),
    ] {
        if let Some(value) = value {
            if value.len() > maximum || value.chars().any(|character| character == '\0') {
                return Err(RavynError::Invalid(format!("media {name} is invalid")));
            }
        }
    }
    Ok(())
}

fn optional_u64_to_i64(value: Option<u64>, field: &str) -> Result<Option<i64>> {
    value
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| RavynError::Invalid(format!("{field} exceeds SQLite range")))
        })
        .transpose()
}

fn non_negative_i64_to_u64(value: i64, field: &str) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| RavynError::Internal(format!("{field} stored a negative value")))
}

fn optional_i64_to_u64(value: Option<i64>, field: &str) -> Result<Option<u64>> {
    value
        .map(|value| non_negative_i64_to_u64(value, field))
        .transpose()
}

fn archive_token_is_safe(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

fn truncate_error(error: &str) -> String {
    const LIMIT: usize = 4096;
    error.chars().take(LIMIT).collect()
}

fn row_to_media_item(row: SqliteRow) -> Result<MediaItemRecord> {
    Ok(MediaItemRecord {
        id: parse_optional_uuid(Some(row.try_get::<String, _>("id")?))?
            .ok_or_else(|| RavynError::Internal("media item id is missing".into()))?,
        job_id: parse_optional_uuid(Some(row.try_get::<String, _>("job_id")?))?
            .ok_or_else(|| RavynError::Internal("media item job id is missing".into()))?,
        item_key: row.try_get("item_key")?,
        extractor: row.try_get("extractor")?,
        media_id: row.try_get("media_id")?,
        title: row.try_get("title")?,
        webpage_url: row.try_get("webpage_url")?,
        playlist_id: row.try_get("playlist_id")?,
        playlist_title: row.try_get("playlist_title")?,
        playlist_index: non_negative_optional(row.try_get("playlist_index")?),
        playlist_count: non_negative_optional(row.try_get("playlist_count")?),
        extension: row.try_get("extension")?,
        state: row.try_get("state")?,
        output_path: row
            .try_get::<Option<String>, _>("output_path")?
            .map(Into::into),
        output_id: parse_optional_uuid(row.try_get("output_id")?)?,
        retry_job_id: parse_optional_uuid(row.try_get("retry_job_id")?)?,
        error: row.try_get("error")?,
        metadata: serde_json::from_str(&row.try_get::<String, _>("metadata_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_media_archive(row: SqliteRow) -> Result<MediaArchiveRecord> {
    Ok(MediaArchiveRecord {
        extractor: row.try_get("extractor")?,
        media_id: row.try_get("media_id")?,
        first_job_id: parse_optional_uuid(row.try_get("first_job_id")?)?,
        last_job_id: parse_optional_uuid(row.try_get("last_job_id")?)?,
        last_output_id: parse_optional_uuid(row.try_get("last_output_id")?)?,
        webpage_url: row.try_get("webpage_url")?,
        downloaded_at: row.try_get("downloaded_at")?,
        metadata: serde_json::from_str(&row.try_get::<String, _>("metadata_json")?)?,
    })
}

fn parse_optional_uuid(value: Option<String>) -> Result<Option<Uuid>> {
    value
        .map(|value| {
            Uuid::parse_str(&value).map_err(|error| RavynError::Internal(error.to_string()))
        })
        .transpose()
}

fn non_negative_optional(value: Option<i64>) -> Option<u64> {
    value.map(|value| u64::try_from(value.max(0)).unwrap_or_default())
}
