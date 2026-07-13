//! Reusable download preset persistence.

use std::{collections::BTreeMap, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::{DownloadOptions, DuplicatePolicy},
    error::{RavynError, Result},
    storage::Repository,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadPresetPayload {
    pub destination: Option<PathBuf>,
    pub filename_template: Option<String>,
    pub priority: Option<i32>,
    pub speed_limit_bps: Option<u64>,
    pub duplicate_policy: Option<DuplicatePolicy>,
    pub options: Option<DownloadOptions>,
    pub template_variables: BTreeMap<String, String>,
    pub scheduler: Option<serde_json::Value>,
    pub rules: Vec<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadPreset {
    pub id: Uuid,
    pub name: String,
    pub payload: DownloadPresetPayload,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutDownloadPreset {
    pub name: String,
    #[serde(default)]
    pub payload: DownloadPresetPayload,
}

impl Repository {
    pub async fn create_download_preset(
        &self,
        input: PutDownloadPreset,
    ) -> Result<DownloadPreset> {
        validate_preset(&input)?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO download_presets(id,name,payload_json,created_at,updated_at) VALUES(?,?,?,?,?)",
        )
        .bind(id.to_string())
        .bind(input.name.trim())
        .bind(serde_json::to_string(&input.payload)?)
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await
        .map_err(map_unique_conflict)?;
        self.get_download_preset(id).await
    }

    pub async fn update_download_preset(
        &self,
        id: Uuid,
        input: PutDownloadPreset,
    ) -> Result<DownloadPreset> {
        validate_preset(&input)?;
        let changed = sqlx::query(
            "UPDATE download_presets SET name=?,payload_json=?,updated_at=? WHERE id=?",
        )
        .bind(input.name.trim())
        .bind(serde_json::to_string(&input.payload)?)
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await
        .map_err(map_unique_conflict)?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("download preset {id}")));
        }
        self.get_download_preset(id).await
    }

    pub async fn get_download_preset(&self, id: Uuid) -> Result<DownloadPreset> {
        sqlx::query("SELECT * FROM download_presets WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_preset)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("download preset {id}")))
    }

    pub async fn list_download_presets(&self) -> Result<Vec<DownloadPreset>> {
        sqlx::query("SELECT * FROM download_presets ORDER BY name COLLATE NOCASE,id")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_preset)
            .collect()
    }

    pub async fn delete_download_preset(&self, id: Uuid) -> Result<()> {
        let changed = sqlx::query("DELETE FROM download_presets WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("download preset {id}")));
        }
        Ok(())
    }
}

fn validate_preset(input: &PutDownloadPreset) -> Result<()> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(RavynError::Invalid(
            "preset names must contain between 1 and 120 characters".into(),
        ));
    }
    if input.payload.rules.len() > 128 || input.payload.template_variables.len() > 128 {
        return Err(RavynError::Invalid(
            "presets may contain at most 128 rules or template variables".into(),
        ));
    }
    if let Some(template) = input.payload.filename_template.as_deref() {
        crate::services::library::render_template(template, &input.payload.template_variables)?;
    }
    Ok(())
}

fn row_to_preset(row: SqliteRow) -> Result<DownloadPreset> {
    Ok(DownloadPreset {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        name: row.try_get("name")?,
        payload: serde_json::from_str(&row.try_get::<String, _>("payload_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_unique_conflict(error: sqlx::Error) -> RavynError {
    if matches!(&error, sqlx::Error::Database(database) if database.is_unique_violation()) {
        RavynError::Conflict("a download preset with that name already exists".into())
    } else {
        error.into()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    async fn repository() -> (tempfile::TempDir, Repository) {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("ravyn.sqlite3");
        let repository = Repository::connect(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        (temporary, repository)
    }

    #[tokio::test]
    async fn preset_crud_round_trip_and_name_uniqueness() {
        let (_temporary, repository) = repository().await;
        let created = repository
            .create_download_preset(PutDownloadPreset {
                name: "Music".into(),
                payload: DownloadPresetPayload {
                    filename_template: Some("{artist}/{filename}".into()),
                    template_variables: BTreeMap::from([(
                        "artist".into(),
                        "Example".into(),
                    )]),
                    ..DownloadPresetPayload::default()
                },
            })
            .await
            .unwrap();
        assert_eq!(created.name, "Music");
        assert_eq!(repository.list_download_presets().await.unwrap().len(), 1);

        let conflict = repository
            .create_download_preset(PutDownloadPreset {
                name: "music".into(),
                payload: DownloadPresetPayload::default(),
            })
            .await
            .unwrap_err();
        assert!(matches!(conflict, RavynError::Conflict(_)));

        repository.delete_download_preset(created.id).await.unwrap();
        assert!(repository.list_download_presets().await.unwrap().is_empty());
    }
}
