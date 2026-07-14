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
    pub installation: Option<InstallationRecord>,
}

/// Persisted result of the desktop shell's Windows installation/integration
/// step (application copy, shortcuts, registration), reported by the Tauri
/// setup flow so the backend — not just the frontend — knows whether the
/// app is installed, portable, and whether a relaunch is still pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRecord {
    pub installation_mode: String,
    pub installed_exe: Option<String>,
    pub installed_version: Option<String>,
    pub installed_sha256: Option<String>,
    pub integration_completed: bool,
    pub integration_errors: Vec<String>,
    pub relaunch_pending: bool,
}

impl Repository {
    /// Load the persisted setup state, if any.
    pub async fn load_setup_state(&self) -> Result<Option<SetupStateRecord>> {
        let row = sqlx::query(
            "SELECT completed, completed_at, app_version, library_root, installation_mode,
                    installed_exe, installed_version, installed_sha256, integration_completed,
                    integration_errors, relaunch_pending
             FROM setup_state WHERE id=1",
        )
        .fetch_optional(self.pool())
        .await?;

        row.map(|row| {
            let completed: i64 = row.try_get("completed")?;
            let completed_at: Option<chrono::DateTime<Utc>> = row.try_get("completed_at")?;
            let app_version: Option<String> = row.try_get("app_version")?;
            let library_root: Option<String> = row.try_get("library_root")?;
            let installation_mode: Option<String> = row.try_get("installation_mode")?;
            let integration_errors_json: Option<String> = row.try_get("integration_errors")?;
            let installation = installation_mode.map(|installation_mode| {
                let integration_errors = integration_errors_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                    .unwrap_or_default();
                Ok::<_, crate::error::RavynError>(InstallationRecord {
                    installation_mode,
                    installed_exe: row.try_get("installed_exe")?,
                    installed_version: row.try_get("installed_version")?,
                    installed_sha256: row.try_get("installed_sha256")?,
                    integration_completed: row.try_get::<i64, _>("integration_completed")? != 0,
                    integration_errors,
                    relaunch_pending: row.try_get::<i64, _>("relaunch_pending")? != 0,
                })
            });
            Ok(SetupStateRecord {
                completed: completed != 0,
                completed_at,
                app_version,
                library_root,
                installation: installation.transpose()?,
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

    /// Persist the desktop shell's Windows installation/integration result.
    /// Idempotent: reporting the same or a corrected result simply overwrites
    /// the prior one. Does not touch `completed`/`completed_at`.
    pub async fn save_installation_report(&self, installation: &InstallationRecord) -> Result<()> {
        let now = Utc::now();
        let integration_errors_json = serde_json::to_string(&installation.integration_errors)?;
        sqlx::query(
            "INSERT INTO setup_state(
                id, completed, installation_mode, installed_exe, installed_version,
                installed_sha256, integration_completed, integration_errors,
                relaunch_pending, updated_at)
             VALUES(1,0,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               installation_mode=excluded.installation_mode,
               installed_exe=excluded.installed_exe,
               installed_version=excluded.installed_version,
               installed_sha256=excluded.installed_sha256,
               integration_completed=excluded.integration_completed,
               integration_errors=excluded.integration_errors,
               relaunch_pending=excluded.relaunch_pending,
               updated_at=excluded.updated_at",
        )
        .bind(&installation.installation_mode)
        .bind(&installation.installed_exe)
        .bind(&installation.installed_version)
        .bind(&installation.installed_sha256)
        .bind(installation.integration_completed)
        .bind(integration_errors_json)
        .bind(installation.relaunch_pending)
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

    #[tokio::test]
    async fn installation_report_round_trips_independently_of_completion() {
        let temp = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", temp.path().join("test.sqlite3").display());
        let repository = Repository::connect(&url).await.unwrap();

        let report = InstallationRecord {
            installation_mode: "installed".into(),
            installed_exe: Some(r"C:\Users\Test\AppData\Local\Ravyn\Ravyn.exe".into()),
            installed_version: Some("0.2.0".into()),
            installed_sha256: Some("a".repeat(64)),
            integration_completed: true,
            integration_errors: vec!["desktop shortcut failed".into()],
            relaunch_pending: true,
        };
        repository.save_installation_report(&report).await.unwrap();

        // Reporting installation before setup completes must not mark it
        // completed, and must not require a prior setup_state row.
        let state = repository.load_setup_state().await.unwrap().unwrap();
        assert!(!state.completed);
        assert_eq!(state.installation.as_ref(), Some(&report));

        // Completing setup afterward must preserve the installation report.
        repository
            .save_setup_complete("0.2.0", Some("C:/Users/Test/Downloads/Ravyn"))
            .await
            .unwrap();
        let state = repository.load_setup_state().await.unwrap().unwrap();
        assert!(state.completed);
        assert_eq!(state.installation.as_ref(), Some(&report));

        // Re-reporting overwrites the prior installation result.
        let corrected = InstallationRecord {
            relaunch_pending: false,
            integration_errors: Vec::new(),
            ..report
        };
        repository
            .save_installation_report(&corrected)
            .await
            .unwrap();
        let state = repository.load_setup_state().await.unwrap().unwrap();
        assert!(state.completed, "completion must survive a later report");
        assert_eq!(state.installation, Some(corrected));
    }
}
