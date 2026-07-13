//! Feature-based component selection, provisioning state machine, and manifest
//! provider abstraction.
//!
//! Components represent installable external engines (yt-dlp, FFmpeg, rqbit,
//! 7-Zip).  Features represent user-visible capabilities (video downloads,
//! torrent support, archive extraction, media merging) that require one or
//! more components.
//!
//! The module defines:
//!
//! - **Feature and component identifiers** mapping capabilities to engines.
//! - **Component state machine** tracking installation lifecycle.
//! - **Manifest provider trait** enabling built-in or remote signed manifests.
//! - **Setup profiles** (minimal, recommended, full, custom) for preset
//!   feature selections.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{ProvisioningErrorCode, RavynError, Result};

// ---------------------------------------------------------------------------
// Feature and component identifiers
// ---------------------------------------------------------------------------

/// A user-facing capability that may require one or more components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureId {
    /// HTTP downloads – always enabled, no external engine.
    StandardDownloads,
    /// Video and playlist extraction via yt-dlp.
    VideoExtraction,
    /// High-quality media merging/conversion via FFmpeg.
    MediaMerging,
    /// BitTorrent support via rqbit.
    TorrentSupport,
    /// Archive extraction via 7-Zip.
    ArchiveExtraction,
}

impl FeatureId {
    /// All features in display order.
    pub const ALL: &'static [FeatureId] = &[
        FeatureId::StandardDownloads,
        FeatureId::VideoExtraction,
        FeatureId::MediaMerging,
        FeatureId::TorrentSupport,
        FeatureId::ArchiveExtraction,
    ];

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::StandardDownloads => "Standard Downloads",
            Self::VideoExtraction => "Video & Playlist Extraction",
            Self::MediaMerging => "Media Merging & Conversion",
            Self::TorrentSupport => "BitTorrent Support",
            Self::ArchiveExtraction => "Archive Extraction",
        }
    }

    /// Components required to fulfil this feature.
    pub fn required_components(self) -> &'static [ComponentId] {
        match self {
            Self::StandardDownloads => &[],
            Self::VideoExtraction => &[ComponentId::Ytdlp],
            Self::MediaMerging => &[ComponentId::Ffmpeg],
            Self::TorrentSupport => &[ComponentId::Rqbit],
            Self::ArchiveExtraction => &[ComponentId::SevenZip],
        }
    }

    /// Short description shown to the user.
    pub fn description(self) -> &'static str {
        match self {
            Self::StandardDownloads => {
                "Basic HTTP downloads using the built-in segmented downloader."
            }
            Self::VideoExtraction => {
                "Extract and download video/audio from YouTube and thousands of other sites."
            }
            Self::MediaMerging => "Merge video+audio tracks, transcode, and convert media formats.",
            Self::TorrentSupport => "Download torrents via the rqbit engine with DHT support.",
            Self::ArchiveExtraction => "Extract ZIP, 7z, RAR, and other archive formats.",
        }
    }
}

/// An installable external engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentId {
    Ytdlp,
    Ffmpeg,
    Rqbit,
    SevenZip,
}

impl ComponentId {
    pub const ALL: &'static [ComponentId] = &[
        ComponentId::Ytdlp,
        ComponentId::Ffmpeg,
        ComponentId::Rqbit,
        ComponentId::SevenZip,
    ];

    /// Engine name used in the manifest and `EngineManager` directory.
    pub fn engine_name(self) -> &'static str {
        match self {
            Self::Ytdlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
            Self::Rqbit => "rqbit",
            Self::SevenZip => "7zip",
        }
    }

    /// Built-in command-name default of the matching config path field.
    ///
    /// A config value equal to this default means "no custom path". Note that
    /// 7-Zip's command default (`7z`) differs from its engine name (`7zip`).
    pub fn default_command(self) -> &'static str {
        match self {
            Self::Ytdlp => "yt-dlp",
            Self::Ffmpeg => "ffmpeg",
            Self::Rqbit => "rqbit",
            Self::SevenZip => "7z",
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ytdlp => "yt-dlp",
            Self::Ffmpeg => "FFmpeg",
            Self::Rqbit => "rqbit",
            Self::SevenZip => "7-Zip",
        }
    }

    /// Features that depend on this component.
    pub fn features(self) -> &'static [FeatureId] {
        match self {
            Self::Ytdlp => &[FeatureId::VideoExtraction],
            Self::Ffmpeg => &[FeatureId::MediaMerging],
            Self::Rqbit => &[FeatureId::TorrentSupport],
            Self::SevenZip => &[FeatureId::ArchiveExtraction],
        }
    }

    /// Default config field name for the component path.
    pub fn config_field(self) -> &'static str {
        match self {
            Self::Ytdlp => "ytdlp",
            Self::Ffmpeg => "ffmpeg",
            Self::Rqbit => "rqbit",
            Self::SevenZip => "seven_zip",
        }
    }
}

// ---------------------------------------------------------------------------
// Component states
// ---------------------------------------------------------------------------

/// Lifecycle state of a managed component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    /// No managed binary present; not requested by any enabled feature.
    NotInstalled,
    /// Queued for background download.
    Queued,
    /// Currently being downloaded.
    Downloading,
    /// Downloaded; SHA-256 checksum verification in progress.
    Verifying,
    /// Verifying passed; atomic file replacement in progress.
    Installing,
    /// Managed binary installed and checksum-verified.
    Installed,
    /// A newer manifest version is available.
    UpdateAvailable,
    /// Download, verification, or installation failed.
    Failed,
    /// No manifest artifact exists for the current platform.
    Unsupported,
    /// The active operation was cancelled by the user.
    Cancelled,
    /// User provided a custom path; managed binaries are not used.
    CustomPath,
    /// A configured custom executable cannot be resolved.
    CustomPathInvalid,
}

impl ComponentState {
    pub const ALL: &'static [ComponentState] = &[
        Self::NotInstalled,
        Self::Queued,
        Self::Downloading,
        Self::Verifying,
        Self::Installing,
        Self::Installed,
        Self::UpdateAvailable,
        Self::Failed,
        Self::Unsupported,
        Self::Cancelled,
        Self::CustomPath,
        Self::CustomPathInvalid,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NotInstalled => "Not Installed",
            Self::Queued => "Queued",
            Self::Downloading => "Downloading",
            Self::Verifying => "Verifying",
            Self::Installing => "Installing",
            Self::Installed => "Installed",
            Self::UpdateAvailable => "Update Available",
            Self::Failed => "Failed",
            Self::Unsupported => "Unsupported Platform",
            Self::Cancelled => "Cancelled",
            Self::CustomPath => "Custom Path",
            Self::CustomPathInvalid => "Invalid Custom Path",
        }
    }

    /// Whether the component can serve requests.
    pub fn is_operational(self) -> bool {
        matches!(self, Self::Installed | Self::UpdateAvailable | Self::CustomPath)
    }

    /// Whether the component is actively being provisioned.
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Downloading | Self::Verifying | Self::Installing
        )
    }
}

// ---------------------------------------------------------------------------
// Setup profiles
// ---------------------------------------------------------------------------

/// Preset feature-selection profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupProfile {
    /// Only standard HTTP downloads; no external engines.
    Minimal,
    /// Standard downloads, video extraction, media processing, and archive
    /// extraction (the plan's Recommended preset; torrents stay opt-in).
    Recommended,
    /// All features enabled; all engines installed.
    Full,
    /// User customises individual feature toggles.
    Custom,
}

impl SetupProfile {
    pub const ALL: &'static [SetupProfile] =
        &[Self::Minimal, Self::Recommended, Self::Full, Self::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Recommended => "Recommended",
            Self::Full => "Full",
            Self::Custom => "Custom",
        }
    }

    /// Returns the feature set for a non-custom profile.
    pub fn default_features(self) -> BTreeSet<FeatureId> {
        match self {
            Self::Minimal => BTreeSet::from([FeatureId::StandardDownloads]),
            Self::Recommended => {
                let mut set = BTreeSet::new();
                set.insert(FeatureId::StandardDownloads);
                set.insert(FeatureId::VideoExtraction);
                set.insert(FeatureId::MediaMerging);
                set.insert(FeatureId::ArchiveExtraction);
                set
            }
            Self::Full => FeatureId::ALL.iter().copied().collect(),
            Self::Custom => BTreeSet::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Persisted records
// ---------------------------------------------------------------------------

/// A user-selected feature with its enabled state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSelection {
    pub feature: FeatureId,
    pub enabled: bool,
}

/// Status of a single component returned to the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub component: ComponentId,
    pub state: ComponentState,
    pub enabled: bool,
    pub managed_version: Option<String>,
    pub detected_version: Option<String>,
    pub managed_path: Option<PathBuf>,
    pub custom_path: Option<PathBuf>,
    pub effective_path: Option<PathBuf>,
    pub available_version: Option<String>,
    pub rollback_available: bool,
    pub error_message: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>,
    pub install_started_at: Option<DateTime<Utc>>,
    pub install_completed_at: Option<DateTime<Utc>>,
}

/// Aggregate view of all components and features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentOverview {
    pub setup_profile: SetupProfile,
    pub features: Vec<FeatureStatus>,
    pub components: Vec<ComponentStatus>,
    pub platform: &'static str,
}

/// Status of a single feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStatus {
    pub feature: FeatureId,
    pub enabled: bool,
    pub satisfied: bool,
    pub required_components: Vec<ComponentId>,
}

/// Request body for saving feature selections.
#[derive(Debug, Clone, Deserialize)]
pub struct SaveFeatureSelections {
    pub setup_profile: SetupProfile,
    pub features: Vec<FeatureSelection>,
}

/// Request body for installing a component.
#[derive(Debug, Clone, Deserialize)]
pub struct InstallComponentRequest {
    /// Optional: force re-install even if already installed.
    #[serde(default)]
    pub force: bool,
}

/// Progress event emitted during component installation.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentProgress {
    pub component: ComponentId,
    pub state: ComponentState,
    pub progress_pct: Option<u8>,
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Provisioning cancellation
// ---------------------------------------------------------------------------

/// Per-component cancellation registry for concurrent provisioning tasks.
///
/// Each component receives an independent token. Cancelling FFmpeg therefore
/// never interrupts yt-dlp, rqbit, or 7-Zip. A cancelled operation remains in
/// the registry until its task exits, preventing a retry from racing stale
/// state writes from the previous task.
#[derive(Clone)]
pub struct ProvisioningCancellation {
    inner: Arc<Mutex<BTreeMap<ComponentId, CancellationToken>>>,
    limiter: Arc<tokio::sync::Semaphore>,
}

impl Default for ProvisioningCancellation {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
            limiter: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }
}

impl ProvisioningCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserve a component operation and return its cancellation token.
    pub fn begin(&self, component: ComponentId) -> Result<CancellationToken> {
        let mut operations = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if operations.contains_key(&component) {
            return Err(RavynError::Conflict(format!(
                "component {} already has an active operation",
                component.engine_name()
            )));
        }
        let token = CancellationToken::new();
        operations.insert(component, token.clone());
        Ok(token)
    }

    /// Wait for one of the two process-wide provisioning permits. Queued
    /// work remains cancellable, so an API retry never starts after cancel.
    pub async fn acquire(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<tokio::sync::OwnedSemaphorePermit> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(RavynError::Cancelled),
            permit = self.limiter.clone().acquire_owned() => permit.map_err(|_| {
                RavynError::Unavailable("component provisioning limiter is unavailable".into())
            }),
        }
    }

    /// Cancel only the selected component operation.
    pub fn cancel(&self, component: ComponentId) -> bool {
        let operations = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operations.get(&component).is_some_and(|token| {
            token.cancel();
            true
        })
    }

    /// Release the operation slot after the background task has exited.
    pub fn finish(&self, component: ComponentId) {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&component);
    }

    pub fn is_active(&self, component: ComponentId) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(&component)
    }

    pub fn cancel_all(&self) {
        let operations = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for token in operations.values() {
            token.cancel();
        }
    }
}

// ---------------------------------------------------------------------------
// Manifest provider abstraction
// ---------------------------------------------------------------------------

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const EMBEDDED_MANIFEST: &str = include_str!("../../assets/engines/stable.json");
const ENGINE_MANIFEST_PUBLIC_KEY_HEX: Option<&str> = option_env!("RAVYN_ENGINE_MANIFEST_PUBLIC_KEY");

/// Abstraction over manifest sources (built-in, local, remote, or hybrid).
pub trait ManifestProvider: Send + Sync {
    fn load(&self) -> Result<Option<crate::services::engines::EngineManifest>>;
    fn name(&self) -> &'static str;
}

/// Built-in release manifest compiled into the Ravyn binary.
pub struct BuiltInManifestProvider {
    manifest: crate::services::engines::EngineManifest,
}

impl BuiltInManifestProvider {
    pub fn new(manifest: crate::services::engines::EngineManifest) -> Result<Self> {
        manifest.validate()?;
        Ok(Self { manifest })
    }

    pub fn embedded() -> Result<Self> {
        let manifest = serde_json::from_str::<crate::services::engines::EngineManifest>(
            EMBEDDED_MANIFEST,
        )?;
        Self::new(manifest)
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            manifest: crate::services::engines::EngineManifest {
                schema_version: 1,
                channel: "stable".into(),
                artifacts: Vec::new(),
            },
        }
    }
}

impl ManifestProvider for BuiltInManifestProvider {
    fn load(&self) -> Result<Option<crate::services::engines::EngineManifest>> {
        Ok(Some(self.manifest.clone()))
    }

    fn name(&self) -> &'static str {
        "built-in"
    }
}

/// Optional operator-provided manifest under the Ravyn data directory.
pub struct FileManifestProvider {
    path: PathBuf,
    public_key: Option<[u8; 32]>,
}

impl FileManifestProvider {
    pub fn new(path: PathBuf, public_key: Option<[u8; 32]>) -> Self {
        Self { path, public_key }
    }
}

impl ManifestProvider for FileManifestProvider {
    fn load(&self) -> Result<Option<crate::services::engines::EngineManifest>> {
        let metadata = match std::fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_MANIFEST_BYTES {
            return Err(RavynError::Invalid(format!(
                "engine manifest must be a regular file between 1 and {MAX_MANIFEST_BYTES} bytes"
            )));
        }
        let public_key = self.public_key.ok_or_else(|| {
            RavynError::provisioning(
                ProvisioningErrorCode::ManifestUnavailable,
                "signed engine-manifest refresh is disabled because this build has no release public key",
            )
        })?;
        let bytes = std::fs::read(&self.path)?;
        let signed: crate::services::engines::SignedEngineManifest = serde_json::from_slice(&bytes)?;
        Ok(Some(signed.verify(&public_key)?.clone()))
    }

    fn name(&self) -> &'static str {
        "local-file"
    }
}

/// Uses the first available manifest and falls back to the embedded release
/// catalogue when the optional local override is absent.
pub struct HybridManifestProvider {
    primary: FileManifestProvider,
    fallback: BuiltInManifestProvider,
}

impl HybridManifestProvider {
    pub fn for_data_dir(data_dir: &Path) -> Result<Self> {
        Ok(Self {
            primary: FileManifestProvider::new(
                data_dir.join("engines").join("manifest.json"),
                embedded_manifest_public_key()?,
            ),
            fallback: BuiltInManifestProvider::embedded()?,
        })
    }
}

fn embedded_manifest_public_key() -> Result<Option<[u8; 32]>> {
    let Some(value) = ENGINE_MANIFEST_PUBLIC_KEY_HEX else {
        return Ok(None);
    };
    let bytes = hex::decode(value).map_err(|_| {
        RavynError::Invalid("RAVYN_ENGINE_MANIFEST_PUBLIC_KEY must be hexadecimal".into())
    })?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| {
        RavynError::Invalid(
            "RAVYN_ENGINE_MANIFEST_PUBLIC_KEY must contain exactly 32 bytes".into(),
        )
    })?;
    Ok(Some(key))
}

impl ManifestProvider for HybridManifestProvider {
    fn load(&self) -> Result<Option<crate::services::engines::EngineManifest>> {
        match self.primary.load()? {
            Some(manifest) => Ok(Some(manifest)),
            None => self.fallback.load(),
        }
    }

    fn name(&self) -> &'static str {
        "local-file+built-in"
    }
}

pub fn default_manifest_provider(data_dir: &Path) -> Result<Arc<dyn ManifestProvider>> {
    Ok(Arc::new(HybridManifestProvider::for_data_dir(data_dir)?))
}

/// Result of a successful managed component activation.
#[derive(Debug, Clone)]
pub struct InstalledComponent {
    pub path: PathBuf,
    pub version: String,
    pub detected_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub component: ComponentId,
    pub healthy: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Component manager
// ---------------------------------------------------------------------------

/// Orchestrates component provisioning, installation, and removal.
///
/// Holds the manifest provider, cancellation token, and a reference to the
/// `EngineManager` for low-level binary operations.
pub struct ComponentManager {
    engine_manager: crate::services::engines::EngineManager,
    manifest_provider: Arc<dyn ManifestProvider>,
    cancellation: CancellationToken,
    target: &'static str,
}

impl ComponentManager {
    pub fn new(
        data_dir: &Path,
        manifest_provider: Arc<dyn ManifestProvider>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            engine_manager: crate::services::engines::EngineManager::new(data_dir),
            manifest_provider,
            cancellation,
            target: current_target(),
        }
    }

    pub fn engine_manager(&self) -> &crate::services::engines::EngineManager {
        &self.engine_manager
    }

    pub fn target(&self) -> &'static str {
        self.target
    }

    pub fn manifest_provider_name(&self) -> &'static str {
        self.manifest_provider.name()
    }

    pub fn load_manifest(&self) -> Result<Option<crate::services::engines::EngineManifest>> {
        self.manifest_provider.load()
    }

    pub fn manifest_artifact(
        &self,
        component: ComponentId,
    ) -> Result<Option<crate::services::engines::EngineArtifact>> {
        let Some(manifest) = self.manifest_provider.load()? else {
            return Ok(None);
        };
        manifest.validate()?;
        Ok(manifest
            .artifacts
            .iter()
            .find(|artifact| {
                artifact.engine == component.engine_name() && artifact.target == self.target
            })
            .cloned())
    }

    pub fn available_version(&self, component: ComponentId) -> Result<Option<String>> {
        Ok(self
            .manifest_artifact(component)?
            .map(|artifact| artifact.version))
    }

    pub async fn rollback_available(&self, component: ComponentId) -> bool {
        tokio::fs::try_exists(
            self.engine_manager
                .root_dir()
                .join(component.engine_name())
                .join("previous.json"),
        )
        .await
        .unwrap_or(false)
    }

    /// Determine the reconciled component state from configured paths,
    /// persisted lifecycle state, verified managed metadata, and the manifest.
    pub async fn component_state(
        &self,
        component: ComponentId,
        configured: &crate::config::Config,
        records: &BTreeMap<ComponentId, PersistedComponent>,
        operation_active: bool,
    ) -> ComponentState {
        let config_path = component_config_path(component, configured);
        if config_path != Path::new(component.default_command()) {
            if !executable_resolves(config_path) {
                return ComponentState::CustomPathInvalid;
            }
            if records.get(&component).is_some_and(|record| {
                record.state == ComponentState::CustomPathInvalid
                    && record.custom_path.as_deref() == Some(config_path.as_path())
            }) {
                return ComponentState::CustomPathInvalid;
            }
            return ComponentState::CustomPath;
        }

        if operation_active {
            if let Some(record) = records.get(&component) {
                if record.state.is_busy() {
                    return record.state;
                }
            }
        }

        match self
            .engine_manager
            .active_info(component.engine_name())
            .await
        {
            Ok(Some(active)) => {
                if self
                    .manifest_artifact(component)
                    .ok()
                    .flatten()
                    .is_some_and(|artifact| {
                        version_cmp(&artifact.version, &active.version).is_gt()
                            || (version_cmp(&artifact.version, &active.version).is_eq()
                                && !artifact.sha256.eq_ignore_ascii_case(&active.sha256))
                    })
                {
                    return ComponentState::UpdateAvailable;
                }
                return ComponentState::Installed;
            }
            Ok(None) => {}
            Err(_) => return ComponentState::Failed,
        }

        if let Some(record) = records.get(&component) {
            if matches!(
                record.state,
                ComponentState::Failed
                    | ComponentState::Unsupported
                    | ComponentState::Cancelled
                    | ComponentState::CustomPathInvalid
            ) {
                return record.state;
            }
        }

        match self.manifest_artifact(component) {
            Ok(Some(_)) => ComponentState::NotInstalled,
            Ok(None) => ComponentState::Unsupported,
            Err(_) => ComponentState::Failed,
        }
    }

    pub async fn effective_path(
        &self,
        component: ComponentId,
        configured: &crate::config::Config,
        _records: &BTreeMap<ComponentId, PersistedComponent>,
    ) -> Option<PathBuf> {
        let config_path = component_config_path(component, configured);
        if config_path != Path::new(component.default_command()) {
            return executable_resolves(config_path).then(|| config_path.clone());
        }
        self.engine_manager
            .active_path(component.engine_name())
            .await
            .ok()
            .flatten()
    }

    pub async fn installed_version(&self, component: ComponentId) -> Option<String> {
        self.engine_manager
            .active_info(component.engine_name())
            .await
            .ok()
            .flatten()
            .map(|info| info.version)
    }

    /// Return the currently verified managed component, if one remains active.
    pub async fn active_managed_component(
        &self,
        component: ComponentId,
    ) -> Result<Option<InstalledComponent>> {
        Ok(self
            .engine_manager
            .active_info(component.engine_name())
            .await?
            .map(|active| InstalledComponent {
                path: active.path,
                version: active.version,
                detected_version: None,
            }))
    }

    /// Reconcile the state after a failed or cancelled update. A previously
    /// verified version remains operational and is exposed as installed or
    /// update-available instead of being hidden behind a failed state.
    pub async fn state_after_unsuccessful_operation(
        &self,
        component: ComponentId,
        empty_state: ComponentState,
    ) -> Result<(ComponentState, Option<InstalledComponent>)> {
        let active = self.active_managed_component(component).await?;
        let state = match active.as_ref() {
            Some(installed)
                if self
                    .available_version(component)?
                    .is_some_and(|available| available.as_str() != installed.version.as_str()) =>
            {
                ComponentState::UpdateAvailable
            }
            Some(_) => ComponentState::Installed,
            None => empty_state,
        };
        Ok((state, active))
    }

    pub async fn feature_satisfied(
        &self,
        feature: FeatureId,
        enabled: bool,
        configured: &crate::config::Config,
        records: &BTreeMap<ComponentId, PersistedComponent>,
        active_operations: &ProvisioningCancellation,
    ) -> bool {
        if !enabled {
            return true;
        }
        for &component in feature.required_components() {
            if !self
                .component_state(
                    component,
                    configured,
                    records,
                    active_operations.is_active(component),
                )
                .await
                .is_operational()
            {
                return false;
            }
        }
        true
    }

    pub async fn install_component(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
    ) -> Result<InstalledComponent> {
        self.install_component_with_progress(component, config, None, None)
            .await
    }

    pub async fn install_component_with_progress(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
        stage: Option<&(dyn Fn(ComponentState) + Send + Sync)>,
    ) -> Result<InstalledComponent> {
        let artifact = self.manifest_artifact(component)?.ok_or_else(|| {
            RavynError::provisioning(
                ProvisioningErrorCode::PlatformUnsupported,
                format!(
                    "no managed {} artifact is available for {}",
                    component.engine_name(),
                    self.target
                ),
            )
            .with_component(component.engine_name())
            .with_target(self.target)
        })?;
        let stage_adapter = |engine_stage: crate::services::engines::EngineInstallStage| {
            if let Some(report) = stage {
                report(match engine_stage {
                    crate::services::engines::EngineInstallStage::Downloading => {
                        ComponentState::Downloading
                    }
                    crate::services::engines::EngineInstallStage::Verifying => {
                        ComponentState::Verifying
                    }
                    crate::services::engines::EngineInstallStage::Installing => {
                        ComponentState::Installing
                    }
                });
            }
        };
        let path = self
            .engine_manager
            .download_and_install(
                config,
                &artifact,
                &self.cancellation,
                progress,
                Some(&stage_adapter),
            )
            .await?;
        let health = self
            .health_check(component, config, &BTreeMap::new())
            .await;
        if !health.healthy {
            let health_error = health
                .message
                .unwrap_or_else(|| "component health check failed".into());
            if self.rollback_available(component).await {
                if let Err(error) = self.rollback_component(component, config).await {
                    tracing::error!(
                        %error,
                        component = component.engine_name(),
                        "managed component failed its health check and rollback failed"
                    );
                }
            } else if let Err(error) = self
                .engine_manager
                .deactivate(component.engine_name())
                .await
            {
                tracing::warn!(
                    %error,
                    component = component.engine_name(),
                    "failed to deactivate an unhealthy managed component"
                );
            }
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::HealthCheckFailed,
                format!(
                    "managed {} failed its post-install health check: {health_error}",
                    component.engine_name()
                ),
            )
            .with_component(component.engine_name())
            .with_stage("install")
            .with_expected_version(&artifact.version));
        }
        let detected_version = health.version.ok_or_else(|| {
            RavynError::provisioning(
                ProvisioningErrorCode::HealthCheckFailed,
                format!(
                    "managed {} did not report a version during its health check",
                    component.engine_name()
                ),
            )
            .with_component(component.engine_name())
            .with_stage("install")
            .with_expected_version(&artifact.version)
        })?;
        if !detected_version
            .to_ascii_lowercase()
            .contains(&artifact.version.to_ascii_lowercase())
        {
            let _ = self.engine_manager.deactivate(component.engine_name()).await;
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::HealthCheckFailed,
                format!(
                    "managed {} reported version {detected_version:?}, expected {}",
                    component.engine_name(),
                    artifact.version
                ),
            )
            .with_component(component.engine_name())
            .with_stage("install")
            .with_expected_version(&artifact.version)
            .with_detected_version(&detected_version));
        }
        if let Err(error) = self.cleanup_component(component).await {
            tracing::warn!(
                %error,
                component = component.engine_name(),
                "failed to clean up superseded managed engine versions after install"
            );
        }
        Ok(InstalledComponent {
            path,
            version: artifact.version,
            detected_version: Some(detected_version),
        })
    }

    pub async fn health_check(
        &self,
        component: ComponentId,
        configured: &crate::config::Config,
        records: &BTreeMap<ComponentId, PersistedComponent>,
    ) -> ComponentHealth {
        let Some(path) = self.effective_path(component, configured, records).await else {
            return ComponentHealth {
                component,
                healthy: false,
                path: None,
                version: None,
                message: Some("component executable is not available".into()),
            };
        };

        let mut command = tokio::process::Command::new(&path);
        match component {
            ComponentId::Ytdlp | ComponentId::Rqbit => {
                command.arg("--version");
            }
            ComponentId::Ffmpeg => {
                command.arg("-version");
            }
            ComponentId::SevenZip => {
                command.arg("i");
            }
        }
        let limits = crate::services::process::ProcessLimits {
            wall_time: std::time::Duration::from_secs(10),
            cpu_time: std::time::Duration::from_secs(5),
            memory_bytes: 512 * 1024 * 1024,
            output_file_bytes: None,
            stdout_bytes: 64 * 1024,
            stderr_bytes: 64 * 1024,
        };
        match crate::services::process::run(
            &mut command,
            &limits,
            None,
            CancellationToken::new(),
        )
        .await
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let version = stdout
                    .lines()
                    .chain(stderr.lines())
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(ToOwned::to_owned);
                let capability_error = match component {
                    ComponentId::Rqbit => rqbit_api_health(configured).await.err(),
                    ComponentId::Ffmpeg => ffmpeg_capability_check(&path).await.err(),
                    ComponentId::SevenZip => seven_zip_capability_check(&path).await.err(),
                    ComponentId::Ytdlp => ytdlp_capability_check(&path).await.err(),
                };
                if let Some(error) = capability_error {
                    return ComponentHealth {
                        component,
                        healthy: false,
                        path: Some(path),
                        version,
                        message: Some(error.to_string()),
                    };
                }
                ComponentHealth {
                    component,
                    healthy: true,
                    path: Some(path),
                    version,
                    message: None,
                }
            }
            Ok(output) => ComponentHealth {
                component,
                healthy: false,
                path: Some(path),
                version: None,
                message: Some(format!("component exited with {}", output.status)),
            },
            Err(error) => ComponentHealth {
                component,
                healthy: false,
                path: Some(path),
                version: None,
                message: Some(error.to_string()),
            },
        }
    }

    /// Rolls back to the previous checksum-verified version and runs the same
    /// health check (process launch, version detection, capability
    /// verification) applied after a fresh install. If the restored version
    /// fails the check, it is deactivated rather than left as the reported
    /// active version.
    pub async fn rollback_component(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
    ) -> Result<InstalledComponent> {
        let path = self.engine_manager.rollback(component.engine_name()).await?;
        let health = self.health_check(component, config, &BTreeMap::new()).await;
        if !health.healthy {
            let health_error = health
                .message
                .unwrap_or_else(|| "component health check failed".into());
            if let Err(error) = self
                .engine_manager
                .deactivate(component.engine_name())
                .await
            {
                tracing::warn!(
                    %error,
                    component = component.engine_name(),
                    "failed to deactivate a managed component after a failed rollback health check"
                );
            }
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::RollbackFailed,
                format!(
                    "rolled-back {} failed its post-rollback health check: {health_error}",
                    component.engine_name()
                ),
            )
            .with_component(component.engine_name())
            .with_stage("rollback")
            .with_path(path.display().to_string()));
        }
        if let Err(error) = self.cleanup_component(component).await {
            tracing::warn!(
                %error,
                component = component.engine_name(),
                "failed to clean up superseded managed engine versions after rollback"
            );
        }
        Ok(InstalledComponent {
            path,
            version: health.version.clone().unwrap_or_default(),
            detected_version: health.version,
        })
    }

    pub async fn remove_component(&self, component: ComponentId) -> Result<()> {
        let engine_dir = self.engine_manager.root_dir().join(component.engine_name());
        if tokio::fs::try_exists(&engine_dir).await? {
            tokio::fs::remove_dir_all(&engine_dir).await?;
        }
        Ok(())
    }

    /// Deletes superseded version directories (beyond the active and single
    /// previous version kept for rollback/diagnostics) and stale `.download`
    /// temp files for `component`.
    pub async fn cleanup_component(
        &self,
        component: ComponentId,
    ) -> Result<crate::services::engines::EngineCleanupReport> {
        self.engine_manager
            .cleanup_versions(component.engine_name())
            .await
    }
}

/// Compares vendor versions without treating a lower version as an update.
///
/// Engine vendors use both dotted semantic versions and date-like releases.
/// Comparing their numeric runs makes `2025.10.1` sort after `2025.9.30`
/// while leaving non-numeric suffixes as a deterministic tie-breaker.
fn version_cmp(left: &str, right: &str) -> Ordering {
    let left_numbers = left
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let right_numbers = right
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if !left_numbers.is_empty() && !right_numbers.is_empty() {
        for (left, right) in left_numbers.iter().zip(right_numbers.iter()) {
            let left = left.trim_start_matches('0');
            let right = right.trim_start_matches('0');
            let numeric_order = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
            if !numeric_order.is_eq() {
                return numeric_order;
            }
        }
        return left_numbers.len().cmp(&right_numbers.len());
    }

    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}

/// Verify the HTTP API that Ravyn's torrent adapter actually uses. A managed
/// rqbit executable alone is not operational until this request succeeds.
async fn rqbit_api_health(config: &crate::config::Config) -> Result<()> {
    const MAX_RQBIT_HEALTH_BYTES: usize = 4 * 1024 * 1024;
    const REQUIRED_ENDPOINTS: &[&str] = &[
        "GET /torrents",
        "POST /torrents",
        "GET /torrents/{id}/stats/v1",
        "POST /torrents/{id}/pause",
        "POST /torrents/{id}/start",
    ];

    let mut request = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(config.rqbit_timeout_secs.min(10)))
        .build()?
        .get(config.rqbit_api.trim_end_matches('/'));
    if let (Some(username), Some(password)) = (&config.rqbit_username, &config.rqbit_password) {
        request = request.basic_auth(username, Some(password));
    }
    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(RavynError::Unavailable(format!(
            "rqbit HTTP health check returned {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RQBIT_HEALTH_BYTES as u64)
    {
        return Err(RavynError::Protocol(
            "rqbit HTTP health response exceeds the 4 MiB limit".into(),
        ));
    }
    let body = response.bytes().await?;
    if body.len() > MAX_RQBIT_HEALTH_BYTES {
        return Err(RavynError::Protocol(
            "rqbit HTTP health response exceeds the 4 MiB limit".into(),
        ));
    }
    let root: serde_json::Value = serde_json::from_slice(&body)?;
    if root.get("server").and_then(serde_json::Value::as_str) != Some("rqbit") {
        return Err(RavynError::Unavailable(
            "rqbit HTTP health response did not identify rqbit".into(),
        ));
    }
    let apis = root
        .get("apis")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| RavynError::Unavailable("rqbit HTTP health response has no API map".into()))?;
    let missing = REQUIRED_ENDPOINTS
        .iter()
        .filter(|endpoint| !apis.keys().any(|actual| rqbit_endpoint_matches(actual, endpoint)))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(RavynError::Unavailable(format!(
            "rqbit HTTP API is missing required endpoints: {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

fn rqbit_endpoint_matches(actual: &str, required: &str) -> bool {
    let normalize = |value: &str| {
        value
            .replace("{id_or_infohash}", "{id}")
            .replace("{torrent_id}", "{id}")
    };
    normalize(actual) == normalize(required)
}

const CAPABILITY_PROCESS_LIMITS: crate::services::process::ProcessLimits =
    crate::services::process::ProcessLimits {
        wall_time: std::time::Duration::from_secs(15),
        cpu_time: std::time::Duration::from_secs(10),
        memory_bytes: 512 * 1024 * 1024,
        output_file_bytes: None,
        stdout_bytes: 64 * 1024,
        stderr_bytes: 64 * 1024,
    };

/// Runs a minimal decode-to-null encode through a synthetic `lavfi` source so
/// a health check exercises real codec/muxer capability rather than just the
/// version banner.
async fn ffmpeg_capability_check(path: &Path) -> Result<()> {
    let mut command = tokio::process::Command::new(path);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=16x16:d=0.1",
        "-frames:v",
        "1",
        "-f",
        "null",
        "-",
    ]);
    let output = crate::services::process::run(
        &mut command,
        &CAPABILITY_PROCESS_LIMITS,
        None,
        CancellationToken::new(),
    )
    .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RavynError::Unavailable(format!(
            "ffmpeg failed a minimal lavfi encode/decode capability check: {}",
            output.status
        )))
    }
}

const YTDLP_REQUIRED_OPTIONS: &[&str] = &[
    "--dump-single-json",
    "--download-archive",
    "--ffmpeg-location",
    "--progress-template",
];

/// Runs `--help` and checks for options the adapter layer relies on, catching
/// a managed yt-dlp build that launches but is too old or stripped down.
async fn ytdlp_capability_check(path: &Path) -> Result<()> {
    let mut command = tokio::process::Command::new(path);
    command.arg("--help");
    let output = crate::services::process::run(
        &mut command,
        &CAPABILITY_PROCESS_LIMITS,
        None,
        CancellationToken::new(),
    )
    .await?;
    if !output.status.success() {
        return Err(RavynError::Unavailable(format!(
            "yt-dlp --help capability probe exited with {}",
            output.status
        )));
    }
    let help = String::from_utf8_lossy(&output.stdout);
    let missing: Vec<&str> = YTDLP_REQUIRED_OPTIONS
        .iter()
        .copied()
        .filter(|option| !help.contains(option))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(RavynError::Unavailable(format!(
            "yt-dlp is missing required options: {}",
            missing.join(", ")
        )))
    }
}

/// A hand-built, uncompressed single-entry ZIP archive (one empty file named
/// `health.check`) used to prove 7-Zip can actually read and test an archive
/// rather than merely printing its own version banner.
fn minimal_test_archive() -> Vec<u8> {
    const NAME: &[u8] = b"health.check";
    let mut bytes = Vec::with_capacity(128);
    let local_header_offset = 0u32;

    // Local file header.
    bytes.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    bytes.extend_from_slice(&0u16.to_le_bytes()); // mod time
    bytes.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date (1980-01-01)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // crc32 of empty content
    bytes.extend_from_slice(&0u32.to_le_bytes()); // compressed size
    bytes.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size
    bytes.extend_from_slice(&(NAME.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // extra length
    bytes.extend_from_slice(NAME);

    let central_directory_offset = bytes.len() as u32;

    // Central directory header.
    bytes.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&20u16.to_le_bytes()); // version made by
    bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&0u16.to_le_bytes()); // method: stored
    bytes.extend_from_slice(&0u16.to_le_bytes()); // mod time
    bytes.extend_from_slice(&0x0021u16.to_le_bytes()); // mod date
    bytes.extend_from_slice(&0u32.to_le_bytes()); // crc32
    bytes.extend_from_slice(&0u32.to_le_bytes()); // compressed size
    bytes.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size
    bytes.extend_from_slice(&(NAME.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // extra length
    bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length
    bytes.extend_from_slice(&0u16.to_le_bytes()); // disk number start
    bytes.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    bytes.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    bytes.extend_from_slice(&local_header_offset.to_le_bytes());
    bytes.extend_from_slice(NAME);

    let central_directory_size = bytes.len() as u32 - central_directory_offset;

    // End of central directory record.
    bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // disk number
    bytes.extend_from_slice(&0u16.to_le_bytes()); // disk with central dir
    bytes.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
    bytes.extend_from_slice(&1u16.to_le_bytes()); // total entries
    bytes.extend_from_slice(&central_directory_size.to_le_bytes());
    bytes.extend_from_slice(&central_directory_offset.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length

    bytes
}

/// Writes the synthetic archive to a scratch file and asks 7-Zip to test its
/// integrity, proving the managed binary can actually open and read archives.
async fn seven_zip_capability_check(path: &Path) -> Result<()> {
    let archive_path =
        std::env::temp_dir().join(format!("ravyn-7z-health-{}.zip", uuid::Uuid::new_v4()));
    tokio::fs::write(&archive_path, minimal_test_archive()).await?;
    let mut command = tokio::process::Command::new(path);
    command.arg("t").arg(&archive_path);
    let result = crate::services::process::run(
        &mut command,
        &CAPABILITY_PROCESS_LIMITS,
        None,
        CancellationToken::new(),
    )
    .await;
    let _ = tokio::fs::remove_file(&archive_path).await;
    let output = result?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RavynError::Unavailable(format!(
            "7-Zip failed to test a minimal archive: {}",
            output.status
        )))
    }
}

fn component_config_path(
    component: ComponentId,
    config: &crate::config::Config,
) -> &PathBuf {
    match component {
        ComponentId::Ytdlp => &config.ytdlp,
        ComponentId::Ffmpeg => &config.ffmpeg,
        ComponentId::Rqbit => &config.rqbit,
        ComponentId::SevenZip => &config.seven_zip,
    }
}

fn executable_resolves(path: &Path) -> bool {
    if path.is_absolute() || path.components().count() > 1 {
        return path.is_file();
    }
    let Some(search_path) = std::env::var_os("PATH") else {
        return false;
    };
    #[cfg(windows)]
    let extensions = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![".EXE".into(), ".CMD".into(), ".BAT".into()]);
    for directory in std::env::split_paths(&search_path) {
        let candidate = directory.join(path);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        for extension in &extensions {
            let candidate = directory.join(format!("{}{}", path.display(), extension));
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Target triple resolution
// ---------------------------------------------------------------------------

/// Resolve the compile-time target triple at runtime.
pub fn current_target() -> &'static str {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "unknown"
    }
}

// ---------------------------------------------------------------------------
// Persisted component record (stored in the database)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedComponent {
    pub component: ComponentId,
    pub state: ComponentState,
    pub managed_version: Option<String>,
    pub detected_version: Option<String>,
    pub managed_path: Option<PathBuf>,
    pub custom_path: Option<PathBuf>,
    pub error_message: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>,
    pub install_started_at: Option<DateTime<Utc>>,
    pub install_completed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Setup profile presets
// ---------------------------------------------------------------------------

/// Resolve a setup profile into a set of feature selections.
pub fn resolve_profile_features(profile: SetupProfile) -> BTreeSet<FeatureId> {
    profile.default_features()
}

/// Determine which components are required for a set of features.
pub fn required_components_for_features(features: &BTreeSet<FeatureId>) -> BTreeSet<ComponentId> {
    let mut components = BTreeSet::new();
    for feature in features {
        for component in feature.required_components() {
            components.insert(*component);
        }
    }
    components
}

/// Reconstruct a feature set from the stored `features_json` strings.
///
/// The storage layer serialises each enabled `FeatureId` with serde, producing
/// strings like `"video_extraction"`.  This helper deserialises them back into
/// a `BTreeSet`.
pub fn features_from_stored_json(features_json: &[String]) -> Result<BTreeSet<FeatureId>> {
    let mut set = BTreeSet::new();
    for value in features_json {
        let feature: FeatureId = serde_json::from_str(value)
            .map_err(|_| RavynError::Invalid(format!("invalid feature value: {value}")))?;
        set.insert(feature);
    }
    Ok(set)
}

/// Compute the effective feature set from profile and stored selections.
///
/// For non-custom profiles the feature set is derived from the profile alone.
/// For `Custom` the stored JSON selections are deserialised.
pub fn effective_feature_set(
    profile: SetupProfile,
    features_json: &[String],
) -> Result<BTreeSet<FeatureId>> {
    let mut features = match profile {
        SetupProfile::Custom => features_from_stored_json(features_json)?,
        _ => resolve_profile_features(profile),
    };
    features.insert(FeatureId::StandardDownloads);
    Ok(features)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_test_archive_is_structurally_valid() {
        let bytes = minimal_test_archive();
        assert_eq!(&bytes[0..4], &0x0403_4b50u32.to_le_bytes());
        let eocd = &bytes[bytes.len() - 22..];
        assert_eq!(&eocd[0..4], &0x0605_4b50u32.to_le_bytes());
        let entries_total = u16::from_le_bytes([eocd[10], eocd[11]]);
        assert_eq!(entries_total, 1);
        let cd_size = u32::from_le_bytes([eocd[12], eocd[13], eocd[14], eocd[15]]);
        let cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]);
        assert_eq!(cd_offset as usize + cd_size as usize, bytes.len() - 22);
        assert_eq!(&bytes[cd_offset as usize..cd_offset as usize + 4], &0x0201_4b50u32.to_le_bytes());
    }

    #[test]
    fn default_commands_match_config_defaults() {
        // 7-Zip's command default differs from its engine directory name; a
        // config left at the default must never be treated as a custom path.
        assert_eq!(ComponentId::SevenZip.default_command(), "7z");
        assert_eq!(ComponentId::SevenZip.engine_name(), "7zip");
        assert_eq!(ComponentId::Ytdlp.default_command(), "yt-dlp");
    }

    #[test]
    fn recommended_profile_matches_the_design_plan() {
        let features = SetupProfile::Recommended.default_features();
        assert!(features.contains(&FeatureId::StandardDownloads));
        assert!(features.contains(&FeatureId::VideoExtraction));
        assert!(features.contains(&FeatureId::MediaMerging));
        assert!(features.contains(&FeatureId::ArchiveExtraction));
        assert!(!features.contains(&FeatureId::TorrentSupport));
    }

    #[test]
    fn feature_to_component_mapping() {
        assert!(
            FeatureId::StandardDownloads
                .required_components()
                .is_empty()
        );
        assert_eq!(
            FeatureId::VideoExtraction.required_components(),
            &[ComponentId::Ytdlp]
        );
        assert_eq!(
            FeatureId::MediaMerging.required_components(),
            &[ComponentId::Ffmpeg]
        );
        assert_eq!(
            FeatureId::TorrentSupport.required_components(),
            &[ComponentId::Rqbit]
        );
        assert_eq!(
            FeatureId::ArchiveExtraction.required_components(),
            &[ComponentId::SevenZip]
        );
    }

    #[test]
    fn component_all_features() {
        assert_eq!(ComponentId::Ytdlp.features(), &[FeatureId::VideoExtraction]);
        assert_eq!(ComponentId::Ffmpeg.features(), &[FeatureId::MediaMerging]);
        assert_eq!(ComponentId::Rqbit.features(), &[FeatureId::TorrentSupport]);
        assert_eq!(
            ComponentId::SevenZip.features(),
            &[FeatureId::ArchiveExtraction]
        );
    }

    #[test]
    fn state_machine_properties() {
        assert!(ComponentState::Installed.is_operational());
        assert!(ComponentState::CustomPath.is_operational());
        assert!(ComponentState::UpdateAvailable.is_operational());
        assert!(!ComponentState::NotInstalled.is_operational());
        assert!(!ComponentState::Failed.is_operational());
        assert!(!ComponentState::Cancelled.is_operational());

        assert!(ComponentState::Queued.is_busy());
        assert!(ComponentState::Downloading.is_busy());
        assert!(ComponentState::Verifying.is_busy());
        assert!(ComponentState::Installing.is_busy());
        assert!(!ComponentState::Installed.is_busy());
        assert!(!ComponentState::Failed.is_busy());
        assert!(!ComponentState::Cancelled.is_busy());
    }

    #[test]
    fn setup_profile_features() {
        assert_eq!(
            resolve_profile_features(SetupProfile::Minimal),
            BTreeSet::from([FeatureId::StandardDownloads])
        );
        assert!(
            resolve_profile_features(SetupProfile::Recommended)
                .contains(&FeatureId::VideoExtraction)
        );
        assert_eq!(
            resolve_profile_features(SetupProfile::Full),
            FeatureId::ALL.iter().copied().collect()
        );
    }

    #[test]
    fn required_components_aggregation() {
        let mut features = BTreeSet::new();
        features.insert(FeatureId::VideoExtraction);
        features.insert(FeatureId::TorrentSupport);
        let components = required_components_for_features(&features);
        assert!(components.contains(&ComponentId::Ytdlp));
        assert!(components.contains(&ComponentId::Rqbit));
        assert!(!components.contains(&ComponentId::Ffmpeg));
        assert!(!components.contains(&ComponentId::SevenZip));
    }

    #[test]
    fn cancellation_is_isolated_per_component() {
        let registry = ProvisioningCancellation::new();
        let ytdlp = registry.begin(ComponentId::Ytdlp).unwrap();
        let ffmpeg = registry.begin(ComponentId::Ffmpeg).unwrap();
        assert!(registry.cancel(ComponentId::Ytdlp));
        assert!(ytdlp.is_cancelled());
        assert!(!ffmpeg.is_cancelled());
        assert!(registry.begin(ComponentId::Ytdlp).is_err());
        registry.finish(ComponentId::Ytdlp);
        assert!(registry.begin(ComponentId::Ytdlp).is_ok());
    }

    #[tokio::test]
    async fn provisioning_limiter_caps_simultaneous_installations_at_two() {
        let registry = ProvisioningCancellation::new();
        let cancellation = CancellationToken::new();
        let first = registry.acquire(&cancellation).await.unwrap();
        let second = registry.acquire(&cancellation).await.unwrap();

        // A third simultaneous installation must queue, not proceed.
        let third = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            registry.acquire(&cancellation),
        )
        .await;
        assert!(
            third.is_err(),
            "a third concurrent install must block while two permits are held"
        );

        // Releasing one permit must unblock exactly one queued acquire.
        drop(first);
        let unblocked = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            registry.acquire(&cancellation),
        )
        .await;
        assert!(unblocked.is_ok(), "releasing a permit must unblock a queued install");
        drop(second);
        drop(unblocked);
    }

    #[tokio::test]
    async fn provisioning_limiter_acquire_is_cancellable_while_queued() {
        let registry = ProvisioningCancellation::new();
        let holder_cancellation = CancellationToken::new();
        let _first = registry.acquire(&holder_cancellation).await.unwrap();
        let _second = registry.acquire(&holder_cancellation).await.unwrap();

        let queued_cancellation = CancellationToken::new();
        queued_cancellation.cancel();
        assert!(matches!(
            registry.acquire(&queued_cancellation).await,
            Err(RavynError::Cancelled)
        ));
    }

    #[test]
    fn current_target_is_non_empty() {
        assert!(!current_target().is_empty());
    }

    #[test]
    fn built_in_manifest_provider_returns_manifest() {
        let provider = BuiltInManifestProvider::empty();
        let manifest = provider.load().unwrap();
        assert!(manifest.is_some());
        assert_eq!(manifest.unwrap().artifacts.len(), 0);
    }

    #[test]
    fn embedded_manifest_parses_validates_and_covers_every_windows_engine() {
        let provider = BuiltInManifestProvider::embedded().unwrap();
        let manifest = provider.load().unwrap().unwrap();
        let target = "x86_64-pc-windows-msvc";
        for engine in [
            ComponentId::Ytdlp.engine_name(),
            ComponentId::Rqbit.engine_name(),
            ComponentId::Ffmpeg.engine_name(),
        ] {
            assert!(
                manifest.artifact(engine, target).is_ok(),
                "expected an embedded {engine} artifact for {target}"
            );
        }
    }

    #[test]
    fn local_manifest_requires_a_valid_release_signature() {
        use ed25519_dalek::{Signer, SigningKey};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("manifest.json");
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let manifest = crate::services::engines::EngineManifest {
            schema_version: 1,
            channel: "stable".into(),
            artifacts: Vec::new(),
        };
        let signature = signing_key.sign(&serde_json::to_vec(&manifest).unwrap());
        std::fs::write(
            &path,
            serde_json::to_vec(&crate::services::engines::SignedEngineManifest {
                manifest,
                signature: hex::encode(signature.to_bytes()),
            })
            .unwrap(),
        )
        .unwrap();

        let provider = FileManifestProvider::new(
            path.clone(),
            Some(*signing_key.verifying_key().as_bytes()),
        );
        assert!(provider.load().unwrap().is_some());

        std::fs::write(&path, br#"{"schema_version":1,"channel":"stable","artifacts":[]}"#)
            .unwrap();
        assert!(provider.load().is_err());
    }

    #[test]
    fn version_comparison_orders_dotted_and_date_versions() {
        assert!(version_cmp("2025.10.1", "2025.9.30").is_gt());
        assert!(version_cmp("2025.01.01", "v2025.1.1").is_eq());
        assert!(version_cmp("2025.1.1", "2026.1.1").is_lt());
    }

    #[test]
    fn rqbit_health_normalizes_documented_id_parameters() {
        assert!(rqbit_endpoint_matches(
            "GET /torrents/{id_or_infohash}/stats/v1",
            "GET /torrents/{id}/stats/v1"
        ));
        assert!(!rqbit_endpoint_matches(
            "GET /torrents",
            "POST /torrents"
        ));
    }
}
