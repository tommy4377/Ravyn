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
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::{RavynError, Result};

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
    /// User provided a custom path; managed binaries are not used.
    CustomPath,
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
        Self::CustomPath,
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
            Self::CustomPath => "Custom Path",
        }
    }

    /// Whether the component can serve requests.
    pub fn is_operational(self) -> bool {
        matches!(self, Self::Installed | Self::CustomPath)
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
    /// Standard downloads + yt-dlp for video extraction.
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
            Self::Minimal => BTreeSet::new(),
            Self::Recommended => {
                let mut set = BTreeSet::new();
                set.insert(FeatureId::StandardDownloads);
                set.insert(FeatureId::VideoExtraction);
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
    pub managed_path: Option<PathBuf>,
    pub custom_path: Option<PathBuf>,
    pub effective_path: Option<PathBuf>,
    pub error_message: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
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

/// Shared, resettable cancellation for background provisioning.
///
/// A raw `CancellationToken` is one-way: once cancelled it would abort every
/// future installation as well. This wrapper hands out the current token and
/// replaces it with a fresh one after each cancellation so retries work.
#[derive(Clone)]
pub struct ProvisioningCancellation {
    inner: Arc<std::sync::Mutex<CancellationToken>>,
}

impl Default for ProvisioningCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvisioningCancellation {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(CancellationToken::new())),
        }
    }

    /// The token active installations should observe.
    pub fn current(&self) -> CancellationToken {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Cancel all active installations and arm a fresh token for retries.
    pub fn cancel_and_reset(&self) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.cancel();
        *guard = CancellationToken::new();
    }
}

// ---------------------------------------------------------------------------
// Manifest provider abstraction
// ---------------------------------------------------------------------------

/// Abstraction over manifest sources (built-in, remote, hybrid).
///
/// The trait is object-safe so a `Box<dyn ManifestProvider>` can be held at
/// runtime.  All methods are synchronous because manifest loading should be
/// fast (built-in) or cached (remote).
pub trait ManifestProvider: Send + Sync {
    /// Load the manifest for the current platform.
    ///
    /// Returns `None` if no manifest is available (e.g. remote fetch failed
    /// and no fallback exists).
    fn load(&self) -> Result<Option<crate::services::engines::EngineManifest>>;

    /// Human-readable name of this provider (for logging).
    fn name(&self) -> &'static str;
}

/// Built-in manifest compiled into the binary.
///
/// The manifest is empty until real artifact data is populated for each
/// release.  The structure is validated at compile-time via tests.
pub struct BuiltInManifestProvider {
    manifest: crate::services::engines::EngineManifest,
}

impl BuiltInManifestProvider {
    /// Create a provider from a pre-built manifest.
    pub fn new(manifest: crate::services::engines::EngineManifest) -> Self {
        Self { manifest }
    }

    /// Empty manifest placeholder – used when no artifact data is available yet.
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
        data_dir: &std::path::Path,
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

    /// Reference to the underlying engine manager.
    pub fn engine_manager(&self) -> &crate::services::engines::EngineManager {
        &self.engine_manager
    }

    /// Current platform target triple.
    pub fn target(&self) -> &'static str {
        self.target
    }

    /// Load the manifest from the active provider.
    pub fn load_manifest(&self) -> Result<Option<crate::services::engines::EngineManifest>> {
        self.manifest_provider.load()
    }

    /// Determine the component state given the current config and persisted
    /// component records.
    pub async fn component_state(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
        records: &BTreeMap<ComponentId, PersistedComponent>,
    ) -> ComponentState {
        let default = std::path::Path::new(component.engine_name());
        let config_path = match component {
            ComponentId::Ytdlp => &config.ytdlp,
            ComponentId::Ffmpeg => &config.ffmpeg,
            ComponentId::Rqbit => &config.rqbit,
            ComponentId::SevenZip => &config.seven_zip,
        };

        // User provided an explicit, non-default path → custom path.
        if config_path != default {
            return ComponentState::CustomPath;
        }

        // Check the persisted record first.
        if let Some(record) = records.get(&component) {
            match record.state {
                ComponentState::Installed => {
                    // Verify the binary still exists and checksum is valid.
                    if let Some(ref path) = record.managed_path {
                        if path.exists() {
                            return ComponentState::Installed;
                        }
                    }
                    // Binary missing – mark as not installed.
                    return ComponentState::NotInstalled;
                }
                ComponentState::Failed => return ComponentState::Failed,
                ComponentState::Unsupported => return ComponentState::Unsupported,
                _ => {}
            }
        }

        // Check the engine manager for an active binary.
        if let Ok(Some(path)) = self
            .engine_manager
            .active_path(component.engine_name())
            .await
        {
            if path.exists() {
                return ComponentState::Installed;
            }
        }

        ComponentState::NotInstalled
    }

    /// Compute the effective executable path for a component.
    pub async fn effective_path(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
        records: &BTreeMap<ComponentId, PersistedComponent>,
    ) -> Option<PathBuf> {
        let default = std::path::Path::new(component.engine_name());
        let config_path = match component {
            ComponentId::Ytdlp => &config.ytdlp,
            ComponentId::Ffmpeg => &config.ffmpeg,
            ComponentId::Rqbit => &config.rqbit,
            ComponentId::SevenZip => &config.seven_zip,
        };

        if config_path != default {
            return Some(config_path.clone());
        }

        if let Some(record) = records.get(&component) {
            if let Some(ref path) = record.managed_path {
                if path.exists() {
                    return Some(path.clone());
                }
            }
        }

        self.engine_manager
            .active_path(component.engine_name())
            .await
            .ok()
            .flatten()
    }

    /// Check whether a feature's required components are all operational.
    pub async fn feature_satisfied(
        &self,
        feature: FeatureId,
        enabled: bool,
        config: &crate::config::Config,
        records: &BTreeMap<ComponentId, PersistedComponent>,
    ) -> bool {
        if !enabled {
            return true; // Disabled features are always "satisfied".
        }
        let manager = self;
        for &component in feature.required_components() {
            if !manager
                .component_state(component, config, records)
                .await
                .is_operational()
            {
                return false;
            }
        }
        true
    }

    /// Install a single component in the background.
    ///
    /// Returns immediately; the caller should emit progress events via the
    /// event bus and update the persisted state on completion.
    pub async fn install_component(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
    ) -> Result<PathBuf> {
        self.install_component_with_progress(component, config, None)
            .await
    }

    /// Install a single component, reporting download progress as
    /// `(bytes_downloaded, bytes_total)` through the optional callback.
    pub async fn install_component_with_progress(
        &self,
        component: ComponentId,
        config: &crate::config::Config,
        progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    ) -> Result<PathBuf> {
        let manifest = self
            .manifest_provider
            .load()?
            .ok_or_else(|| RavynError::Unavailable("no manifest available".into()))?;

        let artifact = manifest.artifact(component.engine_name(), self.target)?;

        self.engine_manager
            .download_and_install(config, artifact, &self.cancellation, progress)
            .await
    }

    /// Roll back a component to its previous version.
    pub async fn rollback_component(&self, component: ComponentId) -> Result<PathBuf> {
        self.engine_manager.rollback(component.engine_name()).await
    }

    /// Remove a managed component and its directory.
    pub async fn remove_component(&self, component: ComponentId) -> Result<()> {
        let engine_dir = self.engine_manager.root_dir().join(component.engine_name());
        if tokio::fs::try_exists(&engine_dir).await? {
            tokio::fs::remove_dir_all(&engine_dir).await?;
        }
        Ok(())
    }
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
    pub managed_path: Option<PathBuf>,
    pub custom_path: Option<PathBuf>,
    pub error_message: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
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
    match profile {
        SetupProfile::Custom => features_from_stored_json(features_json),
        _ => Ok(resolve_profile_features(profile)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!ComponentState::NotInstalled.is_operational());
        assert!(!ComponentState::Failed.is_operational());

        assert!(ComponentState::Queued.is_busy());
        assert!(ComponentState::Downloading.is_busy());
        assert!(ComponentState::Verifying.is_busy());
        assert!(ComponentState::Installing.is_busy());
        assert!(!ComponentState::Installed.is_busy());
        assert!(!ComponentState::Failed.is_busy());
    }

    #[test]
    fn setup_profile_features() {
        assert!(resolve_profile_features(SetupProfile::Minimal).is_empty());
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
}
