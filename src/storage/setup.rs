//! Persistent storage for the one-row setup completion state.

use chrono::Utc;
use sqlx::Row;

use crate::{error::Result, storage::Repository};

/// Persisted setup completion snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupStateRecord {
    pub completed: bool,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub app_version: Option<String>,
    pub library_root: Option<String>,
}

impl Repository {
    /// Load the persisted setup state, if any.
    pub async fn load_setup_state(&self) -> Result<Option<SetupStateRecord>> {
        let row = sqlx::query(
            "SELECT completed, completed_at, app_version, library_root FROM setup_state WHERE id=1",
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(|row| {
            let completed: i64 = row.try_get("completed")?;
            let completed_at: Option<chrono::DateTime<Utc>> = row.try_get("completed_at")?;
            let app_version: Option<String> = row.try_get("app_version")?;
            let library_root: Option<String> = row.try_get("library_root")?;
            Ok(SetupStateRecord {
                completed: completed != 0,
                completed_at,
                app_version,
                library_root,
            })
        })
        .transpose()
    }

    /// Mark setup as complete, recording the app version and library root.
    pub async fn save_setup_complete(
        &self,
        app_version: &str,
        library_root: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO setup_state(id,completed,completed_at,app_version,library_root,updated_at)
             VALUES(1,1,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               completed=1,
               completed_at=excluded.completed_at,
               app_version=excluded.app_version,
               library_root=excluded.library_root,
               updated_at=excluded.updated_at",
        )
        .bind(now)
        .bind(app_version)
        .bind(library_root)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn setup_state_round_trips_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", temp.path().join("test.sqlite3").display());
        let repository = Repository::connect(&url).await.unwrap();

        assert!(repository.load_setup_state().await.unwrap().is_none());

        repository
            .save_setup_complete("0.2.0", Some("C:/Users/Test/Downloads/Ravyn"))
            .await
            .unwrap();
        let state = repository.load_setup_state().await.unwrap().unwrap();
        assert!(state.completed);
        assert_eq!(state.app_version.as_deref(), Some("0.2.0"));
        assert_eq!(
            state.library_root.as_deref(),
            Some("C:/Users/Test/Downloads/Ravyn")
        );
        assert!(state.completed_at.is_some());

        // Re-completing must update, not fail.
        repository.save_setup_complete("0.3.0", None).await.unwrap();
        let state = repository.load_setup_state().await.unwrap().unwrap();
        assert_eq!(state.app_version.as_deref(), Some("0.3.0"));
        assert_eq!(state.library_root, None);
    }
}
