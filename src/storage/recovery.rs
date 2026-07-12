use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    storage::Repository,
};

const PENDING_DATABASE: &str = ".ravyn-restore-pending.sqlite3";
const PENDING_REQUEST: &str = ".ravyn-restore-request.json";
const ROLLBACK_DATABASE: &str = ".ravyn-restore-rollback.sqlite3";
const LAST_RESULT: &str = "restore-last-result.json";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestorePhase {
    #[default]
    Staged,
    Applied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRestore {
    pub backup_name: String,
    pub requested_at: DateTime<Utc>,
    #[serde(default)]
    pub phase: RestorePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResultRecord {
    pub backup_name: String,
    pub outcome: String,
    pub completed_at: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreStatus {
    pub pending: Option<PendingRestore>,
    pub last_result: Option<RestoreResultRecord>,
    pub restart_required: bool,
}

pub(crate) struct AppliedRestore {
    request: PendingRestore,
    rollback_exists: bool,
}

pub async fn schedule(
    data_dir: &Path,
    backup_path: &Path,
    backup_name: &str,
) -> Result<RestoreStatus> {
    validate_backup_name(backup_name)?;
    if read_json_optional::<PendingRestore>(&pending_request_path(data_dir))
        .await?
        .is_some()
        || pending_database_path(data_dir).exists()
    {
        return Err(RavynError::Conflict(
            "a database restore is already pending".into(),
        ));
    }
    let integrity = Repository::verify_database_file(backup_path).await?;
    if integrity != "ok" {
        return Err(RavynError::Invalid(format!(
            "backup integrity check failed: {integrity}"
        )));
    }

    let temporary = data_dir.join(format!(".restore-stage-{}.sqlite3", Uuid::new_v4()));
    copy_and_sync(backup_path, &temporary).await?;
    tokio::fs::rename(&temporary, pending_database_path(data_dir)).await?;

    let request = PendingRestore {
        backup_name: backup_name.to_owned(),
        requested_at: Utc::now(),
        phase: RestorePhase::Staged,
    };
    write_json_atomic(&pending_request_path(data_dir), &request).await?;
    status(data_dir).await
}

pub async fn cancel(data_dir: &Path) -> Result<RestoreStatus> {
    let pending = read_json_optional::<PendingRestore>(&pending_request_path(data_dir)).await?;
    if pending
        .as_ref()
        .is_some_and(|request| request.phase == RestorePhase::Applied)
    {
        return Err(RavynError::Conflict(
            "an applied database restore cannot be cancelled while the backend is starting".into(),
        ));
    }
    remove_json_state(&pending_request_path(data_dir)).await?;
    remove_if_exists(&pending_database_path(data_dir)).await?;
    status(data_dir).await
}

pub async fn status(data_dir: &Path) -> Result<RestoreStatus> {
    let pending = read_json_optional::<PendingRestore>(&pending_request_path(data_dir)).await?;
    let last_result =
        read_json_optional::<RestoreResultRecord>(&last_result_path(data_dir)).await?;
    Ok(RestoreStatus {
        restart_required: pending
            .as_ref()
            .is_some_and(|request| request.phase == RestorePhase::Staged),
        pending,
        last_result,
    })
}

pub(crate) async fn apply_pending(data_dir: &Path) -> Result<Option<AppliedRestore>> {
    let request = read_json_optional::<PendingRestore>(&pending_request_path(data_dir)).await?;
    let pending_path = pending_database_path(data_dir);
    let pending_exists = tokio::fs::try_exists(&pending_path).await?;

    let Some(mut request) = request else {
        if pending_exists {
            return Err(RavynError::Internal(
                "a staged database exists without a restore request".into(),
            ));
        }
        return Ok(None);
    };

    let database = database_path(data_dir);
    let rollback = rollback_path(data_dir);

    match request.phase {
        RestorePhase::Applied => {
            if pending_exists {
                if sqlite_bundle_exists(&database).await? {
                    return Err(RavynError::Internal(
                        "an applied restore has both an active and staged database".into(),
                    ));
                }
                let integrity = Repository::verify_database_file(&pending_path).await?;
                if integrity != "ok" {
                    return Err(RavynError::Invalid(
                        "the staged database backup is not valid".into(),
                    ));
                }
                tokio::fs::rename(&pending_path, &database).await?;
            }
            if !sqlite_bundle_exists(&database).await? {
                return Err(RavynError::Internal(
                    "an applied restore does not contain an active database".into(),
                ));
            }
            let rollback_exists = sqlite_bundle_exists(&rollback).await?;
            Ok(Some(AppliedRestore {
                request,
                rollback_exists,
            }))
        }
        RestorePhase::Staged => {
            if !pending_exists {
                return Err(RavynError::Internal(
                    "a restore request exists without its staged database".into(),
                ));
            }

            let integrity = Repository::verify_database_file(&pending_path).await?;
            if integrity != "ok" {
                record_result(
                    data_dir,
                    RestoreResultRecord {
                        backup_name: request.backup_name.clone(),
                        outcome: "failure".into(),
                        completed_at: Utc::now(),
                        message: "the staged backup failed its startup integrity check".into(),
                    },
                )
                .await?;
                remove_if_exists(&pending_path).await?;
                remove_json_state(&pending_request_path(data_dir)).await?;
                return Err(RavynError::Invalid(
                    "the staged database backup is not valid".into(),
                ));
            }

            let database_exists = sqlite_bundle_exists(&database).await?;
            let mut rollback_exists = sqlite_bundle_exists(&rollback).await?;
            match (database_exists, rollback_exists) {
                (true, false) => {
                    move_sqlite_bundle(&database, &rollback).await?;
                    rollback_exists = true;
                }
                (false, true) => {
                    // A previous startup was interrupted after moving the old
                    // database but before advancing the restore marker.
                }
                (false, false) => {
                    // First database initialization: there is no old database
                    // to preserve.
                }
                (true, true) => {
                    return Err(RavynError::Conflict(
                        "both the active and rollback database bundles exist during a staged restore"
                            .into(),
                    ));
                }
            }

            request.phase = RestorePhase::Applied;
            if let Err(error) = write_json_atomic(&pending_request_path(data_dir), &request).await {
                if rollback_exists && !sqlite_bundle_exists(&database).await? {
                    let _ = move_sqlite_bundle(&rollback, &database).await;
                }
                return Err(error);
            }

            if let Err(error) = tokio::fs::rename(&pending_path, &database).await {
                request.phase = RestorePhase::Staged;
                let _ = write_json_atomic(&pending_request_path(data_dir), &request).await;
                if rollback_exists && !sqlite_bundle_exists(&database).await? {
                    let _ = move_sqlite_bundle(&rollback, &database).await;
                }
                return Err(error.into());
            }

            Ok(Some(AppliedRestore {
                request,
                rollback_exists,
            }))
        }
    }
}

pub(crate) async fn finalize(data_dir: &Path, applied: AppliedRestore) -> Result<()> {
    if applied.rollback_exists {
        let backups = data_dir.join("backups");
        tokio::fs::create_dir_all(&backups).await?;
        let archived = backups.join(format!(
            "pre-restore-{}-{}.sqlite3",
            Utc::now().format("%Y%m%dT%H%M%SZ"),
            Uuid::new_v4()
        ));
        move_sqlite_bundle(&rollback_path(data_dir), &archived).await?;
    }
    remove_json_state(&pending_request_path(data_dir)).await?;
    record_result(
        data_dir,
        RestoreResultRecord {
            backup_name: applied.request.backup_name,
            outcome: "success".into(),
            completed_at: Utc::now(),
            message: "database restore completed successfully".into(),
        },
    )
    .await
}

pub(crate) async fn rollback_after_open_failure(
    data_dir: &Path,
    applied: AppliedRestore,
) -> Result<()> {
    let database = database_path(data_dir);
    remove_sqlite_bundle(&database).await?;
    if applied.rollback_exists {
        move_sqlite_bundle(&rollback_path(data_dir), &database).await?;
    }
    remove_json_state(&pending_request_path(data_dir)).await?;
    record_result(
        data_dir,
        RestoreResultRecord {
            backup_name: applied.request.backup_name,
            outcome: "failure".into(),
            completed_at: Utc::now(),
            message: "the restored database could not be opened or migrated; the previous database was restored"
                .into(),
        },
    )
    .await
}

fn validate_backup_name(name: &str) -> Result<()> {
    if name.is_empty() || Path::new(name).components().count() != 1 || !name.ends_with(".sqlite3") {
        return Err(RavynError::Invalid("invalid backup name".into()));
    }
    Ok(())
}

fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ravyn.sqlite3")
}

fn pending_database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PENDING_DATABASE)
}

fn pending_request_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PENDING_REQUEST)
}

fn rollback_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ROLLBACK_DATABASE)
}

fn last_result_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LAST_RESULT)
}

fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

async fn sqlite_bundle_exists(database: &Path) -> Result<bool> {
    for path in [
        database.to_path_buf(),
        sqlite_sidecar(database, "-wal"),
        sqlite_sidecar(database, "-shm"),
    ] {
        if tokio::fs::try_exists(path).await? {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn move_sqlite_bundle(source: &Path, destination: &Path) -> Result<()> {
    for (from, to) in [
        (source.to_path_buf(), destination.to_path_buf()),
        (
            sqlite_sidecar(source, "-wal"),
            sqlite_sidecar(destination, "-wal"),
        ),
        (
            sqlite_sidecar(source, "-shm"),
            sqlite_sidecar(destination, "-shm"),
        ),
    ] {
        if tokio::fs::try_exists(&from).await? {
            tokio::fs::rename(from, to).await?;
        }
    }
    Ok(())
}

async fn remove_sqlite_bundle(database: &Path) -> Result<()> {
    remove_if_exists(database).await?;
    remove_if_exists(&sqlite_sidecar(database, "-wal")).await?;
    remove_if_exists(&sqlite_sidecar(database, "-shm")).await?;
    Ok(())
}

async fn copy_and_sync(source: &Path, destination: &Path) -> Result<()> {
    let mut input = tokio::fs::File::open(source).await?;
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .await?;
    tokio::io::copy(&mut input, &mut output).await?;
    output.sync_all().await?;
    Ok(())
}

async fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let backup = path.with_extension("bak");
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .await?;
    use tokio::io::AsyncWriteExt;
    output.write_all(&bytes).await?;
    output.sync_all().await?;

    remove_if_exists(&backup).await?;
    let had_previous = tokio::fs::try_exists(path).await?;
    if had_previous {
        tokio::fs::rename(path, &backup).await?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        if had_previous {
            let _ = tokio::fs::rename(&backup, path).await;
        }
        return Err(error.into());
    }
    remove_if_exists(&backup).await?;
    Ok(())
}

async fn read_json_optional<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    let backup = path.with_extension("bak");
    let selected = if tokio::fs::try_exists(path).await? {
        path
    } else if tokio::fs::try_exists(&backup).await? {
        &backup
    } else {
        return Ok(None);
    };
    let bytes = tokio::fs::read(selected).await?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

async fn remove_json_state(path: &Path) -> Result<()> {
    remove_if_exists(path).await?;
    remove_if_exists(&path.with_extension("bak")).await?;
    Ok(())
}

async fn record_result(data_dir: &Path, result: RestoreResultRecord) -> Result<()> {
    remove_if_exists(&last_result_path(data_dir)).await?;
    write_json_atomic(&last_result_path(data_dir), &result).await
}

async fn remove_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::config::{Config, PersistentSettings};

    #[tokio::test]
    async fn staged_restore_replaces_database_and_records_success() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let database_url = format!("sqlite://{}", data_dir.join("ravyn.sqlite3").display());
        let repository = Repository::connect(&database_url).await.unwrap();
        let config = Config::parse_from(["ravyn"]);
        let mut original = PersistentSettings::from_config(&config);
        original.max_active = 3;
        repository
            .save_persistent_settings(&original)
            .await
            .unwrap();
        let backups = data_dir.join("backups");
        tokio::fs::create_dir_all(&backups).await.unwrap();
        let backup = backups.join("known-good.sqlite3");
        repository.backup_to(&backup).await.unwrap();

        let mut changed = original.clone();
        changed.max_active = 17;
        repository.save_persistent_settings(&changed).await.unwrap();
        repository.pool().close().await;

        schedule(data_dir, &backup, "known-good.sqlite3")
            .await
            .unwrap();
        let applied = apply_pending(data_dir).await.unwrap().unwrap();
        let restored = Repository::connect(&database_url).await.unwrap();
        assert_eq!(
            restored
                .load_persistent_settings()
                .await
                .unwrap()
                .unwrap()
                .max_active,
            3
        );
        restored.pool().close().await;
        finalize(data_dir, applied).await.unwrap();
        let state = status(data_dir).await.unwrap();
        assert!(state.pending.is_none());
        assert_eq!(state.last_result.unwrap().outcome, "success");
    }

    #[tokio::test]
    async fn applied_marker_survives_a_restart_before_finalization() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let database_url = format!("sqlite://{}", data_dir.join("ravyn.sqlite3").display());
        let repository = Repository::connect(&database_url).await.unwrap();
        let backups = data_dir.join("backups");
        tokio::fs::create_dir_all(&backups).await.unwrap();
        let backup = backups.join("known-good.sqlite3");
        repository.backup_to(&backup).await.unwrap();
        repository.pool().close().await;

        schedule(data_dir, &backup, "known-good.sqlite3")
            .await
            .unwrap();
        let first = apply_pending(data_dir).await.unwrap().unwrap();
        assert!(first.rollback_exists);
        drop(first);

        let recovered = apply_pending(data_dir).await.unwrap().unwrap();
        assert_eq!(recovered.request.phase, RestorePhase::Applied);
        finalize(data_dir, recovered).await.unwrap();
        assert!(status(data_dir).await.unwrap().pending.is_none());
    }
}
