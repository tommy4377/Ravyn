use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::error::{RavynError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBrowserToken {
    pub name: String,
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTokenRecord {
    pub id: Uuid,
    pub name: String,
    pub allowed_origins: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedBrowserToken {
    #[serde(flatten)]
    pub record: BrowserTokenRecord,
    /// Returned only once. Ravyn persists a SHA-256 digest, never the token.
    pub token: String,
}

pub fn issue(request: CreateBrowserToken) -> Result<(IssuedBrowserToken, String)> {
    let name = request.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(RavynError::Invalid(
            "browser token name must contain 1 to 120 characters".into(),
        ));
    }
    let mut origins = request
        .allowed_origins
        .iter()
        .map(|origin| normalize_origin(origin))
        .collect::<Result<Vec<_>>>()?;
    origins.sort();
    origins.dedup();
    if origins.is_empty() {
        return Err(RavynError::Invalid(
            "at least one browser origin is required".into(),
        ));
    }
    if origins.len() > 32 {
        return Err(RavynError::Invalid(
            "a browser token may allow at most 32 origins".into(),
        ));
    }

    let id = Uuid::new_v4();
    let token = format!(
        "rvn_b_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let token_hash = hash_token(&token);
    let record = BrowserTokenRecord {
        id,
        name: name.to_owned(),
        allowed_origins: origins,
        created_at: Utc::now(),
        last_used_at: None,
        revoked_at: None,
    };
    Ok((IssuedBrowserToken { record, token }, token_hash))
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn normalize_origin(value: &str) -> Result<String> {
    let value = value.trim();
    let url = Url::parse(value)
        .map_err(|_| RavynError::Invalid(format!("invalid browser origin `{value}`")))?;
    if !matches!(
        url.scheme(),
        "http" | "https" | "chrome-extension" | "moz-extension" | "safari-web-extension"
    ) {
        return Err(RavynError::Invalid(format!(
            "unsupported browser origin scheme `{}`",
            url.scheme()
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(RavynError::Invalid(
            "browser origins may not contain credentials".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() || !matches!(url.path(), "" | "/") {
        return Err(RavynError::Invalid(
            "browser origins may not contain a path, query, or fragment".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| RavynError::Invalid("browser origin must contain a host".into()))?;
    let mut normalized = format!("{}://{}", url.scheme(), host.to_ascii_lowercase());
    if let Some(port) = url.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }
    Ok(normalized)
}

pub fn origin_allowed(allowed: &[String], supplied: &str) -> bool {
    normalize_origin(supplied)
        .ok()
        .is_some_and(|origin| allowed.iter().any(|item| item == &origin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_browser_origins() {
        assert_eq!(
            normalize_origin("https://Example.com:8443/").unwrap(),
            "https://example.com:8443"
        );
        assert_eq!(
            normalize_origin("chrome-extension://ABCDEF").unwrap(),
            "chrome-extension://abcdef"
        );
    }

    #[test]
    fn rejects_origins_with_paths() {
        assert!(normalize_origin("https://example.com/page").is_err());
    }

    #[test]
    fn token_is_only_exposed_in_issued_record() {
        let (issued, hash) = issue(CreateBrowserToken {
            name: "browser".into(),
            allowed_origins: vec!["moz-extension://abc".into()],
        })
        .unwrap();
        assert!(issued.token.starts_with("rvn_b_"));
        assert_eq!(hash, hash_token(&issued.token));
    }

    #[test]
    fn rejects_credentials_and_excessive_origin_lists() {
        assert!(normalize_origin("https://user:secret@example.com").is_err());
        let request = CreateBrowserToken {
            name: "too-many".into(),
            allowed_origins: (0..33)
                .map(|index| format!("moz-extension://extension-{index}"))
                .collect(),
        };
        assert!(issue(request).is_err());
    }
}
