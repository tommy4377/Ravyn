//! User profile persistence and active-profile selection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    config::PersistentSettingsPatch,
    error::{RavynError, Result},
    storage::Repository,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub name: String,
    pub settings_patch: PersistentSettingsPatch,
    pub default_preset_id: Option<Uuid>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutUserProfile {
    pub name: String,
    #[serde(default)]
    pub settings_patch: PersistentSettingsPatch,
    pub default_preset_id: Option<Uuid>,
}

impl Repository {
    pub async fn create_user_profile(&self, input: PutUserProfile) -> Result<UserProfile> {
        validate_profile(&input)?;
        if let Some(preset_id) = input.default_preset_id {
            self.get_download_preset(preset_id).await?;
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query("INSERT INTO user_profiles(id,name,settings_patch_json,default_preset_id,active,created_at,updated_at) VALUES(?,?,?,?,0,?,?)")
            .bind(id.to_string())
            .bind(input.name.trim())
            .bind(serde_json::to_string(&input.settings_patch)?)
            .bind(input.default_preset_id.map(|value| value.to_string()))
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await
            .map_err(map_unique_conflict)?;
        self.get_user_profile(id).await
    }

    pub async fn update_user_profile(
        &self,
        id: Uuid,
        input: PutUserProfile,
    ) -> Result<UserProfile> {
        validate_profile(&input)?;
        if let Some(preset_id) = input.default_preset_id {
            self.get_download_preset(preset_id).await?;
        }
        let changed = sqlx::query("UPDATE user_profiles SET name=?,settings_patch_json=?,default_preset_id=?,updated_at=? WHERE id=?")
            .bind(input.name.trim())
            .bind(serde_json::to_string(&input.settings_patch)?)
            .bind(input.default_preset_id.map(|value| value.to_string()))
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.pool())
            .await
            .map_err(map_unique_conflict)?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("user profile {id}")));
        }
        self.get_user_profile(id).await
    }

    pub async fn get_user_profile(&self, id: Uuid) -> Result<UserProfile> {
        sqlx::query("SELECT * FROM user_profiles WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_profile)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("user profile {id}")))
    }

    pub async fn list_user_profiles(&self) -> Result<Vec<UserProfile>> {
        sqlx::query("SELECT * FROM user_profiles ORDER BY active DESC,name COLLATE NOCASE,id")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_profile)
            .collect()
    }

    pub async fn get_active_user_profile(&self) -> Result<Option<UserProfile>> {
        sqlx::query("SELECT * FROM user_profiles WHERE active=1 LIMIT 1")
            .fetch_optional(self.pool())
            .await?
            .map(row_to_profile)
            .transpose()
    }

    pub async fn activate_user_profile(&self, id: Uuid) -> Result<UserProfile> {
        self.get_user_profile(id).await?;
        let mut transaction = self.pool().begin().await?;
        sqlx::query("UPDATE user_profiles SET active=0 WHERE active=1")
            .execute(&mut *transaction)
            .await?;
        let changed = sqlx::query("UPDATE user_profiles SET active=1,updated_at=? WHERE id=?")
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("user profile {id}")));
        }
        transaction.commit().await?;
        self.get_user_profile(id).await
    }

    /// Activates a profile and persists its merged runtime settings atomically.
    pub async fn activate_user_profile_with_settings(
        &self,
        id: Uuid,
        settings: &crate::config::PersistentSettings,
    ) -> Result<UserProfile> {
        self.get_user_profile(id).await?;
        let mut transaction = self.pool().begin().await?;
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO runtime_settings(id,settings_json,updated_at) VALUES(1,?,?) \
             ON CONFLICT(id) DO UPDATE SET settings_json=excluded.settings_json,updated_at=excluded.updated_at",
        )
        .bind(serde_json::to_string(settings)?)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE user_profiles SET active=0 WHERE active=1")
            .execute(&mut *transaction)
            .await?;
        let changed = sqlx::query("UPDATE user_profiles SET active=1,updated_at=? WHERE id=?")
            .bind(now)
            .bind(id.to_string())
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("user profile {id}")));
        }
        transaction.commit().await?;
        self.get_user_profile(id).await
    }

    pub async fn delete_user_profile(&self, id: Uuid) -> Result<()> {
        let changed = sqlx::query("DELETE FROM user_profiles WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("user profile {id}")));
        }
        Ok(())
    }
}

fn validate_profile(input: &PutUserProfile) -> Result<()> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(RavynError::Invalid(
            "profile names must contain between 1 and 120 characters".into(),
        ));
    }
    if let Some(overrides) = input.settings_patch.library_category_overrides.as_ref() {
        crate::services::library::validate_category_overrides(overrides)?;
    }
    let serialized = serde_json::to_vec(&input.settings_patch)?;
    if serialized.len() > 64 * 1024 {
        return Err(RavynError::Invalid(
            "profile settings patches may not exceed 64 KiB".into(),
        ));
    }
    Ok(())
}

fn row_to_profile(row: SqliteRow) -> Result<UserProfile> {
    Ok(UserProfile {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        name: row.try_get("name")?,
        settings_patch: serde_json::from_str(&row.try_get::<String, _>(
            "settings_patch_json",
        )?)?,
        default_preset_id: row
            .try_get::<Option<String>, _>("default_preset_id")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        active: row.try_get("active")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_unique_conflict(error: sqlx::Error) -> RavynError {
    if matches!(&error, sqlx::Error::Database(database) if database.is_unique_violation()) {
        RavynError::Conflict("a user profile with that name already exists".into())
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
    async fn activation_persists_settings_and_profile_in_one_operation() {
        use clap::Parser;

        let (temporary, repository) = repository().await;
        let profile = repository
            .create_user_profile(PutUserProfile {
                name: "Portable".into(),
                settings_patch: PersistentSettingsPatch {
                    max_active: Some(2),
                    ..PersistentSettingsPatch::default()
                },
                default_preset_id: None,
            })
            .await
            .unwrap();
        let config = crate::config::Config::try_parse_from([
            "ravyn",
            "--data-dir",
            temporary.path().join("data").to_str().unwrap(),
        ])
        .unwrap();
        let mut settings = crate::config::PersistentSettings::from_config(&config);
        settings.max_active = 2;

        let active = repository
            .activate_user_profile_with_settings(profile.id, &settings)
            .await
            .unwrap();
        let persisted = repository
            .load_persistent_settings()
            .await
            .unwrap()
            .unwrap();

        assert!(active.active);
        assert_eq!(persisted.max_active, 2);
        assert_eq!(
            repository.get_active_user_profile().await.unwrap().unwrap().id,
            profile.id
        );
    }

    #[tokio::test]
    async fn activating_a_profile_is_exclusive() {
        let (_temporary, repository) = repository().await;
        let home = repository
            .create_user_profile(PutUserProfile {
                name: "Home".into(),
                settings_patch: PersistentSettingsPatch {
                    max_active: Some(6),
                    ..PersistentSettingsPatch::default()
                },
                default_preset_id: None,
            })
            .await
            .unwrap();
        let laptop = repository
            .create_user_profile(PutUserProfile {
                name: "Laptop".into(),
                settings_patch: PersistentSettingsPatch {
                    max_active: Some(2),
                    ..PersistentSettingsPatch::default()
                },
                default_preset_id: None,
            })
            .await
            .unwrap();

        repository.activate_user_profile(home.id).await.unwrap();
        repository.activate_user_profile(laptop.id).await.unwrap();
        let active = repository.get_active_user_profile().await.unwrap().unwrap();
        assert_eq!(active.id, laptop.id);
        assert_eq!(
            repository
                .list_user_profiles()
                .await
                .unwrap()
                .into_iter()
                .filter(|profile| profile.active)
                .count(),
            1
        );
    }
}
