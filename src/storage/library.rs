//! Persistent records for downloaded and imported library items.

use std::{path::PathBuf, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    services::library::LibraryCategory,
    storage::Repository,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryEntryState {
    Active,
    Trashed,
    Missing,
}

impl LibraryEntryState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Trashed => "trashed",
            Self::Missing => "missing",
        }
    }
}

impl FromStr for LibraryEntryState {
    type Err = RavynError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "trashed" => Ok(Self::Trashed),
            "missing" => Ok(Self::Missing),
            _ => Err(RavynError::Internal(format!(
                "database contains unknown library state {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: Uuid,
    pub job_id: Option<Uuid>,
    pub source_url: String,
    pub mirrors: Vec<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub path: PathBuf,
    pub filename: String,
    pub category: LibraryCategory,
    pub mime_type: Option<String>,
    pub media_metadata: serde_json::Value,
    pub torrent_metadata: serde_json::Value,
    pub tags: Vec<String>,
    pub trust: Option<serde_json::Value>,
    pub state: LibraryEntryState,
    pub trash_path: Option<PathBuf>,
    pub imported: bool,
    pub downloaded_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewLibraryEntry {
    pub job_id: Option<Uuid>,
    pub source_url: String,
    pub mirrors: Vec<String>,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub path: PathBuf,
    pub filename: String,
    pub category: LibraryCategory,
    pub mime_type: Option<String>,
    pub media_metadata: serde_json::Value,
    pub torrent_metadata: serde_json::Value,
    pub tags: Vec<String>,
    pub trust: Option<serde_json::Value>,
    pub imported: bool,
    pub downloaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryListFilter {
    pub search: Option<String>,
    pub category: Option<LibraryCategory>,
    pub state: Option<LibraryEntryState>,
    pub tag: Option<String>,
    pub mime_type: Option<String>,
    pub downloaded_from: Option<DateTime<Utc>>,
    pub downloaded_to: Option<DateTime<Utc>>,
}

impl Repository {
    pub async fn upsert_library_entry(&self, input: NewLibraryEntry) -> Result<LibraryEntry> {
        if input.source_url.len() > 16_384 {
            return Err(RavynError::Invalid(
                "library source URL may not exceed 16384 characters".into(),
            ));
        }
        if input.filename.trim().is_empty() || input.filename.len() > 1_024 {
            return Err(RavynError::Invalid(
                "library filename must contain between 1 and 1024 characters".into(),
            ));
        }
        if input.tags.len() > 256 || input.mirrors.len() > 256 {
            return Err(RavynError::Invalid(
                "library entries may contain at most 256 tags or mirrors".into(),
            ));
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let size_bytes = input
            .size_bytes
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RavynError::Invalid("library file size exceeds SQLite range".into()))?;
        let path = input.path.to_string_lossy().to_string();
        if let Some(sha256) = input.sha256.as_deref() {
            crate::services::checksum::validate_sha256(sha256)?;
        }
        let sha256 = input.sha256.map(|value| value.to_ascii_lowercase());
        sqlx::query(
            "INSERT INTO library_entries(\
                id,job_id,source_url,mirrors_json,sha256,size_bytes,path,filename,category,mime_type,\
                media_metadata_json,torrent_metadata_json,tags_json,trust_json,state,trash_path,\
                imported,downloaded_at,created_at,updated_at\
             ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?, 'active',NULL,?,?,?,?) \
             ON CONFLICT(path) WHERE state!='trashed' DO UPDATE SET \
                job_id=excluded.job_id,source_url=excluded.source_url,mirrors_json=excluded.mirrors_json,\
                sha256=excluded.sha256,size_bytes=excluded.size_bytes,filename=excluded.filename,\
                category=excluded.category,mime_type=excluded.mime_type,\
                media_metadata_json=excluded.media_metadata_json,\
                torrent_metadata_json=excluded.torrent_metadata_json,tags_json=excluded.tags_json,\
                trust_json=excluded.trust_json,state='active',trash_path=NULL,imported=excluded.imported,\
                downloaded_at=excluded.downloaded_at,updated_at=excluded.updated_at",
        )
        .bind(id.to_string())
        .bind(input.job_id.map(|value| value.to_string()))
        .bind(input.source_url)
        .bind(serde_json::to_string(&input.mirrors)?)
        .bind(sha256)
        .bind(size_bytes)
        .bind(&path)
        .bind(input.filename)
        .bind(input.category.as_str())
        .bind(input.mime_type)
        .bind(serde_json::to_string(&input.media_metadata)?)
        .bind(serde_json::to_string(&input.torrent_metadata)?)
        .bind(serde_json::to_string(&input.tags)?)
        .bind(input.trust.map(|value| serde_json::to_string(&value)).transpose()?)
        .bind(input.imported)
        .bind(input.downloaded_at)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        self.get_library_entry_by_path(&input.path).await
    }

    pub async fn get_library_entry(&self, id: Uuid) -> Result<LibraryEntry> {
        sqlx::query("SELECT * FROM library_entries WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_library_entry)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("library entry {id}")))
    }

    pub async fn get_library_entry_by_path(&self, path: &std::path::Path) -> Result<LibraryEntry> {
        sqlx::query(
            "SELECT * FROM library_entries WHERE path=? AND state!='trashed' \
             ORDER BY CASE state WHEN 'active' THEN 0 ELSE 1 END,updated_at DESC LIMIT 1",
        )
        .bind(path.to_string_lossy().to_string())
        .fetch_optional(self.pool())
        .await?
        .map(row_to_library_entry)
        .transpose()?
        .ok_or_else(|| RavynError::NotFound(format!("library entry for {}", path.display())))
    }

    /// Returns whether another live or missing entry reserves a logical destination path.
    pub async fn library_path_is_reserved_by_other(
        &self,
        id: Uuid,
        path: &std::path::Path,
    ) -> Result<bool> {
        let reserved: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM library_entries WHERE path=? AND state!='trashed' AND id<>? LIMIT 1",
        )
        .bind(path.to_string_lossy().to_string())
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;
        Ok(reserved.is_some())
    }

    pub async fn find_active_library_entry_by_sha256(
        &self,
        sha256: &str,
    ) -> Result<Option<LibraryEntry>> {
        sqlx::query(
            "SELECT * FROM library_entries WHERE sha256=? AND state='active' ORDER BY downloaded_at DESC,id DESC LIMIT 1",
        )
        .bind(sha256.to_ascii_lowercase())
        .fetch_optional(self.pool())
        .await?
        .map(row_to_library_entry)
        .transpose()
    }

    pub async fn list_library_entries(
        &self,
        filter: &LibraryListFilter,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<LibraryEntry>> {
        let limit = i64::try_from(limit.clamp(1, 201))
            .map_err(|_| RavynError::Invalid("library page limit is invalid".into()))?;
        let offset = i64::try_from(offset)
            .map_err(|_| RavynError::Invalid("library page offset is too large".into()))?;
        let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM library_entries WHERE 1=1");

        if let Some(search) = normalized_search(filter.search.as_deref()) {
            query.push(" AND (filename LIKE ").push_bind(search.clone());
            query.push(" OR path LIKE ").push_bind(search.clone());
            query.push(" OR source_url LIKE ").push_bind(search);
            query.push(")");
        }
        if let Some(category) = filter.category {
            query.push(" AND category=").push_bind(category.as_str());
        }
        if let Some(state) = filter.state {
            query.push(" AND state=").push_bind(state.as_str());
        }
        if let Some(tag) = normalized_search(filter.tag.as_deref()) {
            query.push(" AND tags_json LIKE ").push_bind(tag);
        }
        if let Some(mime_type) = filter
            .mime_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            query
                .push(" AND mime_type=")
                .push_bind(mime_type.to_ascii_lowercase());
        }
        if let Some(downloaded_from) = filter.downloaded_from {
            query
                .push(" AND downloaded_at>=")
                .push_bind(downloaded_from);
        }
        if let Some(downloaded_to) = filter.downloaded_to {
            query.push(" AND downloaded_at<=").push_bind(downloaded_to);
        }
        query
            .push(" ORDER BY downloaded_at DESC,id DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        query
            .build()
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_library_entry)
            .collect()
    }

    pub async fn find_library_duplicate_candidates(
        &self,
        sha256: Option<&str>,
        size_bytes: Option<u64>,
        filename: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LibraryEntry>> {
        let sha256 = sha256.map(str::trim).filter(|value| !value.is_empty());
        let filename = filename.map(str::trim).filter(|value| !value.is_empty());
        if sha256.is_none() && size_bytes.is_none() && filename.is_none() {
            return Err(RavynError::Invalid(
                "duplicate search requires sha256, size_bytes, or filename".into(),
            ));
        }
        if let Some(sha256) = sha256 {
            crate::services::checksum::validate_sha256(sha256)?;
        }
        let size_bytes = size_bytes
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RavynError::Invalid("duplicate size exceeds SQLite range".into()))?;
        let limit = i64::try_from(limit.clamp(1, 100))
            .map_err(|_| RavynError::Invalid("duplicate search limit is invalid".into()))?;
        let mut query =
            QueryBuilder::<Sqlite>::new("SELECT * FROM library_entries WHERE state='active' AND (");
        let mut has_clause = false;
        if let Some(sha256) = sha256 {
            query.push("sha256=").push_bind(sha256.to_ascii_lowercase());
            has_clause = true;
        }
        if let Some(size_bytes) = size_bytes {
            if has_clause {
                query.push(" OR ");
            }
            query.push("size_bytes=").push_bind(size_bytes);
            has_clause = true;
        }
        if let Some(filename) = filename {
            if has_clause {
                query.push(" OR ");
            }
            query.push("filename COLLATE NOCASE=").push_bind(filename);
        }
        query
            .push(") ORDER BY downloaded_at DESC,id DESC LIMIT ")
            .push_bind(limit);
        query
            .build()
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_library_entry)
            .collect()
    }

    pub async fn list_trashed_library_entries_before(
        &self,
        cutoff: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<LibraryEntry>> {
        let limit = i64::try_from(limit.clamp(1, 1_000))
            .map_err(|_| RavynError::Invalid("library cleanup limit is invalid".into()))?;
        sqlx::query(
            "SELECT * FROM library_entries \
             WHERE state='trashed' AND updated_at<=? \
             ORDER BY updated_at ASC,id ASC LIMIT ?",
        )
        .bind(cutoff)
        .bind(limit)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(row_to_library_entry)
        .collect()
    }

    pub async fn update_library_state(
        &self,
        id: Uuid,
        state: LibraryEntryState,
        path: &std::path::Path,
        trash_path: Option<&std::path::Path>,
    ) -> Result<LibraryEntry> {
        let changed = sqlx::query(
            "UPDATE library_entries SET state=?,path=?,trash_path=?,updated_at=? WHERE id=?",
        )
        .bind(state.as_str())
        .bind(path.to_string_lossy().to_string())
        .bind(trash_path.map(|value| value.to_string_lossy().to_string()))
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("library entry {id}")));
        }
        self.get_library_entry(id).await
    }

    pub async fn purge_library_entry(&self, id: Uuid) -> Result<()> {
        let changed = sqlx::query("DELETE FROM library_entries WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("library entry {id}")));
        }
        Ok(())
    }

    pub async fn set_library_trust_for_job<T: serde::Serialize>(
        &self,
        job_id: Uuid,
        trust: &T,
    ) -> Result<u64> {
        let changed =
            sqlx::query("UPDATE library_entries SET trust_json=?,updated_at=? WHERE job_id=?")
                .bind(serde_json::to_string(trust)?)
                .bind(Utc::now())
                .bind(job_id.to_string())
                .execute(self.pool())
                .await?
                .rows_affected();
        Ok(changed)
    }

    pub async fn increment_stat_counter(&self, key: &str, amount: u64) -> Result<u64> {
        if key.trim().is_empty() || key.len() > 128 {
            return Err(RavynError::Invalid(
                "stat counter keys must contain between 1 and 128 characters".into(),
            ));
        }
        let amount = i64::try_from(amount)
            .map_err(|_| RavynError::Invalid("stat counter increment is too large".into()))?;
        sqlx::query(
            "INSERT INTO stat_counters(key,value) VALUES(?,?) \
             ON CONFLICT(key) DO UPDATE SET value=value+excluded.value",
        )
        .bind(key)
        .bind(amount)
        .execute(self.pool())
        .await?;
        let value: i64 = sqlx::query_scalar("SELECT value FROM stat_counters WHERE key=?")
            .bind(key)
            .fetch_one(self.pool())
            .await?;
        u64::try_from(value)
            .map_err(|_| RavynError::Internal("stat counter became negative".into()))
    }
}

fn normalized_search(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"))
}

pub(crate) fn row_to_library_entry(row: SqliteRow) -> Result<LibraryEntry> {
    let size_bytes = row
        .try_get::<Option<i64>, _>("size_bytes")?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| RavynError::Internal("library size is negative".into()))?;
    Ok(LibraryEntry {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        job_id: row
            .try_get::<Option<String>, _>("job_id")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        source_url: row.try_get("source_url")?,
        mirrors: serde_json::from_str(&row.try_get::<String, _>("mirrors_json")?)?,
        sha256: row.try_get("sha256")?,
        size_bytes,
        path: PathBuf::from(row.try_get::<String, _>("path")?),
        filename: row.try_get("filename")?,
        category: LibraryCategory::from_str(&row.try_get::<String, _>("category")?)?,
        mime_type: row.try_get("mime_type")?,
        media_metadata: serde_json::from_str(&row.try_get::<String, _>("media_metadata_json")?)?,
        torrent_metadata: serde_json::from_str(
            &row.try_get::<String, _>("torrent_metadata_json")?,
        )?,
        tags: serde_json::from_str(&row.try_get::<String, _>("tags_json")?)?,
        trust: row
            .try_get::<Option<String>, _>("trust_json")?
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        state: LibraryEntryState::from_str(&row.try_get::<String, _>("state")?)?,
        trash_path: row
            .try_get::<Option<String>, _>("trash_path")?
            .map(PathBuf::from),
        imported: row.try_get("imported")?,
        downloaded_at: row.try_get("downloaded_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl Repository {
    pub async fn load_cleanup_policies(&self) -> Result<crate::services::library::CleanupPolicies> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT cleanup_json FROM library_settings WHERE id=1")
                .fetch_optional(self.pool())
                .await?;
        match value {
            Some(value) => Ok(serde_json::from_str(&value)?),
            None => Ok(crate::services::library::CleanupPolicies::default()),
        }
    }

    pub async fn save_cleanup_policies(
        &self,
        policies: &crate::services::library::CleanupPolicies,
    ) -> Result<()> {
        policies.validate()?;
        sqlx::query(
            "INSERT INTO library_settings(id,cleanup_json,updated_at) VALUES(1,?,?) \
             ON CONFLICT(id) DO UPDATE SET cleanup_json=excluded.cleanup_json,updated_at=excluded.updated_at",
        )
        .bind(serde_json::to_string(policies)?)
        .bind(Utc::now())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_job_logs_before(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        Ok(sqlx::query("DELETE FROM job_logs WHERE timestamp<?")
            .bind(cutoff)
            .execute(self.pool())
            .await?
            .rows_affected())
    }

    pub async fn personal_statistics(
        &self,
    ) -> Result<crate::services::library::PersonalStatistics> {
        use crate::services::library::{CategoryStatistics, PersonalStatistics};
        let rows = sqlx::query(
            "SELECT category,state,COUNT(*) AS files,COALESCE(SUM(size_bytes),0) AS bytes \
             FROM library_entries GROUP BY category,state",
        )
        .fetch_all(self.pool())
        .await?;
        let mut categories = std::collections::BTreeMap::new();
        let mut total_files = 0_u64;
        let mut total_downloaded_bytes = 0_u64;
        let mut active_storage_bytes = 0_u64;
        let mut trashed_storage_bytes = 0_u64;
        for row in rows {
            let category: String = row.try_get("category")?;
            let state: String = row.try_get("state")?;
            let files = u64::try_from(row.try_get::<i64, _>("files")?)
                .map_err(|_| RavynError::Internal("negative library file count".into()))?;
            let bytes = u64::try_from(row.try_get::<i64, _>("bytes")?)
                .map_err(|_| RavynError::Internal("negative library byte count".into()))?;
            total_files = total_files.saturating_add(files);
            total_downloaded_bytes = total_downloaded_bytes.saturating_add(bytes);
            match state.as_str() {
                "active" => active_storage_bytes = active_storage_bytes.saturating_add(bytes),
                "trashed" => trashed_storage_bytes = trashed_storage_bytes.saturating_add(bytes),
                _ => {}
            }
            let entry = categories
                .entry(category)
                .or_insert(CategoryStatistics { files: 0, bytes: 0 });
            entry.files = entry.files.saturating_add(files);
            entry.bytes = entry.bytes.saturating_add(bytes);
        }

        let speed_rows = sqlx::query(
            "SELECT downloaded_bytes,started_at,completed_at FROM jobs \
             WHERE status IN ('completed','partial') AND started_at IS NOT NULL AND completed_at IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?;
        let mut speed_bytes = 0_u64;
        let mut speed_seconds = 0_u64;
        for row in speed_rows {
            let downloaded = row.try_get::<i64, _>("downloaded_bytes")?;
            let started: DateTime<Utc> = row.try_get("started_at")?;
            let completed: DateTime<Utc> = row.try_get("completed_at")?;
            if downloaded >= 0 {
                speed_bytes = speed_bytes.saturating_add(downloaded as u64);
                speed_seconds = speed_seconds.saturating_add(
                    completed
                        .signed_duration_since(started)
                        .num_seconds()
                        .max(1) as u64,
                );
            }
        }
        let counters = sqlx::query("SELECT key,value FROM stat_counters")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("key")?,
                    row.try_get::<i64, _>("value")?,
                ))
            })
            .collect::<Result<std::collections::HashMap<_, _>>>()?;

        Ok(PersonalStatistics {
            total_files,
            total_downloaded_bytes,
            active_storage_bytes,
            trashed_storage_bytes,
            average_speed_bps: if speed_seconds == 0 {
                0
            } else {
                speed_bytes / speed_seconds
            },
            saved_bandwidth_bytes: counter(&counters, "saved_bandwidth_bytes")?,
            duplicate_avoidance_count: counter(&counters, "duplicate_avoidance_count")?,
            categories,
            monthly_activity: activity_buckets(self, "%Y-%m").await?,
            yearly_activity: activity_buckets(self, "%Y").await?,
        })
    }
}

async fn activity_buckets(
    repository: &Repository,
    format: &str,
) -> Result<Vec<crate::services::library::ActivityBucket>> {
    use crate::services::library::ActivityBucket;
    let rows = sqlx::query(
        "SELECT strftime(?,downloaded_at) AS period,COUNT(*) AS files,COALESCE(SUM(size_bytes),0) AS bytes \
         FROM library_entries GROUP BY period ORDER BY period DESC LIMIT 120",
    )
    .bind(format)
    .fetch_all(repository.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ActivityBucket {
                period: row
                    .try_get::<Option<String>, _>("period")?
                    .unwrap_or_default(),
                files: u64::try_from(row.try_get::<i64, _>("files")?)
                    .map_err(|_| RavynError::Internal("negative activity count".into()))?,
                bytes: u64::try_from(row.try_get::<i64, _>("bytes")?)
                    .map_err(|_| RavynError::Internal("negative activity bytes".into()))?,
            })
        })
        .collect()
}

fn counter(values: &std::collections::HashMap<String, i64>, key: &str) -> Result<u64> {
    u64::try_from(values.get(key).copied().unwrap_or_default())
        .map_err(|_| RavynError::Internal(format!("stat counter {key} is negative")))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn repository() -> (tempfile::TempDir, Repository) {
        let temporary = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            temporary.path().join("test.sqlite3").display()
        );
        let repository = Repository::connect(&url).await.unwrap();
        (temporary, repository)
    }

    fn entry(path: PathBuf) -> NewLibraryEntry {
        NewLibraryEntry {
            job_id: None,
            source_url: "https://example.test/manual.pdf".into(),
            mirrors: Vec::new(),
            sha256: Some("AA".repeat(32)),
            size_bytes: Some(42),
            filename: "manual.pdf".into(),
            path,
            category: LibraryCategory::Documents,
            mime_type: Some("application/pdf".into()),
            media_metadata: serde_json::json!({}),
            torrent_metadata: serde_json::json!({}),
            tags: vec!["manual".into()],
            trust: None,
            imported: false,
            downloaded_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_search_and_hash_lookup_round_trip() {
        let (temporary, repository) = repository().await;
        let path = temporary.path().join("manual.pdf");
        let inserted = repository
            .upsert_library_entry(entry(path.clone()))
            .await
            .unwrap();

        let by_hash = repository
            .find_active_library_entry_by_sha256(&"aa".repeat(32))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_hash.id, inserted.id);

        let listed = repository
            .list_library_entries(
                &LibraryListFilter {
                    search: Some("manual".into()),
                    category: Some(LibraryCategory::Documents),
                    state: Some(LibraryEntryState::Active),
                    tag: Some("manual".into()),
                    ..Default::default()
                },
                0,
                10,
            )
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, path);
    }

    #[tokio::test]
    async fn rejects_invalid_sha256_values_before_persisting() {
        let (temporary, repository) = repository().await;
        let mut invalid = entry(temporary.path().join("invalid.bin"));
        invalid.sha256 = Some("not-a-sha256".into());

        assert!(repository.upsert_library_entry(invalid).await.is_err());
    }

    #[tokio::test]
    async fn duplicate_candidates_match_hash_size_or_case_insensitive_filename() {
        let (temporary, repository) = repository().await;
        let path = temporary.path().join("manual.pdf");
        repository.upsert_library_entry(entry(path)).await.unwrap();

        let matches = repository
            .find_library_duplicate_candidates(None, Some(42), Some("MANUAL.PDF"), 10)
            .await
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert!(
            repository
                .find_library_duplicate_candidates(None, None, None, 10)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn state_and_counter_updates_are_persistent() {
        let (temporary, repository) = repository().await;
        let path = temporary.path().join("manual.pdf");
        let inserted = repository
            .upsert_library_entry(entry(path.clone()))
            .await
            .unwrap();
        let trash_path = temporary.path().join("Trash/manual.pdf");
        let updated = repository
            .update_library_state(
                inserted.id,
                LibraryEntryState::Trashed,
                &trash_path,
                Some(&trash_path),
            )
            .await
            .unwrap();
        assert_eq!(updated.state, LibraryEntryState::Trashed);
        assert_eq!(
            repository
                .increment_stat_counter("saved_bytes", 7)
                .await
                .unwrap(),
            7
        );
        assert_eq!(
            repository
                .increment_stat_counter("saved_bytes", 5)
                .await
                .unwrap(),
            12
        );
    }
}
