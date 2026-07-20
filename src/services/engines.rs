//! Verified, versioned installation primitives for managed external engines.

use std::{
    io::Read as _,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header::LOCATION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

mod artifacts;

use artifacts::*;

use crate::{
    config::Config,
    error::{ProvisioningErrorCode, RavynError, Result},
    services::security,
};

const MAX_ENGINE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENGINE_METADATA_BYTES: u64 = 64 * 1024;
const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineManifest {
    pub schema_version: u32,
    pub channel: String,
    /// Monotonic release sequence used to prevent remote manifest downgrade
    /// and replay attacks. Embedded development manifests may omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_version: Option<u64>,
    /// UTC publication time for remote release manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<DateTime<Utc>>,
    /// UTC expiry time after which a remote manifest may only be used through
    /// the explicitly bounded last-known-good grace period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
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
    /// SHA-256 of the downloaded artifact exactly as served by `url` (the
    /// archive or installer itself when an installation strategy is set).
    pub sha256: String,
    /// Exact artifact size. A value of zero is allowed only when
    /// `max_size_bytes` supplies a signed upper bound for a publisher that
    /// does not expose an exact byte count.
    pub size_bytes: u64,
    /// Signed upper bound used only when `size_bytes` is zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size_bytes: Option<u64>,
    /// Safe relative path of the executable inside the managed version
    /// directory. Direct and ZIP-member artifacts normally use a basename.
    pub filename: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// When set, the downloaded artifact is a ZIP archive and this is the
    /// forward-slash-separated path of the executable to extract as
    /// `filename`.
    #[serde(default)]
    pub archive_member: Option<String>,
    /// SHA-256 of the extracted [`Self::archive_member`] content. Required
    /// (and only meaningful) when `archive_member` is set; this, not
    /// `sha256`, becomes the activation checksum stored for the installed
    /// executable.
    #[serde(default)]
    pub member_sha256: Option<String>,
    /// Optional package installation strategy. Package artifacts are verified
    /// before the fixed, non-shelling strategy is executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer: Option<EngineInstaller>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInstaller {
    pub kind: EngineInstallerKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineInstallerKind {
    /// Extract a Windows Installer package with `msiexec /a` into the managed
    /// version directory without registering a machine-wide installation.
    MsiAdministrative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveEngine {
    version: String,
    /// Physical installation directory. Older metadata omitted this field and
    /// used `version` as the directory name, which remains the compatibility
    /// fallback during upgrades.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<String>,
    filename: String,
    sha256: String,
}

/// Public, checksum-verified metadata for an active managed engine.
#[derive(Debug, Clone, Serialize)]
pub struct ActiveEngineInfo {
    pub version: String,
    pub filename: String,
    pub sha256: String,
    pub path: PathBuf,
}

/// Lifecycle stage reported while activating a managed engine artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineInstallStage {
    Downloading,
    Verifying,
    Installing,
}

impl ActiveEngine {
    fn validate(&self) -> Result<()> {
        validate_token(&self.version, "version")?;
        if let Some(directory) = &self.directory {
            validate_token(directory, "installation directory")?;
        }
        validate_relative_path(&self.filename, "activation path")?;
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|value| value.is_ascii_hexdigit()) {
            return Err(RavynError::Invalid(
                "managed engine activation checksum is invalid".into(),
            ));
        }
        Ok(())
    }

    fn directory_name(&self) -> &str {
        self.directory.as_deref().unwrap_or(&self.version)
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
        match (
            self.manifest_version,
            self.generated_at.as_ref(),
            self.expires_at.as_ref(),
        ) {
            (None, None, None) => {}
            (Some(version), Some(generated_at), Some(expires_at)) => {
                if version == 0 {
                    return Err(RavynError::Invalid(
                        "engine manifest version must be greater than zero".into(),
                    ));
                }
                if expires_at <= generated_at {
                    return Err(RavynError::Invalid(
                        "engine manifest expiry must be after its generation time".into(),
                    ));
                }
                if expires_at.signed_duration_since(*generated_at) > ChronoDuration::days(90) {
                    return Err(RavynError::Invalid(
                        "engine manifest validity window may not exceed 90 days".into(),
                    ));
                }
            }
            _ => {
                return Err(RavynError::Invalid(
                    "remote engine manifest metadata must include version, generated_at, and expires_at together".into(),
                ));
            }
        }
        if self.artifacts.len() > 256 {
            return Err(RavynError::Invalid(
                "engine manifest contains too many artifacts".into(),
            ));
        }
        let mut identities = std::collections::BTreeSet::new();
        for artifact in &self.artifacts {
            artifact.validate()?;
            if !identities.insert((&artifact.engine, &artifact.target)) {
                return Err(RavynError::Invalid(format!(
                    "engine manifest contains duplicate {} artifact for {}",
                    artifact.engine, artifact.target
                )));
            }
        }
        Ok(())
    }

    /// Applies the additional freshness and channel rules required for a
    /// remotely distributed manifest. Embedded manifests intentionally remain
    /// valid without release metadata so development builds can start offline.
    pub fn validate_remote(&self, expected_channel: &str, now: DateTime<Utc>) -> Result<()> {
        self.validate()?;
        if self.channel != expected_channel {
            return Err(RavynError::Invalid(format!(
                "engine manifest channel {} does not match configured channel {expected_channel}",
                self.channel
            )));
        }
        let (_, generated_at, expires_at) = self.remote_metadata().ok_or_else(|| {
            RavynError::Invalid(
                "remote engine manifests require version and validity metadata".into(),
            )
        })?;
        if generated_at > now + ChronoDuration::minutes(10) {
            return Err(RavynError::Invalid(
                "engine manifest generation time is unreasonably far in the future".into(),
            ));
        }
        if expires_at <= now {
            return Err(RavynError::Invalid("engine manifest has expired".into()));
        }
        Ok(())
    }

    pub fn remote_metadata(&self) -> Option<(u64, DateTime<Utc>, DateTime<Utc>)> {
        Some((self.manifest_version?, self.generated_at?, self.expires_at?))
    }

    pub fn artifact(&self, engine: &str, target: &str) -> Result<&EngineArtifact> {
        self.validate()?;
        self.artifacts
            .iter()
            .find(|artifact| artifact.engine == engine && artifact.target == target)
            .ok_or_else(|| {
                RavynError::provisioning(
                    ProvisioningErrorCode::PlatformUnsupported,
                    format!("no managed {engine} artifact is available for {target}"),
                )
                .with_component(engine)
                .with_target(target)
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
            RavynError::provisioning(
                ProvisioningErrorCode::InvalidManifestSignature,
                "engine manifest signature verification failed",
            )
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
        validate_relative_path(&self.filename, "filename")?;
        if self.size_bytes > MAX_ENGINE_BYTES {
            return Err(RavynError::Invalid(format!(
                "managed engine size may not exceed {MAX_ENGINE_BYTES} bytes"
            )));
        }
        match (self.size_bytes, self.max_size_bytes) {
            (0, Some(limit)) if (1..=MAX_ENGINE_BYTES).contains(&limit) => {}
            (0, _) => {
                return Err(RavynError::Invalid(
                    "managed engine artifacts without an exact size require a bounded max_size_bytes".into(),
                ));
            }
            (_, None) => {}
            (_, Some(_)) => {
                return Err(RavynError::Invalid(
                    "managed engine max_size_bytes is only valid when size_bytes is zero".into(),
                ));
            }
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
        if let Some(member) = &self.archive_member {
            if member.is_empty()
                || member.len() > 512
                || member.starts_with('/')
                || member.contains('\\')
                || member
                    .split('/')
                    .any(|segment| segment.is_empty() || segment == "." || segment == "..")
            {
                return Err(RavynError::Invalid(
                    "managed engine archive member must be a safe relative forward-slash path"
                        .into(),
                ));
            }
            let member_sha256 = self.member_sha256.as_deref().ok_or_else(|| {
                RavynError::Invalid(
                    "managed engine artifacts with an archive member require member_sha256".into(),
                )
            })?;
            if member_sha256.len() != 64
                || !member_sha256.bytes().all(|value| value.is_ascii_hexdigit())
            {
                return Err(RavynError::Invalid(
                    "managed engine member SHA-256 must contain exactly 64 hexadecimal characters"
                        .into(),
                ));
            }
        } else if self.member_sha256.is_some() {
            return Err(RavynError::Invalid(
                "managed engine member_sha256 requires archive_member to be set".into(),
            ));
        }
        if self.installer.is_some() && self.archive_member.is_some() {
            return Err(RavynError::Invalid(
                "managed engine installer and archive_member strategies are mutually exclusive"
                    .into(),
            ));
        }
        if matches!(
            self.installer.as_ref().map(|installer| installer.kind),
            Some(EngineInstallerKind::MsiAdministrative)
        ) && (!url.path().to_ascii_lowercase().ends_with(".msi")
            || !self.target.to_ascii_lowercase().contains("windows"))
        {
            return Err(RavynError::Invalid(
                "MSI administrative extraction requires a Windows target and an .msi artifact URL"
                    .into(),
            ));
        }
        Ok(())
    }

    fn exact_size(&self) -> Option<u64> {
        (self.size_bytes != 0).then_some(self.size_bytes)
    }

    fn download_limit(&self) -> u64 {
        self.exact_size()
            .or(self.max_size_bytes)
            .unwrap_or(MAX_ENGINE_BYTES)
    }

    /// The checksum that verifies the installed executable for direct and ZIP
    /// artifacts. Installer strategies compute the activation checksum from
    /// the executable produced inside the private candidate directory.
    fn activation_sha256(&self) -> &str {
        self.member_sha256.as_deref().unwrap_or(&self.sha256)
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
        if let Some(expected) = artifact.exact_size() {
            if bytes.len() as u64 != expected {
                return Err(RavynError::provisioning(
                    ProvisioningErrorCode::DownloadInterrupted,
                    format!(
                        "managed engine size mismatch: expected {expected}, received {}",
                        bytes.len()
                    ),
                )
                .with_component(&artifact.engine)
                .with_expected_version(&artifact.version));
            }
        } else if bytes.len() as u64 > artifact.download_limit() {
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::DownloadInterrupted,
                "managed engine artifact exceeded its signed size limit",
            )
            .with_component(&artifact.engine)
            .with_expected_version(&artifact.version));
        }
        let actual = hex::encode(Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::ChecksumMismatch,
                "managed engine checksum verification failed",
            )
            .with_component(&artifact.engine)
            .with_expected_version(&artifact.version));
        }

        let (directory_name, version_dir, temporary, destination) =
            self.prepare_candidate(artifact).await?;
        let cancellation = CancellationToken::new();
        let install_result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .await?;
            file.write_all(bytes).await?;
            file.sync_all().await?;
            drop(file);
            materialize_verified_artifact(
                artifact,
                &temporary,
                &destination,
                &version_dir,
                &cancellation,
            )
            .await
        }
        .await;
        let activation_sha256 = match install_result {
            Ok(checksum) => checksum,
            Err(error) => {
                cleanup_candidate(&temporary, &version_dir).await;
                return Err(error);
            }
        };

        let active = ActiveEngine {
            version: artifact.version.clone(),
            directory: Some(directory_name),
            filename: artifact.filename.clone(),
            sha256: activation_sha256,
        };
        if let Err(error) = self.write_active(&artifact.engine, &active).await {
            cleanup_candidate(&temporary, &version_dir).await;
            return Err(error);
        }
        Ok(destination)
    }

    pub async fn download_and_install(
        &self,
        config: &Config,
        artifact: &EngineArtifact,
        cancellation: &CancellationToken,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
        stage: Option<&(dyn Fn(EngineInstallStage) + Send + Sync)>,
    ) -> Result<PathBuf> {
        artifact.validate()?;
        if let Some(report) = stage {
            report(EngineInstallStage::Downloading);
        }
        let (directory_name, version_dir, temporary, destination) =
            self.prepare_candidate(artifact).await?;
        let mut current = url::Url::parse(&artifact.url)?;
        let mut visited = std::collections::BTreeSet::new();
        let mut redirects = 0_u8;
        let response = loop {
            if !visited.insert(current.as_str().to_owned()) {
                cleanup_candidate(&temporary, &version_dir).await;
                return Err(RavynError::Protocol("engine download redirect loop".into()));
            }
            if current.scheme() != "https" {
                cleanup_candidate(&temporary, &version_dir).await;
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
                _ = cancellation.cancelled() => {
                    cleanup_candidate(&temporary, &version_dir).await;
                    return Err(RavynError::Cancelled);
                },
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
                cleanup_candidate(&temporary, &version_dir).await;
                return Err(RavynError::Protocol(
                    "engine download exceeded the redirect limit".into(),
                ));
            }
        };
        if response.status() != StatusCode::OK {
            cleanup_candidate(&temporary, &version_dir).await;
            return Err(RavynError::Protocol(format!(
                "engine download returned {}",
                response.status()
            )));
        }
        let content_length = response.content_length();
        if let Some(expected) = artifact.exact_size() {
            if content_length.is_some_and(|length| length != expected) {
                cleanup_candidate(&temporary, &version_dir).await;
                return Err(RavynError::Protocol(
                    "engine download Content-Length does not match its manifest".into(),
                ));
            }
        } else if content_length.is_some_and(|length| length > artifact.download_limit()) {
            cleanup_candidate(&temporary, &version_dir).await;
            return Err(RavynError::Protocol(
                "engine download Content-Length exceeds its signed size limit".into(),
            ));
        }
        let progress_total = artifact
            .exact_size()
            .or(content_length)
            .unwrap_or_else(|| artifact.download_limit());

        let install_result: Result<String> = async {
            let mut file = tokio::fs::OpenOptions::new()
                .create_new(true)
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
                if received > artifact.download_limit() || received > MAX_ENGINE_BYTES {
                    return Err(RavynError::Protocol(
                        "engine download exceeded its signed size limit".into(),
                    ));
                }
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                if let Some(report) = progress {
                    report(received, progress_total);
                }
            }
            file.sync_all().await?;
            drop(file);
            if let Some(report) = stage {
                report(EngineInstallStage::Verifying);
            }
            if let Some(expected) = artifact.exact_size() {
                if received != expected {
                    return Err(RavynError::provisioning(
                        ProvisioningErrorCode::DownloadInterrupted,
                        format!(
                            "engine download ended after {received} of {expected} expected bytes"
                        ),
                    )
                    .with_component(&artifact.engine)
                    .with_expected_version(&artifact.version));
                }
            }
            let actual = hex::encode(hasher.finalize());
            if !actual.eq_ignore_ascii_case(&artifact.sha256) {
                return Err(RavynError::provisioning(
                    ProvisioningErrorCode::ChecksumMismatch,
                    "engine download failed checksum verification",
                )
                .with_component(&artifact.engine)
                .with_expected_version(&artifact.version));
            }
            if let Some(report) = stage {
                report(EngineInstallStage::Installing);
            }
            if cancellation.is_cancelled() {
                return Err(RavynError::Cancelled);
            }
            materialize_verified_artifact(
                artifact,
                &temporary,
                &destination,
                &version_dir,
                cancellation,
            )
            .await
        }
        .await;
        let activation_sha256 = match install_result {
            Ok(checksum) => checksum,
            Err(error) => {
                cleanup_candidate(&temporary, &version_dir).await;
                return Err(error);
            }
        };
        if cancellation.is_cancelled() {
            cleanup_candidate(&temporary, &version_dir).await;
            return Err(RavynError::Cancelled);
        }
        let active = ActiveEngine {
            version: artifact.version.clone(),
            directory: Some(directory_name),
            filename: artifact.filename.clone(),
            sha256: activation_sha256,
        };
        if let Err(error) = self.write_active(&artifact.engine, &active).await {
            cleanup_candidate(&temporary, &version_dir).await;
            return Err(error);
        }
        Ok(destination)
    }

    async fn prepare_candidate(
        &self,
        artifact: &EngineArtifact,
    ) -> Result<(String, PathBuf, PathBuf, PathBuf)> {
        let engine_dir = self.root.join(&artifact.engine);
        tokio::fs::create_dir_all(&engine_dir).await?;
        let directory_name = format!("{}-{}", artifact.version, uuid::Uuid::new_v4().simple());
        validate_token(&directory_name, "installation directory")?;
        let version_dir = engine_dir.join(&directory_name);
        let destination = version_dir.join(&artifact.filename);
        let temporary = engine_dir.join(format!(".{directory_name}.download"));
        Ok((directory_name, version_dir, temporary, destination))
    }

    /// Return checksum-verified metadata for the active engine version.
    pub async fn active_info(&self, engine: &str) -> Result<Option<ActiveEngineInfo>> {
        validate_token(engine, "engine")?;
        let metadata_path = self.root.join(engine).join("active.json");
        if !tokio::fs::try_exists(&metadata_path).await? {
            return Ok(None);
        }
        let bytes = read_engine_metadata(&metadata_path).await?;
        let active: ActiveEngine = serde_json::from_slice(&bytes)?;
        active.validate()?;
        let executable = self
            .root
            .join(engine)
            .join(active.directory_name())
            .join(&active.filename);
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
        Ok(Some(ActiveEngineInfo {
            version: active.version,
            filename: active.filename,
            sha256: active.sha256,
            path: executable,
        }))
    }

    pub async fn active_path(&self, engine: &str) -> Result<Option<PathBuf>> {
        Ok(self.active_info(engine).await?.map(|info| info.path))
    }

    /// Remove active activation metadata after a failed first-time health check.
    /// Versioned files remain available for diagnostics and later cleanup, but
    /// no managed executable is considered active.
    pub async fn deactivate(&self, engine: &str) -> Result<()> {
        validate_token(engine, "engine")?;
        let engine_dir = self.root.join(engine);
        for name in ["active.json", ".active.json.tmp"] {
            let path = engine_dir.join(name);
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
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
            .join(previous.directory_name())
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

    /// Removes every installation directory for `engine` except the active
    /// candidate and the single previous candidate kept for rollback/diagnostics,
    /// and deletes any stale `.download` partial-download temp files left
    /// behind by an interrupted or failed install (including inside the
    /// versions that are kept).
    pub async fn cleanup_versions(&self, engine: &str) -> Result<EngineCleanupReport> {
        validate_token(engine, "engine")?;
        let engine_dir = self.root.join(engine);
        let mut report = EngineCleanupReport::default();
        if !tokio::fs::try_exists(&engine_dir).await? {
            return Ok(report);
        }

        let mut kept = std::collections::BTreeSet::new();
        for name in ["active.json", "previous.json"] {
            let path = engine_dir.join(name);
            if let Ok(bytes) = read_engine_metadata(&path).await {
                if let Ok(entry) = serde_json::from_slice::<ActiveEngine>(&bytes) {
                    if entry.validate().is_ok() {
                        kept.insert(entry.directory_name().to_owned());
                    }
                }
            }
        }

        let mut entries = tokio::fs::read_dir(&engine_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if file_type.is_file() {
                if name.starts_with('.')
                    && (name.ends_with(".download") || name.ends_with(".extract"))
                {
                    if let Ok(metadata) = entry.metadata().await {
                        report.bytes_freed = report.bytes_freed.saturating_add(metadata.len());
                    }
                    tokio::fs::remove_file(&path).await?;
                    report.removed_temp_files.push(name.to_owned());
                }
                continue;
            }
            if !file_type.is_dir() {
                continue;
            }
            if kept.contains(name) {
                let freed =
                    remove_download_temp_files(&path, &mut report.removed_temp_files).await?;
                report.bytes_freed = report.bytes_freed.saturating_add(freed);
                continue;
            }
            report.bytes_freed = report
                .bytes_freed
                .saturating_add(directory_size(&path).await.unwrap_or(0));
            tokio::fs::remove_dir_all(&path).await?;
            report.removed_versions.push(name.to_owned());
        }
        Ok(report)
    }
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
            max_size_bytes: None,
            filename: "ffmpeg.exe".into(),
            capabilities: vec!["transcode".into()],
            archive_member: None,
            member_sha256: None,
            installer: None,
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

    fn zip_archive_with(member: &str, content: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(member, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut writer, content).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[tokio::test]
    async fn installs_the_verified_member_of_an_archive_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let member_bytes = b"the real ffmpeg binary";
        let archive_bytes = zip_archive_with("dist/bin/ffmpeg.exe", member_bytes);

        let mut spec = artifact(&archive_bytes);
        spec.archive_member = Some("dist/bin/ffmpeg.exe".into());
        spec.member_sha256 = Some(hex::encode(Sha256::digest(member_bytes)));

        let installed = manager
            .install_verified(&spec, &archive_bytes)
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(&installed).await.unwrap(), member_bytes);
        let info = manager.active_info("ffmpeg").await.unwrap().unwrap();
        assert_eq!(info.sha256, spec.member_sha256.unwrap());

        // The archive itself must not linger next to the extracted binary.
        let mut entries = tokio::fs::read_dir(installed.parent().unwrap())
            .await
            .unwrap();
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        assert_eq!(names, vec!["ffmpeg.exe".to_owned()]);
    }

    #[tokio::test]
    async fn rejects_an_archive_member_whose_content_fails_checksum_verification() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let archive_bytes = zip_archive_with("bin/ffmpeg.exe", b"tampered payload");

        let mut spec = artifact(&archive_bytes);
        spec.archive_member = Some("bin/ffmpeg.exe".into());
        spec.member_sha256 = Some(hex::encode(Sha256::digest(b"expected payload")));

        assert!(
            manager
                .install_verified(&spec, &archive_bytes)
                .await
                .is_err()
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

    #[test]
    fn artifact_validation_rejects_unsafe_urls_and_archive_members() {
        let bytes = b"test executable";
        assert!(artifact(bytes).validate().is_ok());

        let mut insecure = artifact(bytes);
        insecure.url = "http://downloads.example.test/ffmpeg.exe".into();
        assert!(
            insecure.validate().is_err(),
            "a plain-HTTP artifact URL must never validate, even for a mock test server"
        );

        let mut with_credentials = artifact(bytes);
        with_credentials.url = "https://user:pass@downloads.example.test/ffmpeg.exe".into();
        assert!(with_credentials.validate().is_err());

        let mut with_fragment = artifact(bytes);
        with_fragment.url = "https://downloads.example.test/ffmpeg.exe#frag".into();
        assert!(with_fragment.validate().is_err());

        let mut traversal_member = artifact(bytes);
        traversal_member.archive_member = Some("../bin/ffmpeg.exe".into());
        traversal_member.member_sha256 = Some(hex::encode(Sha256::digest(b"x")));
        assert!(traversal_member.validate().is_err());

        let mut absolute_member = artifact(bytes);
        absolute_member.archive_member = Some("/bin/ffmpeg.exe".into());
        absolute_member.member_sha256 = Some(hex::encode(Sha256::digest(b"x")));
        assert!(absolute_member.validate().is_err());

        let mut missing_member_sha = artifact(bytes);
        missing_member_sha.archive_member = Some("bin/ffmpeg.exe".into());
        assert!(missing_member_sha.validate().is_err());

        let mut orphaned_member_sha = artifact(bytes);
        orphaned_member_sha.member_sha256 = Some(hex::encode(Sha256::digest(b"x")));
        assert!(orphaned_member_sha.validate().is_err());

        let mut valid_member = artifact(bytes);
        valid_member.archive_member = Some("bin/ffmpeg.exe".into());
        valid_member.member_sha256 = Some(hex::encode(Sha256::digest(b"x")));
        assert!(valid_member.validate().is_ok());

        let mut bounded_installer = artifact(bytes);
        bounded_installer.engine = "7zip".into();
        bounded_installer.url = "https://downloads.example.test/7zip.msi".into();
        bounded_installer.size_bytes = 0;
        bounded_installer.max_size_bytes = Some(4 * 1024 * 1024);
        bounded_installer.filename = "Files/7-Zip/7z.exe".into();
        bounded_installer.installer = Some(EngineInstaller {
            kind: EngineInstallerKind::MsiAdministrative,
        });
        assert!(bounded_installer.validate().is_ok());

        let mut unbounded_installer = bounded_installer.clone();
        unbounded_installer.max_size_bytes = None;
        assert!(unbounded_installer.validate().is_err());

        let mut non_windows_installer = bounded_installer.clone();
        non_windows_installer.target = "x86_64-unknown-linux-gnu".into();
        assert!(non_windows_installer.validate().is_err());
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
    async fn same_version_repair_preserves_a_distinct_rollback_candidate() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let first = manager
            .install_verified(&artifact(b"first"), b"first")
            .await
            .unwrap();
        let repaired = manager
            .install_verified(&artifact(b"second"), b"second")
            .await
            .unwrap();

        assert_ne!(first.parent(), repaired.parent());
        assert_eq!(manager.rollback("ffmpeg").await.unwrap(), first);
        assert_eq!(tokio::fs::read(first).await.unwrap(), b"first");
    }

    #[tokio::test]
    async fn cleanup_keeps_only_the_active_and_previous_versions() {
        let temp = tempfile::tempdir().unwrap();
        let manager = EngineManager::new(temp.path());
        let v1_path = manager
            .install_verified(&artifact(b"v1"), b"v1")
            .await
            .unwrap();
        let mut v2 = artifact(b"v2");
        v2.version = "7.2.0".into();
        let v2_path = manager.install_verified(&v2, b"v2").await.unwrap();
        let mut v3 = artifact(b"v3");
        v3.version = "7.3.0".into();
        let v3_path = manager.install_verified(&v3, b"v3").await.unwrap();

        // A stray partial-download temp file left in the still-active
        // installation directory, as if a prior operation crashed.
        let stale_temp = temp
            .path()
            .join("engines")
            .join("ffmpeg")
            .join(".interrupted.download");
        tokio::fs::write(&stale_temp, b"partial").await.unwrap();

        let removed_name = v1_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let report = manager.cleanup_versions("ffmpeg").await.unwrap();
        assert_eq!(report.removed_versions, vec![removed_name]);
        assert_eq!(
            report.removed_temp_files,
            vec![".interrupted.download".to_owned()]
        );
        assert!(!tokio::fs::try_exists(&stale_temp).await.unwrap());
        assert!(
            !tokio::fs::try_exists(v1_path.parent().unwrap())
                .await
                .unwrap()
        );
        assert!(
            tokio::fs::try_exists(v2_path.parent().unwrap())
                .await
                .unwrap()
        );
        assert!(
            tokio::fs::try_exists(v3_path.parent().unwrap())
                .await
                .unwrap()
        );
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
    fn manifest_rejects_duplicate_engine_targets() {
        let item = artifact(b"duplicate engine");
        let manifest = EngineManifest {
            schema_version: MANIFEST_SCHEMA,
            channel: "stable".into(),
            manifest_version: None,
            generated_at: None,
            expires_at: None,
            artifacts: vec![item.clone(), item],
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn remote_manifest_requires_complete_fresh_metadata() {
        let now = Utc::now();
        let mut manifest = EngineManifest {
            schema_version: MANIFEST_SCHEMA,
            channel: "stable".into(),
            manifest_version: Some(4),
            generated_at: Some(now - ChronoDuration::hours(1)),
            expires_at: Some(now + ChronoDuration::days(7)),
            artifacts: vec![artifact(b"remote engine")],
        };
        manifest.validate_remote("stable", now).unwrap();
        assert!(manifest.validate_remote("beta", now).is_err());
        manifest.expires_at = Some(now - ChronoDuration::seconds(1));
        assert!(manifest.validate_remote("stable", now).is_err());
        manifest.generated_at = None;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn signed_manifest_rejects_tampering() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let manifest = EngineManifest {
            schema_version: MANIFEST_SCHEMA,
            channel: "stable".into(),
            manifest_version: None,
            generated_at: None,
            expires_at: None,
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
