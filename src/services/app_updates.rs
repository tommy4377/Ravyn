//! Signed application-update metadata shared by the release tool and the
//! desktop shell. The private signing key is never linked into the app.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{RavynError, Result};

pub const APP_UPDATE_SCHEMA_VERSION: u32 = 1;
pub const MAX_APP_UPDATE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppUpdateManifest {
    pub schema: u32,
    pub channel: String,
    pub version: String,
    pub published_at: String,
    pub notes: Option<String>,
    pub artifact: AppUpdateArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppUpdateArtifact {
    pub target: String,
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedAppUpdateManifest {
    pub manifest: AppUpdateManifest,
    pub signature: String,
}

impl AppUpdateManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema != APP_UPDATE_SCHEMA_VERSION {
            return Err(RavynError::Invalid(format!(
                "unsupported app update schema {}; expected {APP_UPDATE_SCHEMA_VERSION}",
                self.schema
            )));
        }
        if self.channel != "stable" {
            return Err(RavynError::Invalid(
                "app update channel must be stable".into(),
            ));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(RavynError::Invalid(
                "app update version is missing or too long".into(),
            ));
        }
        if self.published_at.trim().is_empty() || self.published_at.len() > 64 {
            return Err(RavynError::Invalid(
                "app update publication timestamp is missing or too long".into(),
            ));
        }
        let published_at = DateTime::parse_from_rfc3339(&self.published_at)
            .map_err(|_| RavynError::Invalid("app update publication timestamp is invalid".into()))?
            .with_timezone(&Utc);
        if published_at > Utc::now() + ChronoDuration::minutes(10) {
            return Err(RavynError::Invalid(
                "app update publication timestamp is too far in the future".into(),
            ));
        }
        if self.artifact.target != "windows-x86_64" {
            return Err(RavynError::Invalid(
                "app update artifact target must be windows-x86_64".into(),
            ));
        }
        validate_filename(&self.artifact.filename)?;
        let url = url::Url::parse(&self.artifact.url)
            .map_err(|_| RavynError::Invalid("app update URL is invalid".into()))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(RavynError::Invalid(
                "app update URL must use HTTPS without credentials or fragments".into(),
            ));
        }
        if self.artifact.size_bytes == 0 || self.artifact.size_bytes > MAX_APP_UPDATE_BYTES {
            return Err(RavynError::Invalid(format!(
                "app update size must be between 1 and {MAX_APP_UPDATE_BYTES} bytes"
            )));
        }
        if self.artifact.sha256.len() != 64
            || !self
                .artifact
                .sha256
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(RavynError::Invalid(
                "app update SHA-256 must contain exactly 64 hexadecimal characters".into(),
            ));
        }
        if self
            .notes
            .as_ref()
            .is_some_and(|notes| notes.len() > 16 * 1024)
        {
            return Err(RavynError::Invalid(
                "app update notes exceed the maximum length".into(),
            ));
        }
        Ok(())
    }

    pub fn payload(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }
}

impl SignedAppUpdateManifest {
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<&AppUpdateManifest> {
        let signature_bytes = hex::decode(&self.signature)
            .map_err(|_| RavynError::Invalid("app update signature must be hexadecimal".into()))?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| {
            RavynError::Invalid("app update signature must contain 64 bytes".into())
        })?;
        let key = VerifyingKey::from_bytes(public_key)
            .map_err(|_| RavynError::Invalid("app update public key is invalid".into()))?;
        let payload = self.manifest.payload()?;
        key.verify_strict(&payload, &signature)
            .map_err(|_| RavynError::Invalid("app update signature verification failed".into()))?;
        Ok(&self.manifest)
    }

    pub fn verify_artifact(&self, bytes: &[u8]) -> Result<()> {
        self.manifest.validate()?;
        if bytes.len() as u64 != self.manifest.artifact.size_bytes {
            return Err(RavynError::Invalid(format!(
                "app update size mismatch: expected {}, received {}",
                self.manifest.artifact.size_bytes,
                bytes.len()
            )));
        }
        let digest = hex::encode(Sha256::digest(bytes));
        if !digest.eq_ignore_ascii_case(&self.manifest.artifact.sha256) {
            return Err(RavynError::Invalid(
                "app update SHA-256 verification failed".into(),
            ));
        }
        Ok(())
    }
}

fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty()
        || filename.len() > 180
        || filename.contains('/')
        || filename.contains('\\')
        || filename == "."
        || filename == ".."
        || !filename.to_ascii_lowercase().ends_with(".exe")
    {
        return Err(RavynError::Invalid(
            "app update filename must be a plain .exe filename".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    fn manifest(bytes: &[u8]) -> AppUpdateManifest {
        AppUpdateManifest {
            schema: APP_UPDATE_SCHEMA_VERSION,
            channel: "stable".into(),
            version: "0.3.0".into(),
            published_at: "2026-07-14T00:00:00Z".into(),
            notes: Some("Test release".into()),
            artifact: AppUpdateArtifact {
                target: "windows-x86_64".into(),
                filename: "Ravyn_0.3.0_x64-setup.exe".into(),
                url: "https://example.invalid/Ravyn_0.3.0_x64-setup.exe".into(),
                sha256: hex::encode(Sha256::digest(bytes)),
                size_bytes: bytes.len() as u64,
            },
        }
    }

    #[test]
    fn verifies_manifest_signature_and_artifact() {
        let bytes = b"installer";
        let manifest = manifest(bytes);
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = key.sign(&manifest.payload().unwrap());
        let signed = SignedAppUpdateManifest {
            manifest,
            signature: hex::encode(signature.to_bytes()),
        };

        signed.verify(&key.verifying_key().to_bytes()).unwrap();
        signed.verify_artifact(bytes).unwrap();
    }

    #[test]
    fn rejects_insecure_urls_and_wrong_hashes() {
        let bytes = b"installer";
        let mut invalid = manifest(bytes);
        invalid.artifact.url = "http://example.invalid/setup.exe".into();
        assert!(invalid.validate().is_err());

        let mut credentialed = manifest(bytes);
        credentialed.artifact.url = "https://user@example.invalid/setup.exe".into();
        assert!(credentialed.validate().is_err());

        let mut invalid_timestamp = manifest(bytes);
        invalid_timestamp.published_at = "not-a-timestamp".into();
        assert!(invalid_timestamp.validate().is_err());

        let mut valid = manifest(bytes);
        valid.artifact.sha256 = "00".repeat(32);
        let signed = SignedAppUpdateManifest {
            manifest: valid,
            signature: "00".repeat(64),
        };
        assert!(signed.verify_artifact(bytes).is_err());
    }
}
