use std::{collections::VecDeque, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    config::Config,
    error::{RavynError, Result},
    services::{checksum, security},
    storage::{LibraryEntryState, LibraryListFilter, NewLibraryEntry, Repository},
};

const DEFAULT_MAX_ENTRIES: usize = 100_000;
const DEFAULT_MAX_DEPTH: usize = 64;
const ABSOLUTE_MAX_ENTRIES: usize = 1_000_000;
const ABSOLUTE_MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LibraryImportRequest {
    pub path: PathBuf,
    pub tags: Vec<String>,
    pub max_entries: usize,
    pub max_depth: usize,
}

impl Default for LibraryImportRequest {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            tags: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LibraryImportStatus {
    pub run_id: Option<Uuid>,
    pub running: bool,
    pub root: Option<PathBuf>,
    pub scanned: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub skipped: usize,
    /// True when the configured scan limit stopped the import before every entry was visited.
    pub truncated: bool,
    /// True after a client has requested cancellation and before the worker has stopped.
    pub cancel_requested: bool,
    /// True when the most recent import stopped because it was cancelled.
    pub cancelled: bool,
    pub errors: Vec<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub type SharedImportStatus = Arc<RwLock<LibraryImportStatus>>;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RelocationRequest {
    pub path: Option<PathBuf>,
    pub max_entries: Option<usize>,
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelocationReport {
    pub scanned: usize,
    pub repaired: usize,
    pub unmatched: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyLibraryReport {
    pub checked: usize,
    pub missing: usize,
}

/// Reserves the shared import slot before a background task is spawned.
pub async fn reserve_import(
    config: &Config,
    request: &LibraryImportRequest,
    status: &SharedImportStatus,
) -> Result<LibraryImportStatus> {
    validate_scan_request(
        config,
        &request.path,
        request.max_entries,
        request.max_depth,
    )?;
    let mut current = status.write().await;
    if current.running {
        return Err(RavynError::Conflict(
            "a library import is already running".into(),
        ));
    }
    *current = LibraryImportStatus {
        run_id: Some(Uuid::new_v4()),
        running: true,
        root: Some(request.path.clone()),
        started_at: Some(Utc::now()),
        ..LibraryImportStatus::default()
    };
    Ok(current.clone())
}

/// Runs a previously reserved, bounded, symlink-safe import.
pub async fn import_directory(
    config: Arc<Config>,
    repository: Repository,
    request: LibraryImportRequest,
    status: SharedImportStatus,
    cancellation: CancellationToken,
) -> Result<()> {
    let result =
        import_directory_inner(&config, &repository, &request, &status, &cancellation).await;
    let mut current = status.write().await;
    current.running = false;
    current.completed_at = Some(Utc::now());
    current.cancelled = matches!(result, Err(RavynError::Cancelled));
    if let Err(error) = &result {
        if !matches!(error, RavynError::Cancelled) {
            push_bounded_error(&mut current.errors, error.to_string());
        }
    }
    result
}

async fn import_directory_inner(
    config: &Config,
    repository: &Repository,
    request: &LibraryImportRequest,
    status: &SharedImportStatus,
    cancellation: &CancellationToken,
) -> Result<()> {
    let mut queue = VecDeque::from([(request.path.clone(), 0_usize)]);
    let maximum = request.max_entries.clamp(1, ABSOLUTE_MAX_ENTRIES);
    let maximum_depth = request.max_depth.clamp(1, ABSOLUTE_MAX_DEPTH);
    let mut visited_entries = 0_usize;

    while let Some((directory, depth)) = queue.pop_front() {
        if cancellation.is_cancelled() {
            return Err(RavynError::Cancelled);
        }
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) => {
                let mut current = status.write().await;
                current.skipped += 1;
                push_bounded_error(&mut current.errors, format!("{}: {error}", directory.display()));
                continue;
            }
        };
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(error) => {
                    let mut current = status.write().await;
                    current.skipped += 1;
                    push_bounded_error(&mut current.errors, format!("{}: {error}", directory.display()));
                    break;
                }
            };
            if visited_entries >= maximum {
                status.write().await.truncated = true;
                return Ok(());
            }
            visited_entries += 1;
            if cancellation.is_cancelled() {
                return Err(RavynError::Cancelled);
            }
            let path = entry.path();
            let metadata = match tokio::fs::symlink_metadata(&path).await {
                Ok(metadata) => metadata,
                Err(error) => {
                    let mut current = status.write().await;
                    current.skipped += 1;
                    push_bounded_error(&mut current.errors, format!("{}: {error}", path.display()));
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                status.write().await.skipped += 1;
                continue;
            }
            if metadata.is_dir() {
                if depth < maximum_depth && !is_internal_scan_exclusion(config, &path) {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            if !metadata.is_file() {
                status.write().await.skipped += 1;
                continue;
            }

            status.write().await.scanned += 1;
            let sha256 = match checksum::sha256(&path, cancellation).await {
                Ok(value) => value,
                Err(RavynError::Cancelled) => return Err(RavynError::Cancelled),
                Err(error) => {
                    let mut current = status.write().await;
                    current.skipped += 1;
                    push_bounded_error(
                        &mut current.errors,
                        format!("{}: {error}", path.display()),
                    );
                    continue;
                }
            };
            if repository
                .find_active_library_entry_by_sha256(&sha256)
                .await?
                .is_some()
            {
                status.write().await.duplicates += 1;
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    RavynError::Invalid(format!(
                        "imported path has no valid filename: {}",
                        path.display()
                    ))
                })?
                .to_owned();
            let category = super::classify_file_with_overrides(
                &path,
                None,
                &config.library_category_overrides,
            )
            .await?;
            let source_url = url::Url::from_file_path(&path)
                .map(|value| value.to_string())
                .unwrap_or_else(|_| format!("file://{}", path.display()));
            repository
                .upsert_library_entry(NewLibraryEntry {
                    job_id: None,
                    source_url,
                    mirrors: Vec::new(),
                    sha256: Some(sha256),
                    size_bytes: Some(metadata.len()),
                    path,
                    filename,
                    category,
                    mime_type: None,
                    media_metadata: serde_json::json!({}),
                    torrent_metadata: serde_json::json!({}),
                    tags: request.tags.clone(),
                    trust: None,
                    imported: true,
                    downloaded_at: metadata
                        .modified()
                        .ok()
                        .map(DateTime::<Utc>::from)
                        .unwrap_or_else(Utc::now),
                })
                .await?;
            status.write().await.imported += 1;
        }
    }
    Ok(())
}

/// Marks active entries whose payload no longer exists as missing.
pub async fn verify_entries(repository: &Repository) -> Result<VerifyLibraryReport> {
    let entries = all_entries(repository, Some(LibraryEntryState::Active)).await?;
    let mut missing = 0;
    for entry in &entries {
        let exists = tokio::fs::symlink_metadata(&entry.path)
            .await
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());
        if !exists {
            repository
                .update_library_state(entry.id, LibraryEntryState::Missing, &entry.path, None)
                .await?;
            missing += 1;
        }
    }
    Ok(VerifyLibraryReport {
        checked: entries.len(),
        missing,
    })
}

/// Repairs missing entries by matching their SHA-256 against files in a bounded scan root.
pub async fn repair_relocations(
    config: &Config,
    repository: &Repository,
    request: RelocationRequest,
    cancellation: &CancellationToken,
) -> Result<RelocationReport> {
    let root = request
        .path
        .or_else(|| config.effective_library_root())
        .unwrap_or_else(|| config.effective_download_dir());
    let maximum = request
        .max_entries
        .unwrap_or(DEFAULT_MAX_ENTRIES)
        .clamp(1, ABSOLUTE_MAX_ENTRIES);
    let maximum_depth = request
        .max_depth
        .unwrap_or(DEFAULT_MAX_DEPTH)
        .clamp(1, ABSOLUTE_MAX_DEPTH);
    validate_scan_request(config, &root, maximum, maximum_depth)?;
    let missing = all_entries(repository, Some(LibraryEntryState::Missing)).await?;
    let mut hashes = std::collections::HashMap::<String, Vec<Uuid>>::new();
    for entry in &missing {
        if let Some(hash) = entry.sha256.as_ref() {
            hashes.entry(hash.clone()).or_default().push(entry.id);
        }
    }
    let mut occupied_paths = all_entries(repository, Some(LibraryEntryState::Active))
        .await?
        .into_iter()
        .map(|entry| entry.path)
        .collect::<std::collections::HashSet<_>>();
    let mut report = RelocationReport {
        scanned: 0,
        repaired: 0,
        unmatched: hashes.values().map(Vec::len).sum(),
    };
    if hashes.is_empty() {
        return Ok(report);
    }

    let mut queue = VecDeque::from([(root, 0_usize)]);
    let mut visited_entries = 0_usize;
    while let Some((directory, depth)) = queue.pop_front() {
        let mut entries = tokio::fs::read_dir(&directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            if visited_entries >= maximum {
                return Ok(report);
            }
            visited_entries += 1;
            if cancellation.is_cancelled() {
                return Err(RavynError::Cancelled);
            }
            let path = entry.path();
            let metadata = tokio::fs::symlink_metadata(&path).await?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if depth < maximum_depth && !is_internal_scan_exclusion(config, &path) {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }
            if !metadata.is_file() || occupied_paths.contains(&path) {
                continue;
            }
            report.scanned += 1;
            let sha256 = checksum::sha256(&path, cancellation).await?;
            let mut remove_hash = false;
            if let Some(ids) = hashes.get_mut(&sha256) {
                if let Some(id) = ids.pop() {
                    repository
                        .update_library_state(id, LibraryEntryState::Active, &path, None)
                        .await?;
                    occupied_paths.insert(path.clone());
                    report.repaired += 1;
                    report.unmatched = report.unmatched.saturating_sub(1);
                }
                remove_hash = ids.is_empty();
            }
            if remove_hash {
                hashes.remove(&sha256);
            }
            if hashes.is_empty() {
                return Ok(report);
            }
        }
    }
    Ok(report)
}

fn validate_scan_request(
    config: &Config,
    root: &std::path::Path,
    max_entries: usize,
    max_depth: usize,
) -> Result<()> {
    if root.as_os_str().is_empty() {
        return Err(RavynError::Invalid("library scan path is required".into()));
    }
    if max_entries == 0 || max_entries > ABSOLUTE_MAX_ENTRIES {
        return Err(RavynError::Invalid(format!(
            "library max_entries must be between 1 and {ABSOLUTE_MAX_ENTRIES}"
        )));
    }
    if max_depth == 0 || max_depth > ABSOLUTE_MAX_DEPTH {
        return Err(RavynError::Invalid(format!(
            "library max_depth must be between 1 and {ABSOLUTE_MAX_DEPTH}"
        )));
    }
    security::validate_output_path(config, root)?;
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(RavynError::Invalid(
            "library scan root must be a non-symlink directory".into(),
        ));
    }
    Ok(())
}

fn is_internal_scan_exclusion(config: &Config, path: &std::path::Path) -> bool {
    config
        .effective_library_root()
        .is_some_and(|root| path == root.join("Trash") || path == root.join("Temporary"))
}

async fn all_entries(
    repository: &Repository,
    state: Option<LibraryEntryState>,
) -> Result<Vec<crate::storage::LibraryEntry>> {
    let mut offset = 0_u64;
    let mut entries = Vec::new();
    loop {
        let page = repository
            .list_library_entries(
                &LibraryListFilter {
                    state,
                    ..LibraryListFilter::default()
                },
                offset,
                200,
            )
            .await?;
        let count = page.len();
        entries.extend(page);
        if count < 200 {
            break;
        }
        offset = offset.saturating_add(count as u64);
    }
    Ok(entries)
}

fn push_bounded_error(errors: &mut Vec<String>, error: String) {
    if errors.len() < 100 {
        errors.push(error);
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::services::library::LibraryCategory;

    #[tokio::test]
    async fn cancelled_import_records_a_clean_cancelled_result() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("Ravyn");
        let config = Config::try_parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().join("data").to_str().unwrap(),
            "--library-root",
            root.to_str().unwrap(),
        ])
        .unwrap();
        config.prepare_directories().await.unwrap();
        let repository = Repository::connect(&config.database_url()).await.unwrap();
        let source = root.join("Documents");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::write(source.join("manual.pdf"), b"content")
            .await
            .unwrap();
        let request = LibraryImportRequest {
            path: source,
            tags: Vec::new(),
            max_entries: 100,
            max_depth: 8,
        };
        let status = Arc::new(RwLock::new(LibraryImportStatus::default()));
        reserve_import(&config, &request, &status).await.unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let result = import_directory(
            Arc::new(config),
            repository,
            request,
            status.clone(),
            cancellation,
        )
        .await;
        assert!(matches!(result, Err(RavynError::Cancelled)));
        let snapshot = status.read().await;
        assert!(!snapshot.running);
        assert!(snapshot.cancelled);
        assert!(snapshot.errors.is_empty());
    }

    #[tokio::test]
    async fn import_reports_when_the_entry_limit_truncates_the_scan() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("Ravyn");
        let config = Config::try_parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().join("data").to_str().unwrap(),
            "--library-root",
            root.to_str().unwrap(),
        ])
        .unwrap();
        config.prepare_directories().await.unwrap();
        let repository = Repository::connect(&config.database_url()).await.unwrap();
        let source = root.join("Documents");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::write(source.join("one.pdf"), b"one").await.unwrap();
        tokio::fs::write(source.join("two.pdf"), b"two").await.unwrap();
        let request = LibraryImportRequest {
            path: source,
            tags: Vec::new(),
            max_entries: 1,
            max_depth: 8,
        };
        let status = Arc::new(RwLock::new(LibraryImportStatus::default()));
        reserve_import(&config, &request, &status).await.unwrap();
        import_directory(
            Arc::new(config),
            repository,
            request,
            status.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        let snapshot = status.read().await;
        assert!(snapshot.truncated);
        assert!(!snapshot.cancelled);
    }

    #[tokio::test]
    async fn import_verify_and_relocation_repair_round_trip() {
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
        let source = root.join("Documents/manual.pdf");
        tokio::fs::write(&source, b"%PDF-1.7\ncontent")
            .await
            .unwrap();
        let request = LibraryImportRequest {
            path: root.join("Documents"),
            tags: vec!["imported".into()],
            max_entries: 100,
            max_depth: 8,
        };
        let status = Arc::new(RwLock::new(LibraryImportStatus::default()));
        let reserved = reserve_import(&config, &request, &status).await.unwrap();
        assert!(reserved.running);
        import_directory(
            Arc::new(config.clone()),
            repository.clone(),
            request,
            status.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(status.read().await.imported, 1);

        let entries = all_entries(&repository, Some(LibraryEntryState::Active))
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, LibraryCategory::Documents);

        let moved = root.join("Downloads/manual-moved.pdf");
        tokio::fs::rename(&source, &moved).await.unwrap();
        let verified = verify_entries(&repository).await.unwrap();
        assert_eq!(verified.missing, 1);
        let repaired = repair_relocations(
            &config,
            &repository,
            RelocationRequest::default(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(repaired.repaired, 1);
        assert_eq!(
            repository
                .get_library_entry(entries[0].id)
                .await
                .unwrap()
                .path,
            moved
        );
    }
}
