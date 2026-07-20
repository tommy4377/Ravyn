//! Component health checks, target detection, and feature-profile helpers.

use super::*;

pub(super) fn version_cmp(left: &str, right: &str) -> Ordering {
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
pub(super) async fn rqbit_api_health(config: &crate::config::Config) -> Result<()> {
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
        .timeout(std::time::Duration::from_secs(
            config.rqbit_timeout_secs.min(10),
        ))
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
        .ok_or_else(|| {
            RavynError::Unavailable("rqbit HTTP health response has no API map".into())
        })?;
    let missing = REQUIRED_ENDPOINTS
        .iter()
        .filter(|endpoint| {
            !apis
                .keys()
                .any(|actual| rqbit_endpoint_matches(actual, endpoint))
        })
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

pub(super) fn rqbit_endpoint_matches(actual: &str, required: &str) -> bool {
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
pub(super) async fn ffmpeg_capability_check(path: &Path) -> Result<()> {
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
pub(super) async fn ytdlp_capability_check(path: &Path) -> Result<()> {
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
pub(super) fn minimal_test_archive() -> Vec<u8> {
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
pub(super) async fn seven_zip_capability_check(path: &Path) -> Result<()> {
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

pub(super) fn component_config_path(component: ComponentId, config: &crate::config::Config) -> &PathBuf {
    match component {
        ComponentId::Ytdlp => &config.ytdlp,
        ComponentId::Ffmpeg => &config.ffmpeg,
        ComponentId::Rqbit => &config.rqbit,
        ComponentId::SevenZip => &config.seven_zip,
    }
}

pub(super) fn executable_resolves(path: &Path) -> bool {
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

