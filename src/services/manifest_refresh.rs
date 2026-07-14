//! Secure refresh and last-known-good caching for signed component manifests.
//!
//! Remote catalogues are optional. Ravyn remains usable offline through the
//! embedded catalogue, while configured release builds gain conditional HTTP
//! refresh, bounded metadata reads, replay protection, and an atomic cache.

use std::{path::{Path, PathBuf}, sync::Arc, time::Duration};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

use crate::{
    config::Config,
    error::{ProvisioningErrorCode, RavynError, Result},
    services::{
        components::embedded_manifest_public_key,
        engines::{EngineManifest, SignedEngineManifest},
    },
};

pub const MAX_REMOTE_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const METADATA_FILENAME: &str = "metadata.json";
const MANIFEST_FILENAME: &str = "manifest.json";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestRefreshPhase {
    Disabled,
    Idle,
    Checking,
    Current,
    Stale,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestRefreshStatus {
    pub configured: bool,
    pub phase: ManifestRefreshPhase,
    pub channel: String,
    pub endpoint: Option<String>,
    pub source: &'static str,
    pub manifest_version: Option<u64>,
    pub generated_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub stale: bool,
    pub etag: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_updated_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl ManifestRefreshStatus {
    pub fn disabled(channel: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            configured: false,
            phase: ManifestRefreshPhase::Disabled,
            channel: channel.into(),
            endpoint: None,
            source: "built-in",
            manifest_version: None,
            generated_at: None,
            expires_at: None,
            stale: false,
            etag: None,
            last_checked_at: None,
            last_updated_at: None,
            last_error: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestCacheMetadata {
    endpoint: String,
    channel: String,
    manifest_version: u64,
    generated_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    payload_sha256: String,
    etag: Option<String>,
    last_modified: Option<String>,
    last_checked_at: DateTime<Utc>,
    last_updated_at: DateTime<Utc>,
}

pub struct RemoteManifestRefresher {
    endpoint: url::Url,
    channel: String,
    public_key: [u8; 32],
    cache_dir: PathBuf,
    stale_grace: ChronoDuration,
    refresh_interval: Duration,
    client: Client,
    status: RwLock<ManifestRefreshStatus>,
    refresh_lock: Mutex<()>,
}

impl RemoteManifestRefresher {
    pub fn from_config(config: &Config) -> Result<Option<Arc<Self>>> {
        let Some(endpoint_value) = config.effective_component_manifest_endpoint() else {
            return Ok(None);
        };
        let endpoint = url::Url::parse(endpoint_value)?;
        validate_endpoint(&endpoint)?;
        let public_key = embedded_manifest_public_key()?.ok_or_else(|| {
            RavynError::provisioning(
                ProvisioningErrorCode::ManifestUnavailable,
                "remote component manifests are configured but this build has no release public key",
            )
        })?;
        let channel = config.component_manifest_channel.clone();
        let cache_dir = cache_directory(&config.data_dir, &channel);
        let stale_grace = ChronoDuration::seconds(
            i64::try_from(config.component_manifest_stale_grace_secs).map_err(|_| {
                RavynError::Invalid("component manifest stale grace is too large".into())
            })?,
        );
        let refresh_interval = Duration::from_secs(config.component_manifest_refresh_secs);
        let client = Client::builder()
            .user_agent(format!("Ravyn/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(config.connect_timeout())
            .read_timeout(config.read_timeout())
            .timeout(config.read_timeout() + config.connect_timeout())
            .https_only(true)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error("too many component manifest redirects")
                } else if attempt.url().scheme() == "https" {
                    attempt.follow()
                } else {
                    attempt.error("component manifest redirects must remain on HTTPS")
                }
            }))
            .build()?;
        let (metadata, cache_error) = match read_cache_metadata(&cache_dir) {
            Ok(Some(metadata)) => match validate_cached_manifest(
                &cache_dir.join(MANIFEST_FILENAME),
                &public_key,
                &channel,
                Utc::now(),
                stale_grace,
            ) {
                Ok(_) => (Some(metadata), None),
                Err(error) => (None, Some(error.to_string())),
            },
            Ok(None) => (None, None),
            Err(error) => (None, Some(error.to_string())),
        };
        let status = status_from_metadata(
            &channel,
            endpoint.as_str(),
            metadata.as_ref(),
            stale_grace,
            cache_error,
        );
        Ok(Some(Arc::new(Self {
            endpoint,
            channel,
            public_key,
            cache_dir,
            stale_grace,
            refresh_interval,
            client,
            status: RwLock::new(status),
            refresh_lock: Mutex::new(()),
        })))
    }

    pub fn cache_path(&self) -> PathBuf {
        self.cache_dir.join(MANIFEST_FILENAME)
    }

    pub fn refresh_interval(&self) -> Duration {
        self.refresh_interval
    }

    pub async fn status(&self) -> ManifestRefreshStatus {
        self.status.read().await.clone()
    }

    pub async fn refresh(&self, force: bool) -> Result<ManifestRefreshStatus> {
        let _guard = self.refresh_lock.lock().await;
        {
            let mut status = self.status.write().await;
            status.phase = ManifestRefreshPhase::Checking;
            status.last_error = None;
        }
        let result = self.perform_refresh(force).await;
        match result {
            Ok(status) => {
                *self.status.write().await = status.clone();
                Ok(status)
            }
            Err(error) => {
                let metadata = read_cache_metadata(&self.cache_dir).ok().flatten();
                let mut status = status_from_metadata(
                    &self.channel,
                    self.endpoint.as_str(),
                    metadata.as_ref(),
                    self.stale_grace,
                    Some(error.to_string()),
                );
                if metadata.is_none() {
                    status.phase = ManifestRefreshPhase::Error;
                    status.source = "built-in";
                }
                *self.status.write().await = status;
                Err(error)
            }
        }
    }

    pub fn spawn(self: &Arc<Self>) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(error) = service.refresh(false).await {
                tracing::warn!(%error, "initial component manifest background refresh failed");
            }
            let mut interval = tokio::time::interval(service.refresh_interval());
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(error) = service.refresh(false).await {
                    tracing::warn!(%error, "component manifest background refresh failed");
                }
            }
        });
    }

    async fn perform_refresh(&self, force: bool) -> Result<ManifestRefreshStatus> {
        let previous = read_cache_metadata(&self.cache_dir)?;
        let mut request = self.client.get(self.endpoint.clone());
        if !force {
            if let Some(metadata) = &previous {
                if let Some(etag) = &metadata.etag {
                    request = request.header(IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = &metadata.last_modified {
                    request = request.header(IF_MODIFIED_SINCE, last_modified);
                }
            }
        }
        let response = request.send().await?;
        let now = Utc::now();
        if response.status() == StatusCode::NOT_MODIFIED {
            let mut metadata = previous.ok_or_else(|| {
                RavynError::provisioning(
                    ProvisioningErrorCode::ManifestUnavailable,
                    "component manifest service returned not-modified without a local cache",
                )
            })?;
            validate_cached_manifest(
                &self.cache_path(),
                &self.public_key,
                &self.channel,
                now,
                self.stale_grace,
            )?;
            metadata.last_checked_at = now;
            write_json_atomic(&self.cache_dir.join(METADATA_FILENAME), &metadata).await?;
            return Ok(status_from_metadata(
                &self.channel,
                self.endpoint.as_str(),
                Some(&metadata),
                self.stale_grace,
                None,
            ));
        }
        if !response.status().is_success() {
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::ManifestUnavailable,
                format!(
                    "component manifest service returned HTTP {}",
                    response.status()
                ),
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size == 0 || size > MAX_REMOTE_MANIFEST_BYTES)
        {
            return Err(RavynError::Invalid(format!(
                "component manifest response must be between 1 and {MAX_REMOTE_MANIFEST_BYTES} bytes"
            )));
        }
        let etag = header_string(response.headers().get(ETAG))?;
        let last_modified = header_string(response.headers().get(LAST_MODIFIED))?;
        let bytes = read_bounded(response, MAX_REMOTE_MANIFEST_BYTES).await?;
        let signed: SignedEngineManifest = serde_json::from_slice(&bytes)?;
        let manifest = signed.verify(&self.public_key)?.clone();
        manifest.validate_remote(&self.channel, now)?;
        let (manifest_version, generated_at, expires_at) = manifest
            .remote_metadata()
            .expect("validate_remote requires metadata");
        let payload = serde_json::to_vec(&manifest)?;
        let payload_sha256 = hex::encode(Sha256::digest(payload));
        reject_replay(
            previous.as_ref(),
            manifest_version,
            generated_at,
            &payload_sha256,
        )?;
        let metadata = ManifestCacheMetadata {
            endpoint: self.endpoint.to_string(),
            channel: self.channel.clone(),
            manifest_version,
            generated_at,
            expires_at,
            payload_sha256,
            etag,
            last_modified,
            last_checked_at: now,
            last_updated_at: now,
        };
        write_cache_transaction(&self.cache_dir, &bytes, &metadata).await?;
        Ok(status_from_metadata(
            &self.channel,
            self.endpoint.as_str(),
            Some(&metadata),
            self.stale_grace,
            None,
        ))
    }
}

pub fn cache_directory(data_dir: &Path, channel: &str) -> PathBuf {
    data_dir
        .join("engines")
        .join("manifests")
        .join(channel)
}

pub fn cache_manifest_path(data_dir: &Path, channel: &str) -> PathBuf {
    cache_directory(data_dir, channel).join(MANIFEST_FILENAME)
}

pub fn validate_cached_manifest(
    path: &Path,
    public_key: &[u8; 32],
    channel: &str,
    now: DateTime<Utc>,
    stale_grace: ChronoDuration,
) -> Result<EngineManifest> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_REMOTE_MANIFEST_BYTES {
        return Err(RavynError::Invalid(
            "cached component manifest is empty, oversized, or not a regular file".into(),
        ));
    }
    let signed: SignedEngineManifest = serde_json::from_slice(&std::fs::read(path)?)?;
    let manifest = signed.verify(public_key)?.clone();
    if manifest.channel != channel {
        return Err(RavynError::Invalid(
            "cached component manifest channel does not match the configured channel".into(),
        ));
    }
    let (_, generated_at, expires_at) = manifest.remote_metadata().ok_or_else(|| {
        RavynError::Invalid("cached remote manifest is missing release metadata".into())
    })?;
    let cache_dir = path.parent().ok_or_else(|| {
        RavynError::Invalid("cached component manifest path has no parent directory".into())
    })?;
    if let Some(cache_metadata) = read_cache_metadata(cache_dir)? {
        validate_cache_metadata_pair(&manifest, &cache_metadata)?;
    }
    if generated_at > now + ChronoDuration::minutes(10) {
        return Err(RavynError::Invalid(
            "cached component manifest generation time is in the future".into(),
        ));
    }
    if expires_at + stale_grace <= now {
        return Err(RavynError::provisioning(
            ProvisioningErrorCode::ManifestUnavailable,
            "cached component manifest exceeded its last-known-good grace period",
        ));
    }
    Ok(manifest)
}

fn validate_cache_metadata_pair(
    manifest: &EngineManifest,
    metadata: &ManifestCacheMetadata,
) -> Result<()> {
    let (version, generated_at, expires_at) = manifest.remote_metadata().ok_or_else(|| {
        RavynError::Invalid("cached remote manifest is missing release metadata".into())
    })?;
    let payload_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(manifest)?));
    if metadata.channel != manifest.channel
        || metadata.manifest_version != version
        || metadata.generated_at != generated_at
        || metadata.expires_at != expires_at
        || !metadata.payload_sha256.eq_ignore_ascii_case(&payload_sha256)
    {
        return Err(RavynError::Invalid(
            "component manifest cache metadata does not match the signed payload".into(),
        ));
    }
    validate_endpoint(&url::Url::parse(&metadata.endpoint)?)?;
    Ok(())
}

fn validate_endpoint(endpoint: &url::Url) -> Result<()> {
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(RavynError::Invalid(
            "component manifest endpoint must be an HTTPS URL without credentials or fragments"
                .into(),
        ));
    }
    Ok(())
}

fn reject_replay(
    previous: Option<&ManifestCacheMetadata>,
    version: u64,
    generated_at: DateTime<Utc>,
    payload_sha256: &str,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if version < previous.manifest_version || generated_at < previous.generated_at.clone() {
        return Err(RavynError::provisioning(
            ProvisioningErrorCode::InvalidManifestSignature,
            "component manifest downgrade or replay was rejected",
        ));
    }
    if version == previous.manifest_version && payload_sha256 != previous.payload_sha256 {
        return Err(RavynError::provisioning(
            ProvisioningErrorCode::InvalidManifestSignature,
            "component manifest version was reused with different signed content",
        ));
    }
    Ok(())
}

fn status_from_metadata(
    channel: &str,
    endpoint: &str,
    metadata: Option<&ManifestCacheMetadata>,
    stale_grace: ChronoDuration,
    last_error: Option<String>,
) -> ManifestRefreshStatus {
    let now = Utc::now();
    let stale = metadata.is_some_and(|value| value.expires_at.clone() <= now);
    let usable = metadata.is_some_and(|value| value.expires_at.clone() + stale_grace > now);
    let phase = if metadata.is_none() {
        if last_error.is_some() {
            ManifestRefreshPhase::Error
        } else {
            ManifestRefreshPhase::Idle
        }
    } else if !usable {
        ManifestRefreshPhase::Error
    } else if stale {
        ManifestRefreshPhase::Stale
    } else {
        ManifestRefreshPhase::Current
    };
    ManifestRefreshStatus {
        configured: true,
        phase,
        channel: channel.to_owned(),
        endpoint: Some(endpoint.to_owned()),
        source: if usable { "remote-cache" } else { "built-in" },
        manifest_version: metadata.map(|value| value.manifest_version),
        generated_at: metadata.map(|value| value.generated_at.clone()),
        expires_at: metadata.map(|value| value.expires_at.clone()),
        stale,
        etag: metadata.and_then(|value| value.etag.clone()),
        last_checked_at: metadata.map(|value| value.last_checked_at.clone()),
        last_updated_at: metadata.map(|value| value.last_updated_at.clone()),
        last_error,
    }
}

fn read_cache_metadata(cache_dir: &Path) -> Result<Option<ManifestCacheMetadata>> {
    let path = cache_dir.join(METADATA_FILENAME);
    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 {
        return Err(RavynError::Invalid(
            "component manifest cache metadata is invalid or oversized".into(),
        ));
    }
    Ok(Some(serde_json::from_slice(&std::fs::read(path)?)?))
}

async fn read_bounded(response: reqwest::Response, limit: u64) -> Result<Vec<u8>> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let next_len = bytes.len().saturating_add(chunk.len());
        if u64::try_from(next_len).unwrap_or(u64::MAX) > limit {
            return Err(RavynError::Invalid(format!(
                "component manifest response exceeds {limit} bytes"
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(RavynError::Invalid(
            "component manifest response is empty".into(),
        ));
    }
    Ok(bytes)
}

fn header_string(value: Option<&reqwest::header::HeaderValue>) -> Result<Option<String>> {
    value
        .map(|value| {
            if value.as_bytes().len() > 4 * 1024 {
                return Err(RavynError::Protocol(
                    "component manifest response validator header is too large".into(),
                ));
            }
            value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| {
                    RavynError::Protocol(
                        "component manifest response header is not valid ASCII".into(),
                    )
                })
        })
        .transpose()
}

async fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    write_bytes_atomic(path, &serde_json::to_vec_pretty(value)?).await
}

async fn write_cache_transaction(
    cache_dir: &Path,
    manifest_bytes: &[u8],
    metadata: &ManifestCacheMetadata,
) -> Result<()> {
    tokio::fs::create_dir_all(cache_dir).await?;
    let manifest = cache_dir.join(MANIFEST_FILENAME);
    let metadata_path = cache_dir.join(METADATA_FILENAME);
    let manifest_temp = cache_dir.join("manifest.next");
    let metadata_temp = cache_dir.join("metadata.next");
    let manifest_backup = cache_dir.join("manifest.previous");
    let metadata_backup = cache_dir.join("metadata.previous");
    for path in [&manifest_temp, &metadata_temp, &manifest_backup, &metadata_backup] {
        let _ = tokio::fs::remove_file(path).await;
    }
    tokio::fs::write(&manifest_temp, manifest_bytes).await?;
    tokio::fs::write(&metadata_temp, serde_json::to_vec_pretty(metadata)?).await?;

    let had_manifest = tokio::fs::try_exists(&manifest).await?;
    let had_metadata = tokio::fs::try_exists(&metadata_path).await?;
    if had_manifest {
        tokio::fs::rename(&manifest, &manifest_backup).await?;
    }
    if had_metadata {
        if let Err(error) = tokio::fs::rename(&metadata_path, &metadata_backup).await {
            if had_manifest {
                let _ = tokio::fs::rename(&manifest_backup, &manifest).await;
            }
            return Err(error.into());
        }
    }

    let activation = async {
        tokio::fs::rename(&manifest_temp, &manifest).await?;
        tokio::fs::rename(&metadata_temp, &metadata_path).await?;
        Result::<()>::Ok(())
    }
    .await;
    if let Err(error) = activation {
        let _ = tokio::fs::remove_file(&manifest).await;
        let _ = tokio::fs::remove_file(&metadata_path).await;
        if had_manifest {
            let _ = tokio::fs::rename(&manifest_backup, &manifest).await;
        }
        if had_metadata {
            let _ = tokio::fs::rename(&metadata_backup, &metadata_path).await;
        }
        let _ = tokio::fs::remove_file(&manifest_temp).await;
        let _ = tokio::fs::remove_file(&metadata_temp).await;
        return Err(error);
    }
    let _ = tokio::fs::remove_file(manifest_backup).await;
    let _ = tokio::fs::remove_file(metadata_backup).await;
    Ok(())
}

async fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        RavynError::Invalid("component manifest cache path has no parent".into())
    })?;
    tokio::fs::create_dir_all(parent).await?;
    let temporary = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    let _ = tokio::fs::remove_file(&temporary).await;
    tokio::fs::write(&temporary, bytes).await?;
    let had_existing = tokio::fs::try_exists(path).await?;
    if had_existing {
        let _ = tokio::fs::remove_file(&backup).await;
        tokio::fs::rename(path, &backup).await?;
    }
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        if had_existing {
            let _ = tokio::fs::rename(&backup, path).await;
        }
        return Err(error.into());
    }
    if had_existing {
        let _ = tokio::fs::remove_file(backup).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn metadata(version: u64, generated_at: DateTime<Utc>, hash: &str) -> ManifestCacheMetadata {
        ManifestCacheMetadata {
            endpoint: "https://example.invalid/manifest.json".into(),
            channel: "stable".into(),
            manifest_version: version,
            generated_at,
            expires_at: generated_at + ChronoDuration::days(7),
            payload_sha256: hash.into(),
            etag: None,
            last_modified: None,
            last_checked_at: generated_at,
            last_updated_at: generated_at,
        }
    }

    #[test]
    fn rejects_manifest_downgrades_and_version_reuse() {
        let generated = Utc.with_ymd_and_hms(2026, 7, 14, 0, 0, 0).unwrap();
        let previous = metadata(8, generated, "aa");
        assert!(reject_replay(Some(&previous), 7, generated, "aa").is_err());
        assert!(
            reject_replay(
                Some(&previous),
                8,
                generated - ChronoDuration::seconds(1),
                "aa"
            )
            .is_err()
        );
        assert!(reject_replay(Some(&previous), 8, generated, "bb").is_err());
        reject_replay(Some(&previous), 9, generated, "bb").unwrap();
    }

    #[test]
    fn endpoint_validation_rejects_credentials_and_http() {
        assert!(validate_endpoint(&url::Url::parse("http://example.com/a").unwrap()).is_err());
        assert!(
            validate_endpoint(&url::Url::parse("https://user@example.com/a").unwrap()).is_err()
        );
        validate_endpoint(&url::Url::parse("https://example.com/a").unwrap()).unwrap();
    }

    #[test]
    fn empty_cache_starts_idle_without_reporting_an_error() {
        let status = status_from_metadata(
            "stable",
            "https://example.invalid/manifest.json",
            None,
            ChronoDuration::days(7),
            None,
        );
        assert_eq!(status.phase, ManifestRefreshPhase::Idle);
        assert_eq!(status.source, "built-in");
        assert!(!status.stale);
        assert!(status.last_error.is_none());
    }

    #[test]
    fn cache_metadata_must_match_the_signed_payload() {
        let generated = Utc.with_ymd_and_hms(2026, 7, 14, 0, 0, 0).unwrap();
        let manifest = EngineManifest {
            schema_version: 1,
            channel: "stable".into(),
            manifest_version: Some(9),
            generated_at: Some(generated),
            expires_at: Some(generated + ChronoDuration::days(7)),
            artifacts: Vec::new(),
        };
        let payload_hash = hex::encode(Sha256::digest(serde_json::to_vec(&manifest).unwrap()));
        let valid = metadata(9, generated, &payload_hash);
        validate_cache_metadata_pair(&manifest, &valid).unwrap();

        let mut invalid = valid;
        invalid.payload_sha256 = "00".repeat(32);
        assert!(validate_cache_metadata_pair(&manifest, &invalid).is_err());
    }
}
