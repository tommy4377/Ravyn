//! Persistent storage for component installation state and feature selections.

use std::collections::BTreeMap;

use chrono::Utc;
use sqlx::Row;

use crate::{
    error::Result,
    services::components::{ComponentId, ComponentState, PersistedComponent, SetupProfile},
    storage::Repository,
};

impl Repository {
    /// Load all persisted component records.
    pub async fn load_component_records(
        &self,
    ) -> Result<BTreeMap<ComponentId, PersistedComponent>> {
        let rows = sqlx::query("SELECT component,state,managed_version,managed_path,custom_path,error_message,last_checked_at,install_started_at,install_completed_at FROM component_states")
            .fetch_all(self.pool())
            .await?;

        let mut map = BTreeMap::new();
        for row in rows {
            let component_str: String = row.try_get("component")?;
            let component = parse_component_id(&component_str)?;
            let state_str: String = row.try_get("state")?;
            let state = parse_component_state(&state_str)?;
            let managed_version: Option<String> = row.try_get("managed_version")?;
            let managed_path: Option<String> = row.try_get("managed_path")?;
            let custom_path: Option<String> = row.try_get("custom_path")?;
            let error_message: Option<String> = row.try_get("error_message")?;
            let last_checked_at: Option<chrono::DateTime<Utc>> = row.try_get("last_checked_at")?;
            let install_started_at: Option<chrono::DateTime<Utc>> =
                row.try_get("install_started_at")?;
            let install_completed_at: Option<chrono::DateTime<Utc>> =
                row.try_get("install_completed_at")?;

            map.insert(
                component,
                PersistedComponent {
                    component,
                    state,
                    managed_version,
                    managed_path: managed_path.map(std::path::PathBuf::from),
                    custom_path: custom_path.map(std::path::PathBuf::from),
                    error_message,
                    last_checked_at,
                    install_started_at,
                    install_completed_at,
                },
            );
        }
        Ok(map)
    }

    /// Save or update a single component record.
    pub async fn save_component_record(&self, record: &PersistedComponent) -> Result<()> {
        let component = record.component.engine_name();
        let state = serde_json::to_string(&record.state)?;
        let state_trimmed = state.trim_matches('"');
        let managed_version = &record.managed_version;
        let managed_path = record
            .managed_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let custom_path = record
            .custom_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let error_message = &record.error_message;
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO component_states(component,state,managed_version,managed_path,custom_path,error_message,last_checked_at,install_started_at,install_completed_at,updated_at)
             VALUES(?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(component) DO UPDATE SET
               state=excluded.state,
               managed_version=excluded.managed_version,
               managed_path=excluded.managed_path,
               custom_path=excluded.custom_path,
               error_message=excluded.error_message,
               last_checked_at=excluded.last_checked_at,
               install_started_at=excluded.install_started_at,
               install_completed_at=excluded.install_completed_at,
               updated_at=excluded.updated_at",
        )
        .bind(component)
        .bind(state_trimmed)
        .bind(managed_version)
        .bind(managed_path)
        .bind(custom_path)
        .bind(error_message)
        .bind(record.last_checked_at)
        .bind(record.install_started_at)
        .bind(record.install_completed_at)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Delete a component record.
    pub async fn delete_component_record(&self, component: ComponentId) -> Result<()> {
        sqlx::query("DELETE FROM component_states WHERE component=?")
            .bind(component.engine_name())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Load the persisted feature selections (setup profile + features JSON).
    pub async fn load_feature_selections(&self) -> Result<Option<(SetupProfile, Vec<String>)>> {
        let row =
            sqlx::query("SELECT setup_profile, features_json FROM feature_selections WHERE id=1")
                .fetch_optional(self.pool())
                .await?;

        row.map(|row| {
            let profile_str: String = row.try_get("setup_profile")?;
            let profile = parse_setup_profile(&profile_str)?;
            let features_json: String = row.try_get("features_json")?;
            let features: Vec<String> = serde_json::from_str(&features_json)?;
            Ok((profile, features))
        })
        .transpose()
    }

    /// Save feature selections.
    pub async fn save_feature_selections(
        &self,
        profile: SetupProfile,
        features: &[String],
    ) -> Result<()> {
        let profile_str = serde_json::to_string(&profile)?;
        let profile_trimmed = profile_str.trim_matches('"');
        let features_json = serde_json::to_string(features)?;
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO feature_selections(id,setup_profile,features_json,updated_at)
             VALUES(1,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               setup_profile=excluded.setup_profile,
               features_json=excluded.features_json,
               updated_at=excluded.updated_at",
        )
        .bind(profile_trimmed)
        .bind(features_json)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

fn parse_component_id(s: &str) -> Result<ComponentId> {
    match s {
        "yt-dlp" => Ok(ComponentId::Ytdlp),
        "ffmpeg" => Ok(ComponentId::Ffmpeg),
        "rqbit" => Ok(ComponentId::Rqbit),
        "7zip" => Ok(ComponentId::SevenZip),
        _ => Err(crate::error::RavynError::Invalid(format!(
            "unknown component id: {s}"
        ))),
    }
}

fn parse_component_state(s: &str) -> Result<ComponentState> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|_| crate::error::RavynError::Invalid(format!("unknown component state: {s}")))
}

fn parse_setup_profile(s: &str) -> Result<SetupProfile> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|_| crate::error::RavynError::Invalid(format!("unknown setup profile: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_component_ids() {
        assert_eq!(parse_component_id("yt-dlp").unwrap(), ComponentId::Ytdlp);
        assert_eq!(parse_component_id("ffmpeg").unwrap(), ComponentId::Ffmpeg);
        assert_eq!(parse_component_id("rqbit").unwrap(), ComponentId::Rqbit);
        assert_eq!(parse_component_id("7zip").unwrap(), ComponentId::SevenZip);
        assert!(parse_component_id("unknown").is_err());
    }

    #[test]
    fn parse_component_states() {
        assert_eq!(
            parse_component_state("not_installed").unwrap(),
            ComponentState::NotInstalled
        );
        assert_eq!(
            parse_component_state("installed").unwrap(),
            ComponentState::Installed
        );
        assert_eq!(
            parse_component_state("downloading").unwrap(),
            ComponentState::Downloading
        );
        assert!(parse_component_state("unknown").is_err());
    }
}
