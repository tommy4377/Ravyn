//! Verified, versioned installation primitives for managed external engines.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header::LOCATION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    error::{RavynError, Result},
    services::security,
};

const MAX_ENGINE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENGINE_METADATA_BYTES: u64 = 64 * 1024;
const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineManifest {
    pub schema_version: u32,
    pub channel: String,
    pub artifacts: Vec<EngineArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEngineManifest {
    pub manifest: EngineManifest,
    /// Hex-encoded Ed25519 signature over the compact JSON encoding of
    /// `manifest`. The manifest contains no maps, so this encoding is stable.
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineArtifact {
    pub engine: String,
    pub version: String,
    pub target: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub filename: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveEngine {
    version: String,
    filename: String,
    sha256: String,
}

impl ActiveEngine {
    fn validate(&self) -> Result<()> {
        validate_token(&self.version, "version")?;
        validate_filename(&self.filename)?;
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(RavynError::Invalid(
                "managed engine activation checksum is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EngineManager {
    root: PathBuf,
}

impl EngineManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA {
            return Err(RavynError::Invalid(format!(
                "unsupported engine manifest schema {}",
                self.schema_version
            )));
        }
        validate_token(&self.channel, "channel")?;
        if self.artifacts.len() > 256 {
            return Err(RavynError::Invalid(
                "engine manifest contains too many artifacts".into(),
            ));
        }
        for artifact in &self.artifacts {
            artifact.validate()?;
        }
        Ok(())
    }

    pub fn artifact(&self, engine: &str, target: &str) -> Result<&EngineArtifact> {
        self.validate()?;
        self.artifacts
            .iter()
            .find(|artifact| artifact.engine == engine && artifact.target == target)
            .ok_or_else(|| {
                RavynError::Unavailable(format!(
                    "no managed {engine} artifact is available for {target}"
                ))
            })
    }
}

impl SignedEngineManifest {
    pub fn verify(&self, public_key: &[u8; 32]) -> Result<&EngineManifest> {
        use ed25519_dalek::{Signature, VerifyingKey};

        let signature_bytes = hex::decode(&self.signature).map_err(|_| {
            RavynError::Invalid("engine manifest signature must be hexadecimal".into())
        })?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| {
            RavynError::Invalid("engine manifest signature must contain 64 bytes".into())
        })?;
        let key = VerifyingKey::from_bytes(public_key)
            .map_err(|_| RavynError::Invalid("engine manifest public key is invalid".into()))?;
        let payload = serde_json::to_vec(&self.manifest)?;
        key.verify_strict(&payload, &signature).map_err(|_| {
            RavynError::Protocol("engine manifest signature verification failed".into())
        })?;
        self.manifest.validate()?;
        Ok(&self.manifest)
    }
}

impl EngineArtifact {
    fn validate(&self) -> Result<()> {
        validate_token(&self.engine, "engine")?;
        validate_token(&self.version, "version")?;
        validate_token(&self.target, "target")?;
        validate_filename(&self.filename)?;
        if self.size_bytes == 0 || self.size_bytes > MAX_ENGINE_BYTES {
            return Err(RavynError::Invalid(format!(
                "managed engine size must be between 1 and {MAX_ENGINE_BYTES} bytes"
            )));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(RavynError::Invalid(
                "managed engine SHA-256 must contain exactly 64 hexadecimal characters".into(),
            ));
        }
        let url = url::Url::parse(&self.url)?;
        if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
            return Err(RavynError::Invalid(
                "managed engine artifacts require an HTTPS URL without credentials".into(),
            ));
        }
        if url.password().is_some() || url.fragment().is_some() {
            return Err(RavynError::Invalid(
                "managed engine artifact URLs may not contain passwords or fragments".into(),
            ));
        }
        if self.capabilities.len() > 128
            || self
                .capabilities
                .iter()
                .any(|value| validate_token(value, "capability").is_err())
        {
            return Err(RavynError::Invalid(
                "managed engine capabilities are invalid or excessive".into(),
            ));
        }
        Ok(())
    }
}

impl EngineManager {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("engines"),
        }
    }

    /// Root directory for all managed engine binaries.
    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    pub async fn install_verified(
        &self,
        artifact: &EngineArtifact,
        bytes: &[u8],
    ) -> Result<PathBuf> {
        artifact.validate()?;
        if bytes.len() as u64 != artifact.size_bytes {
            return Err(RavynError::Protocol(format!(
                "managed engine size mismatch: expected {}, received {}",
                artifact.size_bytes,
                bytes.len()
            )));
        }
        let actual = hex::encode(Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(RavynError::Protocol(
                "managed engine checksum verification failed".into(),
            ));
        }

        let version_dir = self.root.join(&artifact.engine).join(&artifact.version);
        tokio::fs::create_dir_all(&version_dir).await?;
        let destination = version_dir.join(&artifact.filename);
        let temporary = version_dir.join(format!(".{}.download", artifact.filename));
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        set_executable(&temporary).await?;
        atomic_replace(&temporary, &destination).await?;

        let active = ActiveEngine {
            version: artifact.version.clone(),
            filename: artifact.filename.clone(),
            sha256: artifact.sha256.to_ascii_lowercase(),
        };
        self.write_active(&artifact.engine, &active).await?;
        Ok(destination)
    }

    pub async fn download_and_install(
        &self,
        config: &Config,
        artifact: &EngineArtifact,
        cancellation: &CancellationToken,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    ) -> Result<PathBuf> {
        artifact.validate()?;
        let version_dir = self.root.join(&artifact.engine).join(&artifact.version);
        tokio::fs::create_dir_all(&version_dir).await?;
        let temporary = version_dir.join(format!(".{}.download", artifact.filename));
        let destination = version_dir.join(&artifact.filename);
        let mut current = url::Url::parse(&artifact.url)?;
        let mut visited = std::collections::BTreeSet::new();
        let mut redirects = 0_u8;
        let response = loop {
            if !visited.insert(current.as_str().to_owned()) {
                return Err(RavynError::Protocol("engine download redirect loop".into()));
            }
            if current.scheme() != "https" {
                return Err(RavynError::Invalid(
                    "managed engine redirects must remain on HTTPS".into(),
                ));
            }
            let host = current
                .host_str()
                .ok_or_else(|| RavynError::Invalid("engine URL has no host".into()))?;
            let addresses = security::resolve_network_source(config, current.as_str()).await?;
            let mut builder = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(config.connect_timeout())
                .read_timeout(config.read_timeout());
            for address in addresses {
                builder = builder.resolve(host, address);
            }
            let client = builder.build()?;
            let response = tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                response = client.get(current.clone()).send() => response?,
            };
            if !response.status().is_redirection() {
                break response;
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    RavynError::Protocol("engine redirect has no valid location".into())
                })?;
            current = current.join(location)?;
            redirects = redirects.saturating_add(1);
            if redirects > 5 {
                return Err(RavynError::Protocol(
                    "engine download exceeded the redirect limit".into(),
                ));
            }
        };
        if response.status() != StatusCode::OK {
            return Err(RavynError::Protocol(format!(
                "engine download returned {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length != artifact.size_bytes)
        {
            return Err(RavynError::Protocol(
                "engine download Content-Length does not match its manifest".into(),
            ));
        }

        let download_result: Result<()> = async {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)
                .await?;
            let mut stream = response.bytes_stream();
            let mut hasher = Sha256::new();
            let mut received = 0_u64;
            while let Some(chunk) = tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                chunk = stream.next() => chunk
            } {
                let chunk = chunk?;
                received = received.saturating_add(chunk.len() as u64);
                if received > artifact.size_bytes || received > MAX_ENGINE_BYTES {
                    return Err(RavynError::Protocol(
                        "engine download exceeded its declared size".into(),
                    ));
                }
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                if let Some(report) = progress {
                    report(received, artifact.size_bytes);
                }
            }
            file.sync_all().await?;
            drop(file);
            let actual = hex::encode(hasher.finalize());
            if received != artifact.size_bytes || !actual.eq_ignore_ascii_case(&artifact.sha256) {
                return Err(RavynError::Protocol(
                    "engine download failed size or checksum verification".into(),
                ));
            }
            set_executable(&temporary).await?;
            atomic_replace(&temporary, &destination).await?;
            Ok(())
        }
        .await;
        if let Err(error) = download_result {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error);
        }
        self.write_active(
            &artifact.engine,
            &ActiveEngine {
                version: artifact.version.clone(),
                filename: artifact.filename.clone(),
                sha256: artifact.sha256.to_ascii_lowercase(),
            },
        )
        .await?;
        Ok(destination)
    }

    pub async fn active_path(&self, engine: &str) -> Result<Option<PathBuf>> {
        validate_token(engine, "engine")?;
        let path = self.root.join(engine).join("active.json");
        if !tokio::fs::try_exists(&path).await? {
            return Ok(None);
        }
        let bytes = read_engine_metadata(&path).await?;
        let active: ActiveEngine = serde_json::from_slice(&bytes)?;
        active.validate()?;
        let executable = self
            .root
            .join(engine)
            .join(active.version)
            .join(active.filename);
        if !tokio::fs::try_exists(&executable).await? {
            return Err(RavynError::Unavailable(format!(
                "active managed {engine} executable is missing"
            )));
        }
        let actual = hash_file(&executable).await?;
        if !actual.eq_ignore_ascii_case(&active.sha256) {
            return Err(RavynError::Unavailable(format!(
                "active managed {engine} executable failed checksum verification"
            )));
        }
        Ok(Some(executable))
    }

    /// Atomically returns to the previously active, checksum-verified version.
    /// The version being replaced becomes the next rollback target.
    pub async fn rollback(&self, engine: &str) -> Result<PathBuf> {
        validate_token(engine, "engine")?;
        let previous_path = self.root.join(engine).join("previous.json");
        if !tokio::fs::try_exists(&previous_path).await? {
            return Err(RavynError::NotFound(format!(
                "managed {engine} has no previous version"
            )));
        }
        let previous: ActiveEngine =
            serde_json::from_slice(&read_engine_metadata(&previous_path).await?)?;
        previous.validate()?;
        let executable = self
            .root
            .join(engine)
            .join(&previous.version)
            .join(&previous.filename);
        let actual = hash_file(&executable).await?;
        if !actual.eq_ignore_ascii_case(&previous.sha256) {
            return Err(RavynError::Unavailable(format!(
                "previous managed {engine} executable failed checksum verification"
            )));
        }
        self.write_active(engine, &previous).await?;
        Ok(executable)
    }

    async fn write_active(&self, engine: &str, active: &ActiveEngine) -> Result<()> {
        let engine_dir = self.root.join(engine);
        tokio::fs::create_dir_all(&engine_dir).await?;
        let destination = engine_dir.join("active.json");
        if tokio::fs::try_exists(&destination).await? {
            let current = read_engine_metadata(&destination)
                .await
                .and_then(|bytes| {
                    serde_json::from_slice::<ActiveEngine>(&bytes).map_err(Into::into)
                })
                .and_then(|active| active.validate().map(|()| active));
            match current {
                Ok(current) => {
                    let previous = engine_dir.join("previous.json");
                    write_metadata_atomic(
                        &engine_dir.join(".previous.json.tmp"),
                        &previous,
                        &current,
                    )
                    .await?;
                }
                Err(error) => tracing::warn!(
                    %error,
                    engine,
                    "discarding invalid managed-engine activation metadata during recovery"
                ),
            }
        }
        let temporary = engine_dir.join(".active.json.tmp");
        write_metadata_atomic(&temporary, &destination, active).await
    }
}

async fn read_engine_metadata(path: &Path) -> Result<Vec<u8>> {
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() > MAX_ENGINE_METADATA_BYTES {
        return Err(RavynError::Invalid(format!(
            "managed engine metadata must be a regular file no larger than {MAX_ENGINE_METADATA_BYTES} bytes"
        )));
    }
    Ok(tokio::fs::read(path).await?)
}

async fn write_metadata_atomic(
    temporary: &Path,
    destination: &Path,
    active: &ActiveEngine,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(active)?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temporary)
        .await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;
    drop(file);
    atomic_replace(temporary, destination).await
}

async fn hash_file(path: &Path) -> Result<String> {
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ENGINE_BYTES {
        return Err(RavynError::Unavailable(
            "managed engine executable has an invalid size".into(),
        ));
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(not(windows))]
async fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    tokio::fs::rename(source, destination).await?;
    Ok(())
}

#[cfg(windows)]
async fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_WRITE_THROUGH,
        ReplaceFileW,
    };

    let source = source.to_owned();
    let destination = destination.to_owned();
    tokio::task::spawn_blocking(move || {
        let source_wide = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let destination_wide = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let replaced = unsafe {
            if destination.exists() {
                ReplaceFileW(
                    destination_wide.as_ptr(),
                    source_wide.as_ptr(),
                    ptr::null(),
                    REPLACEFILE_WRITE_THROUGH,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            } else {
                MoveFileExW(
                    source_wide.as_ptr(),
                    destination_wide.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
        };
        if replaced == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
    .await
    .map_err(|error| RavynError::Internal(format!("engine activation task failed: {error}")))??;
    Ok(())
}

fn validate_token(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(RavynError::Invalid(format!(
            "managed engine {label} is invalid"
        )));
    }
    Ok(())
}

fn validate_filename(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || Path::new(value).file_name().and_then(|name| name.to_str()) != Some(value)
        || value.contains(['/', '\\'])
    {
        return Err(RavynError::Invalid(
            "managed engine filename must be a single safe path component".into(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
async fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = tokio::fs::metadata(path).await?.permissions();
    permissions.set_mode(0o755);
    tokio::fs::set_permissions(path, permissions).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn artifact(bytes: &[u8]) -> EngineArtifact {
        EngineArtifact {
            engine: "ffmpeg".into(),
            version: "7.1.0".into(),
            target: "x86_64-pc-windows-msvc".into(),
            url: "https://downloads.example.test/ffmpeg.exe".into(),
            sha256: hex::encode(Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
            filename: "ffmpeg.exe".into(),
            capabilities: vec!["transcode".into()],
        }
    }

    #[tokio::test]
    async fn installs_and_resolves_a_verified_version() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let bytes = b"test executable";
        let installed = manager
            .install_verified(&artifact(bytes), bytes)
            .await
            .unwrap();
        assert_eq!(
            manager.active_path("ffmpeg").await.unwrap(),
            Some(installed)
        );
    }

    #[tokio::test]
    async fn rejects_corrupt_or_unsafe_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let bytes = b"test executable";
        assert!(
            manager
                .install_verified(&artifact(bytes), b"corrupt")
                .await
                .is_err()
        );
        let mut unsafe_artifact = artifact(bytes);
        unsafe_artifact.filename = "../ffmpeg.exe".into();
        assert!(
            manager
                .install_verified(&unsafe_artifact, bytes)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn activation_can_roll_back_without_trusting_stale_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let first_bytes = b"first executable";
        let first = manager
            .install_verified(&artifact(first_bytes), first_bytes)
            .await
            .unwrap();
        let mut second_artifact = artifact(b"second executable");
        second_artifact.version = "7.2.0".into();
        let second = manager
            .install_verified(&second_artifact, b"second executable")
            .await
            .unwrap();
        assert_eq!(manager.active_path("ffmpeg").await.unwrap(), Some(second));

        assert_eq!(manager.rollback("ffmpeg").await.unwrap(), first);
        assert_eq!(manager.active_path("ffmpeg").await.unwrap(), Some(first));
    }

    #[tokio::test]
    async fn verified_install_repairs_corrupt_activation_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let engine_dir = temp.path().join("engines").join("ffmpeg");
        tokio::fs::create_dir_all(&engine_dir).await.unwrap();
        tokio::fs::write(engine_dir.join("active.json"), b"not json")
            .await
            .unwrap();
        let bytes = b"replacement executable";

        let installed = manager
            .install_verified(&artifact(bytes), bytes)
            .await
            .unwrap();

        assert_eq!(
            manager.active_path("ffmpeg").await.unwrap(),
            Some(installed)
        );
    }

    #[tokio::test]
    async fn oversized_activation_metadata_is_rejected_without_allocation() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let engine_dir = temp.path().join("engines").join("ffmpeg");
        tokio::fs::create_dir_all(&engine_dir).await.unwrap();
        let file = tokio::fs::File::create(engine_dir.join("active.json"))
            .await
            .unwrap();
        file.set_len(MAX_ENGINE_METADATA_BYTES + 1).await.unwrap();

        assert!(manager.active_path("ffmpeg").await.is_err());
    }

    #[test]
    fn signed_manifest_rejects_tampering() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let manifest = EngineManifest {
            schema_version: MANIFEST_SCHEMA,
            channel: "stable".into(),
            artifacts: vec![artifact(b"signed engine")],
        };
        let signature = signing_key.sign(&serde_json::to_vec(&manifest).unwrap());
        let mut signed = SignedEngineManifest {
            manifest,
            signature: hex::encode(signature.to_bytes()),
        };
        assert!(
            signed
                .verify(signing_key.verifying_key().as_bytes())
                .is_ok()
        );
        signed.manifest.channel = "beta".into();
        assert!(
            signed
                .verify(signing_key.verifying_key().as_bytes())
                .is_err()
        );
    }
}
