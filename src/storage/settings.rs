//! Persistent runtime settings stored alongside the queue.

use chrono::Utc;
use sqlx::Row;

use crate::{error::Result, storage::Repository};

impl Repository {
    pub async fn load_persistent_settings(
        &self,
    ) -> Result<Option<crate::config::PersistentSettings>> {
        let row = sqlx::query("SELECT settings_json FROM runtime_settings WHERE id=1")
            .fetch_optional(self.pool())
            .await?;
        row.map(|row| {
            let json: String = row.try_get("settings_json")?;
            serde_json::from_str(&json).map_err(Into::into)
        })
        .transpose()
    }

    pub async fn save_persistent_settings(
        &self,
        settings: &crate::config::PersistentSettings,
    ) -> Result<()> {
        sqlx::query("INSERT INTO runtime_settings(id,settings_json,updated_at) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET settings_json=excluded.settings_json,updated_at=excluded.updated_at")
            .bind(serde_json::to_string(settings)?)
            .bind(Utc::now())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn reset_persistent_settings(&self) -> Result<()> {
        sqlx::query("DELETE FROM runtime_settings WHERE id=1")
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
