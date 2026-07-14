//! Secret-reference records whose values live in the platform keyring.

use chrono::{DateTime, Utc};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    storage::{Repository, jobs::row_uuid},
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct SecretReference {
    pub id: Uuid,
    pub name: String,
    pub secret_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn validate_secret_value(secret_type: &str, secret: &str) -> Result<()> {
    if secret.is_empty() || secret.chars().all(char::is_whitespace) {
        return Err(RavynError::Invalid("secret value must not be empty".into()));
    }
    match secret_type {
        "proxy_credentials" => {
            reqwest::Proxy::all(secret.trim()).map_err(|error| {
                RavynError::Invalid(format!("proxy secret must contain a valid proxy URL: {error}"))
            })?;
        }
        "rqbit_credentials" => {
            #[derive(serde::Deserialize)]
            struct Credentials {
                username: String,
                password: String,
            }
            let credentials: Credentials = serde_json::from_str(secret).map_err(|_| {
                RavynError::Invalid(
                    "rqbit credential secret must be JSON with username and password".into(),
                )
            })?;
            if credentials.username.trim().is_empty() || credentials.password.is_empty() {
                return Err(RavynError::Invalid(
                    "rqbit credential secret contains an empty username or password".into(),
                ));
            }
        }
        "cookies" => {
            let cookies: std::collections::BTreeMap<String, String> =
                serde_json::from_str(secret).map_err(|_| {
                    RavynError::Invalid(
                        "cookie secret must be a JSON object of string name/value pairs".into(),
                    )
                })?;
            if cookies.keys().any(|name| name.trim().is_empty()) {
                return Err(RavynError::Invalid(
                    "cookie secret contains an empty cookie name".into(),
                ));
            }
        }
        "authentication_header" => {
            axum::http::HeaderValue::try_from(secret).map_err(|_| {
                RavynError::Invalid(
                    "authorization header secret contains invalid header characters".into(),
                )
            })?;
        }
        _ => {}
    }
    Ok(())
}

impl Repository {
    pub async fn put_secret_reference(
        &self,
        name: &str,
        secret_type: &str,
        secret: String,
    ) -> Result<SecretReference> {
        const TYPES: &[&str] = &[
            "api_token",
            "proxy_credentials",
            "rqbit_credentials",
            "cookies",
            "authentication_header",
            "tls_certificate",
            "private_key",
        ];
        let name = name.trim();
        if name.is_empty() || name.len() > 160 || !TYPES.contains(&secret_type) {
            return Err(RavynError::Invalid("invalid secret name or type".into()));
        }
        validate_secret_value(secret_type, &secret)?;
        let existing = sqlx::query("SELECT id,keyring_account FROM secret_references WHERE name=?")
            .bind(name)
            .fetch_optional(self.pool())
            .await?;
        let (id, account) = match existing {
            Some(row) => (
                row_uuid(&row, "id")?,
                row.try_get::<String, _>("keyring_account")?,
            ),
            None => {
                let id = Uuid::new_v4();
                (id, id.to_string())
            }
        };
        crate::services::secrets::set(account.clone(), secret).await?;
        let now = Utc::now();
        sqlx::query("INSERT INTO secret_references(id,name,secret_type,keyring_account,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(name) DO UPDATE SET secret_type=excluded.secret_type,updated_at=excluded.updated_at")
            .bind(id.to_string()).bind(name).bind(secret_type).bind(account).bind(now).bind(now)
            .execute(self.pool()).await?;
        self.get_secret_reference(id).await
    }

    pub async fn list_secret_references(&self) -> Result<Vec<SecretReference>> {
        sqlx::query(
            "SELECT id,name,secret_type,created_at,updated_at FROM secret_references ORDER BY name",
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(row_to_secret_reference)
        .collect()
    }

    pub async fn get_secret_reference(&self, id: Uuid) -> Result<SecretReference> {
        sqlx::query(
            "SELECT id,name,secret_type,created_at,updated_at FROM secret_references WHERE id=?",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?
        .map(row_to_secret_reference)
        .transpose()?
        .ok_or_else(|| RavynError::NotFound(format!("secret reference {id}")))
    }

    pub async fn resolve_secret_reference(&self, id: Uuid, expected_type: &str) -> Result<String> {
        let row =
            sqlx::query("SELECT secret_type,keyring_account FROM secret_references WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(self.pool())
                .await?
                .ok_or_else(|| RavynError::NotFound(format!("secret reference {id}")))?;
        let secret_type: String = row.try_get("secret_type")?;
        if secret_type != expected_type {
            return Err(RavynError::Invalid(format!(
                "secret reference {id} has type {secret_type}, expected {expected_type}"
            )));
        }
        let account: String = row.try_get("keyring_account")?;
        crate::services::secrets::get(account).await
    }

    pub async fn delete_secret_reference(&self, id: Uuid) -> Result<()> {
        let row = sqlx::query("SELECT keyring_account FROM secret_references WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("secret reference {id}")))?;
        let needle = format!("%{id}%");
        let mut usages = Vec::new();
        for (label, table, column) in [
            ("runtime settings", "runtime_settings", "settings_json"),
            ("download jobs", "jobs", "options_json"),
            ("schedules", "schedules", "options_json"),
            ("download presets", "download_presets", "payload_json"),
            ("user profiles", "user_profiles", "settings_patch_json"),
            ("basket items", "basket_items", "request_json"),
        ] {
            let query = format!("SELECT COUNT(*) FROM {table} WHERE {column} LIKE ?");
            let count: i64 = sqlx::query_scalar(&query)
                .bind(&needle)
                .fetch_one(self.pool())
                .await?;
            if count > 0 {
                usages.push(format!("{label} ({count})"));
            }
        }
        if !usages.is_empty() {
            return Err(RavynError::Conflict(format!(
                "secret reference {id} is still used by {}; remove those references first",
                usages.join(", ")
            )));
        }
        let account: String = row.try_get("keyring_account")?;
        crate::services::secrets::delete(account).await?;
        sqlx::query("DELETE FROM secret_references WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

pub(crate) fn row_to_secret_reference(row: SqliteRow) -> Result<SecretReference> {
    Ok(SecretReference {
        id: row_uuid(&row, "id")?,
        name: row.try_get("name")?,
        secret_type: row.try_get("secret_type")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::validate_secret_value;

    #[test]
    fn validates_structured_secret_payloads() {
        assert!(validate_secret_value(
            "rqbit_credentials",
            r#"{"username":"ravyn","password":"secret"}"#,
        )
        .is_ok());
        assert!(validate_secret_value("cookies", r#"{"session":"value"}"#).is_ok());
        assert!(
            validate_secret_value("proxy_credentials", "http://user:pass@127.0.0.1:8080")
                .is_ok()
        );
        assert!(validate_secret_value("authentication_header", "Bearer token").is_ok());
    }

    #[test]
    fn rejects_invalid_structured_secret_payloads() {
        assert!(validate_secret_value("rqbit_credentials", "{}").is_err());
        assert!(validate_secret_value("cookies", "[]").is_err());
        assert!(validate_secret_value("proxy_credentials", "not a URL").is_err());
        assert!(
            validate_secret_value(
                "authentication_header",
                "Bearer value\r\nInjected: true",
            )
            .is_err()
        );
    }
}
