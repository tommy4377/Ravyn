use std::{collections::HashSet, path::{Path, PathBuf}, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    config::{Config, PersistentSettings},
    error::{RavynError, Result},
    storage::{LibraryEntry, LibraryEntryState, LibraryListFilter, Repository},
};

const COPY_BUFFER_BYTES: usize = 1024 * 1024;
const DISK_SPACE_RESERVE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryMoveConflictPolicy {
    #[default]
    Fail,
    ReuseIdentical,
}

impl LibraryMoveConflictPolicy {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fail => "fail",
            Self::ReuseIdentical => "reuse_identical",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "fail" => Ok(Self::Fail),
            "reuse_identical" => Ok(Self::ReuseIdentical),
            other => Err(RavynError::Internal(format!(
                "unknown library move conflict policy {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LibraryMoveState {
    #[default]
    Idle,
    Running,
    Cancelling,
    Cancelled,
    Failed,
    RestartRequired,
    Completed,
    RolledBack,
}

impl LibraryMoveState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::RestartRequired => "restart_required",
            Self::Completed => "completed",
            Self::RolledBack => "rolled_back",
        }
    }

    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "cancelling" => Ok(Self::Cancelling),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            "restart_required" => Ok(Self::RestartRequired),
            "completed" => Ok(Self::Completed),
            "rolled_back" => Ok(Self::RolledBack),
            other => Err(RavynError::Internal(format!(
                "unknown library move state {other}"
            ))),
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryMoveRequest {
    pub destination: PathBuf,
    #[serde(default)]
    pub conflict_policy: LibraryMoveConflictPolicy,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryMovePreflight {
    pub source_root: PathBuf,
    pub destination_root: PathBuf,
    pub total_files: usize,
    pub total_bytes: u64,
    pub copy_files: usize,
    pub copy_bytes: u64,
    pub reusable_files: usize,
    pub missing_files: usize,
    pub external_entries: usize,
    pub conflict_files: usize,
    pub available_bytes: Option<u64>,
    pub active_jobs: usize,
    pub import_running: bool,
    pub can_start: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryMoveStatus {
    pub run_id: Option<Uuid>,
    pub state: LibraryMoveState,
    pub source_root: Option<PathBuf>,
    pub destination_root: Option<PathBuf>,
    pub conflict_policy: LibraryMoveConflictPolicy,
    pub total_files: usize,
    pub total_bytes: u64,
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub verified_files: usize,
    pub reused_files: usize,
    pub missing_files: usize,
    pub external_entries: usize,
    pub conflict_files: usize,
    pub cancel_requested: bool,
    pub restart_required: bool,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Default for LibraryMoveStatus {
    fn default() -> Self {
        Self {
            run_id: None,
            state: LibraryMoveState::Idle,
            source_root: None,
            destination_root: None,
            conflict_policy: LibraryMoveConflictPolicy::Fail,
            total_files: 0,
            total_bytes: 0,
            copied_files: 0,
            copied_bytes: 0,
            verified_files: 0,
            reused_files: 0,
            missing_files: 0,
            external_entries: 0,
            conflict_files: 0,
            cancel_requested: false,
            restart_required: false,
            error: None,
            started_at: None,
            updated_at: None,
            completed_at: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LibraryMovePlan {
    pub id: Uuid,
    pub source_root: PathBuf,
    pub destination_root: PathBuf,
    pub conflict_policy: LibraryMoveConflictPolicy,
    pub items: Vec<LibraryMovePlanItem>,
    pub total_files: usize,
    pub total_bytes: u64,
    pub copy_files: usize,
    pub copy_bytes: u64,
    pub reusable_files: usize,
    pub missing_files: usize,
    pub external_entries: usize,
    pub conflict_files: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct LibraryMovePlanItem {
    pub entry_id: Uuid,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub source_entry_path: PathBuf,
    pub destination_entry_path: PathBuf,
    pub was_trashed: bool,
    pub expected_sha256: Option<String>,
    pub size_bytes: u64,
    pub missing: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct LibraryMoveItemRecord {
    pub entry_id: Uuid,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub source_entry_path: PathBuf,
    pub destination_entry_path: PathBuf,
    pub was_trashed: bool,
    pub expected_sha256: Option<String>,
    pub size_bytes: u64,
    pub state: String,
    pub created_destination: bool,
}

pub async fn preflight_library_move(
    config: &Config,
    repository: &Repository,
    request: &LibraryMoveRequest,
    active_jobs: usize,
    import_running: bool,
) -> Result<LibraryMovePreflight> {
    let plan = build_move_plan(config, repository, request).await?;
    let available_bytes = available_disk_bytes(&plan.destination_root);
    let mut issues = Vec::new();
    if active_jobs > 0 {
        issues.push(format!(
            "Pause or finish {active_jobs} active download{} before moving the Library.",
            if active_jobs == 1 { "" } else { "s" }
        ));
    }
    if import_running {
        issues.push("Wait for the current Library import to finish or cancel it first.".into());
    }
    if repository.library_move_blocks_new_jobs().await? {
        issues.push("Another Library move is already active or waiting for restart.".into());
    }
    if plan.conflict_files > 0 {
        issues.push(format!(
            "{} destination file{} conflict with the selected policy.",
            plan.conflict_files,
            if plan.conflict_files == 1 { "" } else { "s" }
        ));
    }
    let required = plan.copy_bytes.saturating_add(DISK_SPACE_RESERVE_BYTES);
    if available_bytes.is_some_and(|available| available < required) {
        issues.push(format!(
            "The destination needs at least {} additional bytes including the safety reserve.",
            required.saturating_sub(available_bytes.unwrap_or_default())
        ));
    }
    if plan.total_files == 0 && plan.missing_files == 0 {
        issues.push("No tracked Library files are stored under the current Library root.".into());
    }

    Ok(LibraryMovePreflight {
        source_root: plan.source_root,
        destination_root: plan.destination_root,
        total_files: plan.total_files,
        total_bytes: plan.total_bytes,
        copy_files: plan.copy_files,
        copy_bytes: plan.copy_bytes,
        reusable_files: plan.reusable_files,
        missing_files: plan.missing_files,
        external_entries: plan.external_entries,
        conflict_files: plan.conflict_files,
        available_bytes,
        active_jobs,
        import_running,
        can_start: issues.is_empty(),
        issues,
    })
}

pub async fn start_library_move(
    configured_config: Arc<Config>,
    repository: Repository,
    manager: Arc<crate::core::manager::JobManager>,
    request: LibraryMoveRequest,
    import_running: bool,
    cancellation: CancellationToken,
) -> Result<LibraryMoveStatus> {
    let active_jobs = manager.active_job_count().await;
    let preflight = preflight_library_move(
        &configured_config,
        &repository,
        &request,
        active_jobs,
        import_running,
    )
    .await?;
    if !preflight.can_start {
        return Err(RavynError::Conflict(preflight.issues.join(" ")));
    }
    let plan = build_move_plan(&configured_config, &repository, &request).await?;
    repository.create_library_move(&plan).await?;
    // The durable transaction blocks queued jobs from being claimed. Recheck
    // active work after the lock is visible so a job cannot slip between
    // preflight and worker startup.
    if manager.active_job_count().await > 0 {
        repository
            .finish_library_move(
                plan.id,
                LibraryMoveState::Failed,
                Some("an active download started before Library maintenance could begin"),
                false,
            )
            .await?;
        return Err(RavynError::Conflict(
            "an active download started before Library maintenance could begin".into(),
        ));
    }
    let initial = repository
        .latest_library_move_status()
        .await?
        .unwrap_or_default();
    let worker_repository = repository.clone();
    tokio::spawn(async move {
        if let Err(error) = execute_library_move(
            &configured_config,
            &worker_repository,
            plan.id,
            &cancellation,
        )
        .await
        {
            if !matches!(error, RavynError::Cancelled) {
                tracing::error!(%error, run_id=%plan.id, "Library move failed");
            }
        }
    });
    Ok(initial)
}

pub async fn execute_library_move(
    configured_config: &Config,
    repository: &Repository,
    run_id: Uuid,
    cancellation: &CancellationToken,
) -> Result<()> {
    repository.recalculate_library_move_progress(run_id).await?;
    let status = repository
        .get_library_move_status(run_id)
        .await?
        .ok_or_else(|| RavynError::NotFound(format!("library move {run_id}")))?;
    if status.cancel_requested || cancellation.is_cancelled() {
        rollback_unactivated_move(repository, run_id, LibraryMoveState::Cancelled, None).await?;
        return Err(RavynError::Cancelled);
    }
    if !matches!(status.state, LibraryMoveState::Running | LibraryMoveState::Cancelling) {
        return Ok(());
    }

    super::prepare_library_layout(
        status
            .destination_root
            .as_deref()
            .ok_or_else(|| RavynError::Internal("Library move has no destination root".into()))?,
    )
    .await?;

    let result = execute_library_move_inner(repository, run_id, status.conflict_policy, cancellation).await;
    match result {
        Ok(()) => {
            let mut settings = repository
                .load_persistent_settings()
                .await?
                .unwrap_or_else(|| PersistentSettings::from_config(configured_config));
            let destination = repository
                .get_library_move_status(run_id)
                .await?
                .and_then(|value| value.destination_root)
                .ok_or_else(|| RavynError::Internal("Library move lost its destination root".into()))?;
            settings.library_root = Some(destination);
            repository.activate_library_move(run_id, &settings).await?;
            Ok(())
        }
        Err(RavynError::Cancelled) => {
            rollback_unactivated_move(repository, run_id, LibraryMoveState::Cancelled, None).await?;
            Err(RavynError::Cancelled)
        }
        Err(error) => {
            let message = error.to_string();
            rollback_unactivated_move(
                repository,
                run_id,
                LibraryMoveState::Failed,
                Some(&message),
            )
            .await?;
            Err(error)
        }
    }
}

async fn execute_library_move_inner(
    repository: &Repository,
    run_id: Uuid,
    conflict_policy: LibraryMoveConflictPolicy,
    cancellation: &CancellationToken,
) -> Result<()> {
    let items = repository.list_library_move_items(run_id).await?;
    for item in items {
        if cancellation.is_cancelled() || repository.library_move_cancel_requested(run_id).await? {
            return Err(RavynError::Cancelled);
        }
        if matches!(item.state.as_str(), "verified" | "reused" | "missing") {
            continue;
        }
        if !tokio::fs::try_exists(&item.source_path).await? {
            repository
                .update_library_move_item(
                    run_id,
                    item.entry_id,
                    "missing",
                    item.expected_sha256.as_deref(),
                    false,
                    None,
                )
                .await?;
            repository.recalculate_library_move_progress(run_id).await?;
            continue;
        }
        validate_regular_file(&item.source_path)?;
        if let Some(parent) = item.destination_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let source_hash = match item.expected_sha256.as_deref() {
            Some(expected) => {
                let actual = crate::services::checksum::sha256(&item.source_path, cancellation).await?;
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(RavynError::Conflict(format!(
                        "{} changed after the Library move was planned",
                        item.source_path.display()
                    )));
                }
                actual
            }
            None => crate::services::checksum::sha256(&item.source_path, cancellation).await?,
        };

        if tokio::fs::try_exists(&item.destination_path).await? {
            validate_regular_file(&item.destination_path)?;
            let destination_hash =
                crate::services::checksum::sha256(&item.destination_path, cancellation).await?;
            if !destination_hash.eq_ignore_ascii_case(&source_hash) {
                return Err(RavynError::Conflict(format!(
                    "destination contains different data: {}",
                    item.destination_path.display()
                )));
            }
            let temporary = temporary_copy_path(&item.destination_path, run_id, item.entry_id)?;
            let recovered_commit = item.state == "committing"
                && committed_destination_is_owned(&temporary, &item.destination_path).await?;
            if !recovered_commit && conflict_policy != LibraryMoveConflictPolicy::ReuseIdentical {
                return Err(RavynError::Conflict(format!(
                    "destination already exists: {}",
                    item.destination_path.display()
                )));
            }
            if recovered_commit {
                let _ = remove_if_exists(&temporary).await;
            }
            repository
                .update_library_move_item(
                    run_id,
                    item.entry_id,
                    if recovered_commit { "verified" } else { "reused" },
                    Some(&source_hash),
                    recovered_commit,
                    None,
                )
                .await?;
            repository.recalculate_library_move_progress(run_id).await?;
            continue;
        }

        repository
            .update_library_move_item(
                run_id,
                item.entry_id,
                "copying",
                Some(&source_hash),
                false,
                None,
            )
            .await?;
        let temporary = temporary_copy_path(&item.destination_path, run_id, item.entry_id)?;
        remove_if_exists(&temporary).await?;
        let copied_hash = copy_and_hash(&item.source_path, &temporary, cancellation).await?;
        if !copied_hash.eq_ignore_ascii_case(&source_hash) {
            let _ = remove_if_exists(&temporary).await;
            return Err(RavynError::Internal(format!(
                "copied Library file failed verification: {}",
                item.destination_path.display()
            )));
        }
        repository
            .update_library_move_item(
                run_id,
                item.entry_id,
                "committing",
                Some(&source_hash),
                false,
                None,
            )
            .await?;
        commit_temporary_file(&temporary, &item.destination_path).await?;
        repository
            .update_library_move_item(
                run_id,
                item.entry_id,
                "verified",
                Some(&source_hash),
                true,
                None,
            )
            .await?;
        repository.recalculate_library_move_progress(run_id).await?;
    }
    Ok(())
}

pub async fn cancel_library_move(
    repository: &Repository,
    cancellation: Option<CancellationToken>,
) -> Result<LibraryMoveStatus> {
    let status = repository
        .active_library_move_status()
        .await?
        .ok_or_else(|| RavynError::Conflict("there is no active Library move to cancel".into()))?;
    if status.state == LibraryMoveState::RestartRequired {
        return Err(RavynError::Conflict(
            "the Library move is already committed; restart Ravyn to finish it".into(),
        ));
    }
    let run_id = status
        .run_id
        .ok_or_else(|| RavynError::Internal("active Library move has no run id".into()))?;
    repository.request_library_move_cancel(run_id).await?;
    if let Some(token) = cancellation {
        token.cancel();
    }
    repository
        .get_library_move_status(run_id)
        .await?
        .ok_or_else(|| RavynError::NotFound(format!("library move {run_id}")))
}

/// Resumes a copy interrupted before settings activation. Source files are
/// retained, so recovery can safely retry or roll back without data loss.
pub async fn recover_interrupted_library_move(
    configured_config: &Config,
    repository: &Repository,
) -> Result<()> {
    let Some(status) = repository.active_library_move_status().await? else {
        return Ok(());
    };
    if status.state == LibraryMoveState::RestartRequired {
        return Ok(());
    }
    let run_id = status
        .run_id
        .ok_or_else(|| RavynError::Internal("active Library move has no run id".into()))?;
    if status.cancel_requested || status.state == LibraryMoveState::Cancelling {
        rollback_unactivated_move(repository, run_id, LibraryMoveState::Cancelled, None).await?;
        return Ok(());
    }
    execute_library_move(
        configured_config,
        repository,
        run_id,
        &CancellationToken::new(),
    )
    .await
}

/// Verifies the committed destination, rolls back database/settings on failure,
/// and removes source copies only after a successful application restart.
pub async fn finalize_activated_library_move(repository: &Repository) -> Result<()> {
    let Some(status) = repository.active_library_move_status().await? else {
        return Ok(());
    };
    if status.state != LibraryMoveState::RestartRequired {
        return Ok(());
    }
    let run_id = status
        .run_id
        .ok_or_else(|| RavynError::Internal("active Library move has no run id".into()))?;
    let items = repository.list_library_move_items(run_id).await?;
    for item in &items {
        if item.state == "missing" {
            continue;
        }
        if !tokio::fs::try_exists(&item.destination_path).await? {
            rollback_activated_move(repository, run_id, "a destination file is missing").await?;
            return Ok(());
        }
        validate_regular_file(&item.destination_path)?;
        let Some(expected) = item.expected_sha256.as_deref() else {
            rollback_activated_move(repository, run_id, "a destination checksum is unavailable").await?;
            return Ok(());
        };
        let actual = crate::services::checksum::sha256(
            &item.destination_path,
            &CancellationToken::new(),
        )
        .await?;
        if !actual.eq_ignore_ascii_case(expected) {
            rollback_activated_move(repository, run_id, "destination verification failed").await?;
            return Ok(());
        }
    }

    for item in &items {
        if item.state == "missing" || !tokio::fs::try_exists(&item.source_path).await? {
            continue;
        }
        let destination_same_file = same_file_path(&item.source_path, &item.destination_path);
        if !destination_same_file {
            tokio::fs::remove_file(&item.source_path).await?;
            remove_empty_ancestors(
                item.source_path.parent(),
                status.source_root.as_deref(),
            )
            .await;
        }
        repository
            .update_library_move_item(
                run_id,
                item.entry_id,
                "source_removed",
                item.expected_sha256.as_deref(),
                item.created_destination,
                None,
            )
            .await?;
    }
    repository
        .finish_library_move(run_id, LibraryMoveState::Completed, None, false)
        .await?;
    Ok(())
}

async fn rollback_activated_move(
    repository: &Repository,
    run_id: Uuid,
    reason: &str,
) -> Result<()> {
    let status = repository
        .get_library_move_status(run_id)
        .await?
        .ok_or_else(|| RavynError::NotFound(format!("library move {run_id}")))?;
    let mut settings = repository
        .load_persistent_settings()
        .await?
        .ok_or_else(|| RavynError::Internal("persistent settings disappeared during rollback".into()))?;
    settings.library_root = status.source_root.clone();
    repository.rollback_library_move_activation(run_id, &settings, reason).await?;
    cleanup_created_destinations(repository, run_id).await?;
    Ok(())
}

async fn rollback_unactivated_move(
    repository: &Repository,
    run_id: Uuid,
    state: LibraryMoveState,
    error: Option<&str>,
) -> Result<()> {
    cleanup_created_destinations(repository, run_id).await?;
    repository
        .finish_library_move(run_id, state, error, false)
        .await
}

async fn cleanup_created_destinations(repository: &Repository, run_id: Uuid) -> Result<()> {
    let items = repository.list_library_move_items(run_id).await?;
    for item in items {
        let temporary = temporary_copy_path(&item.destination_path, run_id, item.entry_id)?;
        let _ = remove_if_exists(&temporary).await;
        let committed_before_journal = item.state == "committing"
            && committed_destination_is_owned(&temporary, &item.destination_path).await?;
        if item.created_destination || committed_before_journal {
            let _ = remove_if_exists(&item.destination_path).await;
        }
    }
    Ok(())
}

async fn build_move_plan(
    config: &Config,
    repository: &Repository,
    request: &LibraryMoveRequest,
) -> Result<LibraryMovePlan> {
    let source_root = canonical_existing_directory(
        &config
            .effective_library_root()
            .ok_or_else(|| RavynError::Conflict("the Library root is not configured".into()))?,
        "current Library root",
    )?;
    let destination_root = canonical_destination_directory(&request.destination)?;
    if same_file_path(&source_root, &destination_root) {
        return Err(RavynError::Invalid(
            "the destination is the current Library root".into(),
        ));
    }
    if destination_root.starts_with(&source_root) || source_root.starts_with(&destination_root) {
        return Err(RavynError::Invalid(
            "the current and destination Library roots may not contain each other".into(),
        ));
    }

    let entries = all_library_entries(repository).await?;
    let mut items = Vec::new();
    let mut total_files = 0_usize;
    let mut total_bytes = 0_u64;
    let mut copy_files = 0_usize;
    let mut copy_bytes = 0_u64;
    let mut reusable_files = 0_usize;
    let mut missing_files = 0_usize;
    let mut external_entries = 0_usize;
    let mut conflict_files = 0_usize;
    let mut destination_paths = HashSet::new();

    for entry in entries {
        let source_entry_path = absolute_existing_or_lexical(&entry.path)?;
        let Ok(entry_relative) = source_entry_path.strip_prefix(&source_root) else {
            external_entries += 1;
            continue;
        };
        if entry_relative.as_os_str().is_empty() {
            external_entries += 1;
            continue;
        }
        let destination_entry_path = destination_root.join(entry_relative);
        let was_trashed = entry.state == LibraryEntryState::Trashed;
        let payload = if was_trashed {
            entry.trash_path.as_deref().unwrap_or(&entry.path)
        } else {
            &entry.path
        };
        let source_path = absolute_existing_or_lexical(payload)?;
        let Ok(payload_relative) = source_path.strip_prefix(&source_root) else {
            external_entries += 1;
            continue;
        };
        if payload_relative.as_os_str().is_empty() {
            external_entries += 1;
            continue;
        }
        let destination_path = destination_root.join(payload_relative);
        if !destination_paths.insert(destination_path.clone()) {
            conflict_files += 1;
            continue;
        }
        let metadata = match std::fs::symlink_metadata(&source_path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Some(metadata),
            _ => None,
        };
        let missing = metadata.is_none() || entry.state == LibraryEntryState::Missing;
        let size_bytes = metadata
            .as_ref()
            .map(std::fs::Metadata::len)
            .or(entry.size_bytes)
            .unwrap_or_default();
        if missing {
            missing_files += 1;
        } else {
            total_files += 1;
            total_bytes = total_bytes.saturating_add(size_bytes);
            if destination_path.exists() {
                match request.conflict_policy {
                    LibraryMoveConflictPolicy::Fail => conflict_files += 1,
                    LibraryMoveConflictPolicy::ReuseIdentical => {
                        if destination_matches_entry(&destination_path, &entry).await? {
                            reusable_files += 1;
                        } else {
                            conflict_files += 1;
                        }
                    }
                }
            } else {
                copy_files += 1;
                copy_bytes = copy_bytes.saturating_add(size_bytes);
            }
        }
        items.push(LibraryMovePlanItem {
            entry_id: entry.id,
            source_path,
            destination_path,
            source_entry_path,
            destination_entry_path,
            was_trashed,
            expected_sha256: entry.sha256,
            size_bytes,
            missing,
        });
    }

    Ok(LibraryMovePlan {
        id: Uuid::new_v4(),
        source_root,
        destination_root,
        conflict_policy: request.conflict_policy,
        items,
        total_files,
        total_bytes,
        copy_files,
        copy_bytes,
        reusable_files,
        missing_files,
        external_entries,
        conflict_files,
    })
}

async fn destination_matches_entry(path: &Path, entry: &LibraryEntry) -> Result<bool> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        _ => return Ok(false),
    };
    if entry.size_bytes.is_some_and(|size| size != metadata.len()) {
        return Ok(false);
    }
    let Some(expected) = entry.sha256.as_deref() else {
        return Ok(false);
    };
    let actual = crate::services::checksum::sha256(path, &CancellationToken::new()).await?;
    Ok(actual.eq_ignore_ascii_case(expected))
}

async fn all_library_entries(repository: &Repository) -> Result<Vec<LibraryEntry>> {
    let mut offset = 0_u64;
    let mut entries = Vec::new();
    loop {
        let page = repository
            .list_library_entries(&LibraryListFilter::default(), offset, 200)
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

fn canonical_existing_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(RavynError::Invalid(format!(
            "{label} must be a non-symlink directory"
        )));
    }
    Ok(std::fs::canonicalize(path)?)
}

fn canonical_destination_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(RavynError::Invalid(
            "the new Library root must be an absolute path".into(),
        ));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(RavynError::Invalid(
            "the new Library root may not contain parent traversal".into(),
        ));
    }
    if path.exists() {
        return canonical_existing_directory(path, "new Library root");
    }
    let parent = path.parent().ok_or_else(|| {
        RavynError::Invalid("the new Library root has no parent directory".into())
    })?;
    let parent = canonical_existing_directory(parent, "new Library parent")?;
    let name = path.file_name().ok_or_else(|| {
        RavynError::Invalid("the new Library root has no directory name".into())
    })?;
    Ok(parent.join(name))
}

fn absolute_existing_or_lexical(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(std::fs::canonicalize(path)?);
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn validate_regular_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(RavynError::Conflict(format!(
            "Library move requires a regular non-symlink file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn temporary_copy_path(destination: &Path, run_id: Uuid, entry_id: Uuid) -> Result<PathBuf> {
    let parent = destination.parent().ok_or_else(|| {
        RavynError::Invalid(format!(
            "Library destination has no parent: {}",
            destination.display()
        ))
    })?;
    Ok(parent.join(format!(".ravyn-move-{run_id}-{entry_id}.part")))
}

async fn copy_and_hash(
    source: &Path,
    destination: &Path,
    cancellation: &CancellationToken,
) -> Result<String> {
    let mut input = tokio::fs::File::open(source).await?;
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        if cancellation.is_cancelled() {
            return Err(RavynError::Cancelled);
        }
        let read = input.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read]).await?;
        hasher.update(&buffer[..read]);
    }
    output.sync_all().await?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(windows)]
async fn commit_temporary_file(temporary: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    let temporary = temporary.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut source_wide: Vec<u16> = temporary.as_os_str().encode_wide().collect();
        source_wide.push(0);
        let mut destination_wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
        destination_wide.push(0);
        let moved = unsafe {
            MoveFileExW(
                source_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        };
        if moved == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    })
    .await
    .map_err(|error| RavynError::Internal(format!("Library move commit task failed: {error}")))?
}

#[cfg(unix)]
async fn commit_temporary_file(temporary: &Path, destination: &Path) -> Result<()> {
    // A hard link creates the destination atomically and never replaces an
    // existing file. The temporary name is removed only after the link exists.
    tokio::fs::hard_link(temporary, destination).await?;
    tokio::fs::remove_file(temporary).await?;
    Ok(())
}

async fn committed_destination_is_owned(temporary: &Path, destination: &Path) -> Result<bool> {
    if !tokio::fs::try_exists(destination).await? {
        return Ok(false);
    }
    if !tokio::fs::try_exists(temporary).await? {
        return Ok(true);
    }
    Ok(same_file_identity(temporary, destination))
}

#[cfg(unix)]
fn same_file_identity(left: &Path, right: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(left), std::fs::metadata(right)) {
        (Ok(left), Ok(right)) => left.dev() == right.dev() && left.ino() == right.ino(),
        _ => false,
    }
}

#[cfg(windows)]
fn same_file_identity(_left: &Path, _right: &Path) -> bool {
    // MoveFileExW removes the temporary name after a successful commit, so two
    // simultaneously existing paths cannot represent our completed commit.
    false
}

async fn remove_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn remove_empty_ancestors(mut directory: Option<&Path>, stop: Option<&Path>) {
    while let Some(path) = directory {
        if stop.is_some_and(|stop| same_file_path(path, stop)) {
            break;
        }
        match tokio::fs::remove_dir(path).await {
            Ok(()) => directory = path.parent(),
            Err(_) => break,
        }
    }
}

fn same_file_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(windows)]
fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let existing = nearest_existing_ancestor(path)?;
    let mut wide: Vec<u16> = existing.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free = 0_u64;
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (result != 0).then_some(free)
}

#[cfg(unix)]
fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let existing = nearest_existing_ancestor(path)?;
    let cstr = std::ffi::CString::new(existing.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(cstr.as_ptr(), &mut stat) };
    (result == 0).then(|| u64::from(stat.f_bavail).saturating_mul(u64::from(stat.f_frsize)))
}

fn nearest_existing_ancestor(path: &Path) -> Option<&Path> {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if candidate.exists() {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::{services::library::LibraryCategory, storage::NewLibraryEntry};

    async fn fixture() -> (tempfile::TempDir, Config, Repository, PathBuf, PathBuf) {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let destination = temporary.path().join("destination");
        tokio::fs::create_dir_all(source.join("Documents")).await.unwrap();
        let config = Config::try_parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().join("data").to_str().unwrap(),
            "--library-root",
            source.to_str().unwrap(),
        ])
        .unwrap();
        config.prepare_bootstrap_directories().await.unwrap();
        let repository = Repository::connect(&config.database_url()).await.unwrap();
        (temporary, config, repository, source, destination)
    }

    async fn add_entry(repository: &Repository, path: PathBuf, content: &[u8]) -> LibraryEntry {
        tokio::fs::write(&path, content).await.unwrap();
        let hash = crate::services::checksum::sha256(&path, &CancellationToken::new())
            .await
            .unwrap();
        repository
            .upsert_library_entry(NewLibraryEntry {
                job_id: None,
                source_url: "https://example.test/file".into(),
                mirrors: Vec::new(),
                sha256: Some(hash),
                size_bytes: Some(content.len() as u64),
                path: path.clone(),
                filename: path.file_name().unwrap().to_string_lossy().into_owned(),
                category: LibraryCategory::Documents,
                mime_type: None,
                media_metadata: serde_json::json!({}),
                torrent_metadata: serde_json::json!({}),
                tags: Vec::new(),
                trust: None,
                imported: false,
                downloaded_at: Utc::now(),
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn preflight_reports_copy_counts_and_external_entries() {
        let (_temporary, config, repository, source, destination) = fixture().await;
        add_entry(&repository, source.join("Documents/manual.pdf"), b"manual").await;
        let external = source.parent().unwrap().join("external.bin");
        add_entry(&repository, external, b"external").await;

        let report = preflight_library_move(
            &config,
            &repository,
            &LibraryMoveRequest {
                destination,
                conflict_policy: LibraryMoveConflictPolicy::Fail,
            },
            0,
            false,
        )
        .await
        .unwrap();
        assert!(report.can_start);
        assert_eq!(report.total_files, 1);
        assert_eq!(report.copy_files, 1);
        assert_eq!(report.external_entries, 1);
    }

    #[tokio::test]
    async fn move_activation_waits_for_restart_before_removing_sources() {
        let (_temporary, config, repository, source, destination) = fixture().await;
        let source_file = source.join("Documents/manual.pdf");
        let entry = add_entry(&repository, source_file.clone(), b"manual").await;
        let request = LibraryMoveRequest {
            destination: destination.clone(),
            conflict_policy: LibraryMoveConflictPolicy::Fail,
        };
        let plan = build_move_plan(&config, &repository, &request).await.unwrap();
        repository.create_library_move(&plan).await.unwrap();
        execute_library_move(&config, &repository, plan.id, &CancellationToken::new())
            .await
            .unwrap();
        assert!(source_file.exists());
        let moved = repository.get_library_entry(entry.id).await.unwrap();
        assert!(moved.path.starts_with(&destination));
        assert_eq!(
            repository.get_library_move_status(plan.id).await.unwrap().unwrap().state,
            LibraryMoveState::RestartRequired
        );

        finalize_activated_library_move(&repository).await.unwrap();
        assert!(!source_file.exists());
        assert!(moved.path.exists());
        assert_eq!(
            repository.get_library_move_status(plan.id).await.unwrap().unwrap().state,
            LibraryMoveState::Completed
        );
    }

    #[tokio::test]
    async fn committing_file_is_recovered_after_a_crash_window() {
        let (_temporary, config, repository, source, destination) = fixture().await;
        let source_file = source.join("Documents/manual.pdf");
        add_entry(&repository, source_file.clone(), b"manual").await;
        let request = LibraryMoveRequest {
            destination: destination.clone(),
            conflict_policy: LibraryMoveConflictPolicy::Fail,
        };
        let plan = build_move_plan(&config, &repository, &request).await.unwrap();
        repository.create_library_move(&plan).await.unwrap();
        let item = repository.list_library_move_items(plan.id).await.unwrap().remove(0);
        tokio::fs::create_dir_all(item.destination_path.parent().unwrap())
            .await
            .unwrap();
        let source_hash = crate::services::checksum::sha256(
            &source_file,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let temporary = temporary_copy_path(&item.destination_path, plan.id, item.entry_id).unwrap();
        copy_and_hash(&source_file, &temporary, &CancellationToken::new())
            .await
            .unwrap();
        repository
            .update_library_move_item(
                plan.id,
                item.entry_id,
                "committing",
                Some(&source_hash),
                false,
                None,
            )
            .await
            .unwrap();
        commit_temporary_file(&temporary, &item.destination_path)
            .await
            .unwrap();

        execute_library_move(&config, &repository, plan.id, &CancellationToken::new())
            .await
            .unwrap();
        let recovered = repository.list_library_move_items(plan.id).await.unwrap().remove(0);
        assert_eq!(recovered.state, "verified");
        assert!(recovered.created_destination);
        assert_eq!(
            repository.get_library_move_status(plan.id).await.unwrap().unwrap().state,
            LibraryMoveState::RestartRequired
        );
    }

    #[tokio::test]
    async fn trashed_entries_preserve_restore_and_payload_paths() {
        let (_temporary, config, repository, source, destination) = fixture().await;
        let source_file = source.join("Documents/manual.pdf");
        let entry = add_entry(&repository, source_file.clone(), b"manual").await;
        let trashed = super::super::move_to_trash(&config, &repository, entry.id)
            .await
            .unwrap();
        let source_trash = trashed.trash_path.clone().unwrap();
        assert!(!source_file.exists());
        assert!(source_trash.exists());

        let request = LibraryMoveRequest {
            destination: destination.clone(),
            conflict_policy: LibraryMoveConflictPolicy::Fail,
        };
        let plan = build_move_plan(&config, &repository, &request).await.unwrap();
        repository.create_library_move(&plan).await.unwrap();
        execute_library_move(&config, &repository, plan.id, &CancellationToken::new())
            .await
            .unwrap();

        let moved = repository.get_library_entry(entry.id).await.unwrap();
        assert_eq!(moved.path, destination.join("Documents/manual.pdf"));
        let destination_trash = moved.trash_path.clone().unwrap();
        assert!(destination_trash.starts_with(destination.join("Trash")));
        assert!(destination_trash.exists());
        assert!(source_trash.exists());

        finalize_activated_library_move(&repository).await.unwrap();
        assert!(!source_trash.exists());
        assert!(destination_trash.exists());
    }

    #[tokio::test]
    async fn cancellation_removes_created_destinations_and_keeps_sources() {
        let (_temporary, config, repository, source, destination) = fixture().await;
        let source_file = source.join("Documents/manual.pdf");
        add_entry(&repository, source_file.clone(), b"manual").await;
        let request = LibraryMoveRequest {
            destination: destination.clone(),
            conflict_policy: LibraryMoveConflictPolicy::Fail,
        };
        let plan = build_move_plan(&config, &repository, &request).await.unwrap();
        repository.create_library_move(&plan).await.unwrap();
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert!(matches!(
            execute_library_move(&config, &repository, plan.id, &cancellation).await,
            Err(RavynError::Cancelled)
        ));
        assert!(source_file.exists());
        assert!(!destination.join("Documents/manual.pdf").exists());
    }
}
