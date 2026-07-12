//! Database backup, verification, and staged-restore control.

use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    storage::Repository,
};

use crate::core::manager::JobManager;

impl JobManager {
    pub async fn backup_database(&self) -> Result<std::path::PathBuf> {
        let directory = self.config.data_dir.join("backups");
        tokio::fs::create_dir_all(&directory).await?;
        let destination = directory.join(format!(
            "ravyn-{}-{}.sqlite3",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
            Uuid::new_v4()
        ));
        self.repository.backup_to(&destination).await?;
        Ok(destination)
    }

    pub async fn list_backups(&self) -> Result<Vec<serde_json::Value>> {
        let directory = self.config.data_dir.join("backups");
        tokio::fs::create_dir_all(&directory).await?;
        let mut entries = tokio::fs::read_dir(&directory).await?;
        let mut backups = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("sqlite3")
            {
                backups.push(serde_json::json!({
                    "name": entry.file_name().to_string_lossy(),
                    "size_bytes": metadata.len(),
                    "modified_at": metadata.modified().ok().map(chrono::DateTime::<chrono::Utc>::from)
                }));
            }
        }
        backups.sort_by(|left, right| right["name"].as_str().cmp(&left["name"].as_str()));
        Ok(backups)
    }

    pub async fn verify_backup(&self, name: &str) -> Result<String> {
        if name.is_empty()
            || std::path::Path::new(name).components().count() != 1
            || !name.ends_with(".sqlite3")
        {
            return Err(RavynError::Invalid("invalid backup name".into()));
        }
        let path = self.config.data_dir.join("backups").join(name);
        if !tokio::fs::try_exists(&path).await? {
            return Err(RavynError::NotFound(format!("backup {name}")));
        }
        Repository::verify_database_file(&path).await
    }

    pub async fn schedule_database_restore(
        &self,
        name: &str,
    ) -> Result<crate::storage::recovery::RestoreStatus> {
        let integrity = self.verify_backup(name).await?;
        if integrity != "ok" {
            return Err(RavynError::Invalid(format!(
                "backup integrity check failed: {integrity}"
            )));
        }
        let path = self.config.data_dir.join("backups").join(name);
        crate::storage::recovery::schedule(&self.config.data_dir, &path, name).await
    }

    pub async fn database_restore_status(&self) -> Result<crate::storage::recovery::RestoreStatus> {
        crate::storage::recovery::status(&self.config.data_dir).await
    }

    pub async fn cancel_database_restore(&self) -> Result<crate::storage::recovery::RestoreStatus> {
        crate::storage::recovery::cancel(&self.config.data_dir).await
    }
}
