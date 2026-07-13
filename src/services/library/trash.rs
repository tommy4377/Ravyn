use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    error::{RavynError, Result},
    services::security,
    storage::{LibraryEntry, LibraryEntryState, Repository},
};

/// Moves an active library item into Ravyn's managed trash directory.
pub async fn move_to_trash(
    config: &Config,
    repository: &Repository,
    id: uuid::Uuid,
) -> Result<LibraryEntry> {
    let entry = repository.get_library_entry(id).await?;
    if entry.state == LibraryEntryState::Trashed {
        return Ok(entry);
    }
    if entry.state != LibraryEntryState::Active {
        return Err(RavynError::Conflict(
            "only active library entries can be moved to trash".into(),
        ));
    }
    security::validate_output_path(config, &entry.path)?;
    validate_regular_file(&entry.path)?;
    let root = config.effective_library_root().ok_or_else(|| {
        RavynError::Conflict("library trash requires RAVYN_LIBRARY_ROOT".into())
    })?;
    let trash_root = root.join("Trash");
    tokio::fs::create_dir_all(&trash_root).await?;
    let target = unique_trash_path(&trash_root, id, &entry.filename).await?;
    move_file(&entry.path, &target).await?;
    match repository
        .update_library_state(
            id,
            LibraryEntryState::Trashed,
            &entry.path,
            Some(&target),
        )
        .await
    {
        Ok(updated) => Ok(updated),
        Err(error) => {
            if let Err(rollback_error) = move_file(&target, &entry.path).await {
                tracing::error!(
                    %rollback_error,
                    entry_id = %id,
                    "failed to roll back a library trash move after the database update failed"
                );
            }
            Err(error)
        }
    }
}

/// Restores a trashed item to its original path.
pub async fn restore(
    config: &Config,
    repository: &Repository,
    id: uuid::Uuid,
) -> Result<LibraryEntry> {
    let entry = repository.get_library_entry(id).await?;
    if entry.state != LibraryEntryState::Trashed {
        return Err(RavynError::Conflict(
            "only trashed library entries can be restored".into(),
        ));
    }
    let trash_path = entry.trash_path.as_deref().ok_or_else(|| {
        RavynError::Internal("trashed library entry has no trash path".into())
    })?;
    security::validate_output_path(config, trash_path)?;
    security::validate_output_path(config, &entry.path)?;
    validate_regular_file(trash_path)?;
    if tokio::fs::try_exists(&entry.path).await?
        || repository
            .library_path_is_reserved_by_other(entry.id, &entry.path)
            .await?
    {
        return Err(RavynError::Conflict(format!(
            "restore destination is already reserved: {}",
            entry.path.display()
        )));
    }
    if let Some(parent) = entry.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    move_file(trash_path, &entry.path).await?;
    match repository
        .update_library_state(id, LibraryEntryState::Active, &entry.path, None)
        .await
    {
        Ok(updated) => Ok(updated),
        Err(error) => {
            if let Err(rollback_error) = move_file(&entry.path, trash_path).await {
                tracing::error!(
                    %rollback_error,
                    entry_id = %id,
                    "failed to roll back a library restore after the database update failed"
                );
            }
            Err(error)
        }
    }
}

/// Permanently removes the payload and its database record.
pub async fn purge(
    config: &Config,
    repository: &Repository,
    id: uuid::Uuid,
) -> Result<()> {
    let entry = repository.get_library_entry(id).await?;
    let payload = if entry.state == LibraryEntryState::Trashed {
        entry.trash_path.clone()
    } else {
        Some(entry.path.clone())
    };
    let mut staged_payload = None;
    if let Some(payload) = payload {
        security::validate_output_path(config, &payload)?;
        match tokio::fs::symlink_metadata(&payload).await {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                // Stage the file before deleting the database row. This allows a safe
                // rollback if SQLite rejects the purge after the filesystem move.
                let root = config.effective_library_root().ok_or_else(|| {
                    RavynError::Conflict("library purge requires a Ravyn library root".into())
                })?;
                let staging_root = root.join("Temporary").join("purge");
                tokio::fs::create_dir_all(&staging_root).await?;
                let staged = unique_trash_path(&staging_root, id, &entry.filename).await?;
                move_file(&payload, &staged).await?;
                staged_payload = Some((payload, staged));
            }
            Ok(_) => {
                return Err(RavynError::Conflict(
                    "library purge target is not a regular file".into(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    if let Err(error) = repository.purge_library_entry(id).await {
        if let Some((original, staged)) = staged_payload.as_ref() {
            if let Err(rollback_error) = move_file(staged, original).await {
                tracing::error!(
                    %rollback_error,
                    entry_id = %id,
                    "failed to roll back a staged library purge after the database update failed"
                );
            }
        }
        return Err(error);
    }

    if let Some((_, staged)) = staged_payload {
        if let Err(error) = tokio::fs::remove_file(&staged).await {
            tracing::warn!(
                %error,
                entry_id = %id,
                path = %staged.display(),
                "library entry was purged but its staged payload could not be deleted"
            );
        }
    }
    Ok(())
}

fn validate_regular_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(RavynError::Conflict(
            "library operation requires a regular non-symlink file".into(),
        ));
    }
    Ok(())
}

async fn unique_trash_path(root: &Path, id: uuid::Uuid, filename: &str) -> Result<PathBuf> {
    let sanitized = crate::services::filename::sanitize(filename);
    let initial = root.join(format!("{id}-{sanitized}"));
    if !tokio::fs::try_exists(&initial).await? {
        return Ok(initial);
    }
    for suffix in 1_u32..=10_000 {
        let candidate = root.join(format!("{id}-{suffix}-{sanitized}"));
        if !tokio::fs::try_exists(&candidate).await? {
            return Ok(candidate);
        }
    }
    Err(RavynError::Conflict(
        "could not allocate a unique trash path".into(),
    ))
}

async fn move_file(source: &Path, destination: &Path) -> Result<()> {
    match tokio::fs::rename(source, destination).await {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            if let Err(copy_error) = tokio::fs::copy(source, destination).await {
                let _ = tokio::fs::remove_file(destination).await;
                return Err(RavynError::Internal(format!(
                    "failed to move library file ({rename_error}); copy fallback failed ({copy_error})"
                )));
            }
            if let Err(remove_error) = tokio::fs::remove_file(source).await {
                if let Err(cleanup_error) = tokio::fs::remove_file(destination).await {
                    tracing::error!(
                        %cleanup_error,
                        path = %destination.display(),
                        "failed to remove a copied library destination after source deletion failed"
                    );
                }
                return Err(remove_error.into());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::{services::library::LibraryCategory, storage::NewLibraryEntry};

    #[tokio::test]
    async fn trash_and_restore_preserve_the_original_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("Ravyn");
        let config = Config::try_parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().join("data").to_str().unwrap(),
            "--download-dir",
            root.join("Downloads").to_str().unwrap(),
            "--library-root",
            root.to_str().unwrap(),
        ])
        .unwrap();
        config.prepare_directories().await.unwrap();
        let repository = Repository::connect(&config.database_url()).await.unwrap();
        let path = root.join("Documents/manual.pdf");
        tokio::fs::write(&path, b"content").await.unwrap();
        let entry = repository
            .upsert_library_entry(NewLibraryEntry {
                job_id: None,
                source_url: "https://example.test/manual.pdf".into(),
                mirrors: Vec::new(),
                sha256: None,
                size_bytes: Some(7),
                path: path.clone(),
                filename: "manual.pdf".into(),
                category: LibraryCategory::Documents,
                mime_type: Some("application/pdf".into()),
                media_metadata: serde_json::json!({}),
                torrent_metadata: serde_json::json!({}),
                tags: Vec::new(),
                trust: None,
                imported: true,
                downloaded_at: chrono::Utc::now(),
            })
            .await
            .unwrap();

        let trashed = move_to_trash(&config, &repository, entry.id).await.unwrap();
        assert!(!tokio::fs::try_exists(&path).await.unwrap());
        assert!(trashed.trash_path.as_ref().unwrap().is_file());

        let restored = restore(&config, &repository, entry.id).await.unwrap();
        assert_eq!(restored.path, path);
        assert!(restored.path.is_file());
        assert_eq!(restored.state, LibraryEntryState::Active);
    }

    #[tokio::test]
    async fn a_trashed_path_can_be_reused_without_losing_the_old_entry() {
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
        let path = root.join("Documents/manual.pdf");
        tokio::fs::write(&path, b"old").await.unwrap();
        let old = repository
            .upsert_library_entry(NewLibraryEntry {
                job_id: None,
                source_url: "https://example.test/old.pdf".into(),
                mirrors: Vec::new(),
                sha256: None,
                size_bytes: Some(3),
                path: path.clone(),
                filename: "manual.pdf".into(),
                category: LibraryCategory::Documents,
                mime_type: Some("application/pdf".into()),
                media_metadata: serde_json::json!({}),
                torrent_metadata: serde_json::json!({}),
                tags: Vec::new(),
                trust: None,
                imported: true,
                downloaded_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
        move_to_trash(&config, &repository, old.id).await.unwrap();

        tokio::fs::write(&path, b"new").await.unwrap();
        let new = repository
            .upsert_library_entry(NewLibraryEntry {
                job_id: None,
                source_url: "https://example.test/new.pdf".into(),
                mirrors: Vec::new(),
                sha256: None,
                size_bytes: Some(3),
                path: path.clone(),
                filename: "manual.pdf".into(),
                category: LibraryCategory::Documents,
                mime_type: Some("application/pdf".into()),
                media_metadata: serde_json::json!({}),
                torrent_metadata: serde_json::json!({}),
                tags: Vec::new(),
                trust: None,
                imported: true,
                downloaded_at: chrono::Utc::now(),
            })
            .await
            .unwrap();

        assert_ne!(old.id, new.id);
        assert_eq!(repository.get_library_entry(old.id).await.unwrap().state, LibraryEntryState::Trashed);
        assert!(restore(&config, &repository, old.id).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"new");
    }
}
