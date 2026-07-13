use std::{collections::BTreeMap, path::Path};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    config::Config,
    error::{RavynError, Result},
    storage::Repository,
};

const MAX_CLEANUP_ENTRIES: usize = 250_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CleanupPolicies {
    pub temporary_max_age_days: u32,
    pub trash_retention_days: u32,
    pub log_retention_days: u32,
    pub cache_retention_days: u32,
}

impl Default for CleanupPolicies {
    fn default() -> Self {
        Self {
            temporary_max_age_days: 7,
            trash_retention_days: 30,
            log_retention_days: 90,
            cache_retention_days: 30,
        }
    }
}

impl CleanupPolicies {
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("temporary_max_age_days", self.temporary_max_age_days),
            ("trash_retention_days", self.trash_retention_days),
            ("log_retention_days", self.log_retention_days),
            ("cache_retention_days", self.cache_retention_days),
        ] {
            if value == 0 || value > 3_650 {
                return Err(RavynError::Invalid(format!(
                    "{name} must be between 1 and 3650"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CleanupReport {
    pub temporary_files_removed: u64,
    pub temporary_bytes_removed: u64,
    pub cache_files_removed: u64,
    pub cache_bytes_removed: u64,
    pub trash_entries_purged: u64,
    pub job_logs_removed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalStatistics {
    pub total_files: u64,
    pub total_downloaded_bytes: u64,
    pub active_storage_bytes: u64,
    pub trashed_storage_bytes: u64,
    pub average_speed_bps: u64,
    pub saved_bandwidth_bytes: u64,
    pub duplicate_avoidance_count: u64,
    pub categories: BTreeMap<String, CategoryStatistics>,
    pub monthly_activity: Vec<ActivityBucket>,
    pub yearly_activity: Vec<ActivityBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryStatistics {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityBucket {
    pub period: String,
    pub files: u64,
    pub bytes: u64,
}

/// Executes every configured retention policy with bounded filesystem work.
pub async fn run_cleanup(
    config: &Config,
    repository: &Repository,
    policies: &CleanupPolicies,
) -> Result<CleanupReport> {
    policies.validate()?;
    let now = Utc::now();
    let mut report = CleanupReport::default();
    if let Some(root) = config.effective_library_root() {
        let temporary = cleanup_directory(
            &root.join("Temporary"),
            now - Duration::days(i64::from(policies.temporary_max_age_days)),
        )
        .await?;
        report.temporary_files_removed = temporary.0;
        report.temporary_bytes_removed = temporary.1;

        let cutoff = now - Duration::days(i64::from(policies.trash_retention_days));
        loop {
            let page = repository
                .list_trashed_library_entries_before(cutoff, 200)
                .await?;
            if page.is_empty() {
                break;
            }
            for entry in page {
                super::purge_entry(config, repository, entry.id).await?;
                report.trash_entries_purged += 1;
            }
        }
    }

    let cache = cleanup_directory(
        &config.data_dir.join("cache"),
        now - Duration::days(i64::from(policies.cache_retention_days)),
    )
    .await?;
    report.cache_files_removed = cache.0;
    report.cache_bytes_removed = cache.1;
    report.job_logs_removed = repository
        .delete_job_logs_before(now - Duration::days(i64::from(policies.log_retention_days)))
        .await?;
    Ok(report)
}

async fn cleanup_directory(root: &Path, cutoff: DateTime<Utc>) -> Result<(u64, u64)> {
    if !tokio::fs::try_exists(root).await? {
        return Ok((0, 0));
    }
    let mut queue = std::collections::VecDeque::from([root.to_path_buf()]);
    let mut directories = Vec::new();
    let mut visited = 0_usize;
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    while let Some(directory) = queue.pop_front() {
        let mut entries = tokio::fs::read_dir(&directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            visited += 1;
            if visited > MAX_CLEANUP_ENTRIES {
                return Ok((files, bytes));
            }
            let path = entry.path();
            let metadata = tokio::fs::symlink_metadata(&path).await?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                queue.push_back(path.clone());
                directories.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata
                .modified()
                .ok()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(Utc::now);
            if modified <= cutoff {
                tokio::fs::remove_file(&path).await?;
                files += 1;
                bytes = bytes.saturating_add(metadata.len());
            }
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        let _ = tokio::fs::remove_dir(directory).await;
    }
    Ok((files, bytes))
}


#[cfg(test)]
mod tests {
    use std::time::{Duration as StdDuration, SystemTime};

    use clap::Parser;

    use super::*;
    use crate::{
        services::library::{LibraryCategory, move_to_trash},
        storage::NewLibraryEntry,
    };

    #[test]
    fn cleanup_policies_reject_unbounded_or_zero_retention() {
        assert!(CleanupPolicies::default().validate().is_ok());
        assert!(CleanupPolicies {
            temporary_max_age_days: 0,
            ..CleanupPolicies::default()
        }
        .validate()
        .is_err());
    }

    #[tokio::test]
    async fn cleanup_removes_expired_temporary_cache_and_trash_payloads() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("Ravyn");
        let config = Config::try_parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().join("data").to_str().unwrap(),
            "--download-dir",
            temporary.path().join("downloads").to_str().unwrap(),
            "--library-root",
            root.to_str().unwrap(),
        ])
        .unwrap();
        config.prepare_directories().await.unwrap();
        let repository = Repository::connect(&config.database_url()).await.unwrap();

        let old = SystemTime::now() - StdDuration::from_secs(3 * 24 * 60 * 60);
        let temporary_file = root.join("Temporary/old.part");
        tokio::fs::write(&temporary_file, b"temporary").await.unwrap();
        std::fs::File::open(&temporary_file)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        let cache_directory = config.data_dir.join("cache");
        tokio::fs::create_dir_all(&cache_directory).await.unwrap();
        let cache_file = cache_directory.join("old.cache");
        tokio::fs::write(&cache_file, b"cache").await.unwrap();
        std::fs::File::open(&cache_file)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();

        let payload = root.join("Documents/old.pdf");
        tokio::fs::write(&payload, b"payload").await.unwrap();
        let entry = repository
            .upsert_library_entry(NewLibraryEntry {
                job_id: None,
                source_url: "https://example.test/old.pdf".into(),
                mirrors: Vec::new(),
                sha256: None,
                size_bytes: Some(7),
                path: payload,
                filename: "old.pdf".into(),
                category: LibraryCategory::Documents,
                mime_type: Some("application/pdf".into()),
                media_metadata: serde_json::json!({}),
                torrent_metadata: serde_json::json!({}),
                tags: Vec::new(),
                trust: None,
                imported: true,
                downloaded_at: Utc::now(),
            })
            .await
            .unwrap();
        move_to_trash(&config, &repository, entry.id).await.unwrap();
        sqlx::query("UPDATE library_entries SET updated_at=? WHERE id=?")
            .bind(Utc::now() - Duration::days(3))
            .bind(entry.id.to_string())
            .execute(repository.pool())
            .await
            .unwrap();

        let report = run_cleanup(
            &config,
            &repository,
            &CleanupPolicies {
                temporary_max_age_days: 1,
                trash_retention_days: 1,
                log_retention_days: 1,
                cache_retention_days: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.temporary_files_removed, 1);
        assert_eq!(report.cache_files_removed, 1);
        assert_eq!(report.trash_entries_purged, 1);
        assert!(!tokio::fs::try_exists(&temporary_file).await.unwrap());
        assert!(!tokio::fs::try_exists(&cache_file).await.unwrap());
    }
}
