use crate::error::{RavynError, Result};

const SERVICE: &str = "ravyn-backend";

pub async fn set(account: String, secret: String) -> Result<()> {
    if secret.is_empty() || secret.len() > 1024 * 1024 {
        return Err(RavynError::Invalid(
            "secret must contain between 1 byte and 1 MiB".into(),
        ));
    }
    tokio::task::spawn_blocking(move || {
        keyring::Entry::new(SERVICE, &account)
            .and_then(|entry| entry.set_password(&secret))
            .map_err(|error| RavynError::Unavailable(format!("platform secret store: {error}")))
    })
    .await
    .map_err(|error| RavynError::Internal(format!("secret-store task failed: {error}")))?
}

pub async fn get(account: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(SERVICE, &account)
            .map_err(|error| RavynError::Unavailable(format!("platform secret store: {error}")))?;
        match entry.get_password() {
            Ok(secret) => Ok(secret),
            Err(keyring::Error::NoEntry) => Err(RavynError::NotFound(format!(
                "platform secret-store entry {account}"
            ))),
            Err(error) => Err(RavynError::Unavailable(format!(
                "platform secret store: {error}"
            ))),
        }
    })
    .await
    .map_err(|error| RavynError::Internal(format!("secret-store task failed: {error}")))?
}

pub async fn delete(account: String) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(SERVICE, &account)
            .map_err(|error| RavynError::Unavailable(format!("platform secret store: {error}")))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(RavynError::Unavailable(format!(
                "platform secret store: {error}"
            ))),
        }
    })
    .await
    .map_err(|error| RavynError::Internal(format!("secret-store task failed: {error}")))?
}
