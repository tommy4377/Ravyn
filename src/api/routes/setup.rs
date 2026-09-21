//! Setup lifecycle API routes.
//!
//! Backs the custom Ravyn setup: first-run detection, Ravyn library
//! preparation, and deterministic setup completion.

use super::*;

use crate::services::{
    components::SetupProfile,
    library::{LIBRARY_DIRECTORIES, prepare_library_layout},
};

/// Longest accepted library path, in UTF-8 bytes.
const MAX_LIBRARY_PATH_BYTES: usize = 4096;

/// Setup mutations are intentionally one-way. Once the lifecycle is committed,
/// repair and update flows must use dedicated APIs instead of silently
/// reopening first-run privileges.
pub(super) async fn ensure_setup_mutable(s: &ApiState) -> Result<()> {
    if s.repository
        .load_setup_state()
        .await?
        .is_some_and(|state| state.completed)
    {
        return Err(crate::error::RavynError::Conflict(
            "setup has already been completed".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SetupLifecycleState {
    NotStarted,
    InProgress,
    RestartRequired,
    ReadyToComplete,
    Completed,
}

#[derive(Serialize)]
pub(super) struct SetupStateResponse {
    completed: bool,
    lifecycle: SetupLifecycleState,
    ready_to_complete: bool,
    restart_required: bool,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_app_version: Option<String>,
    app_version: &'static str,
    platform: &'static str,
    setup_profile: Option<SetupProfile>,
    features_selected: bool,
    library_root: Option<String>,
    library_prepared: bool,
    data_dir: String,
    installation: Option<InstallationResponse>,
    integration_consent: Option<IntegrationConsentResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct IntegrationConsentResponse {
    id: uuid::Uuid,
    installation_mode: String,
    install_application: bool,
    register_installed_app: bool,
    start_menu_shortcut: bool,
    desktop_shortcut: bool,
    launch_at_startup: bool,
    launch_after_setup: bool,
    consented_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::storage::IntegrationConsentRecord> for IntegrationConsentResponse {
    fn from(record: crate::storage::IntegrationConsentRecord) -> Self {
        Self {
            id: record.id,
            installation_mode: record.installation_mode,
            install_application: record.install_application,
            register_installed_app: record.register_installed_app,
            start_menu_shortcut: record.start_menu_shortcut,
            desktop_shortcut: record.desktop_shortcut,
            launch_at_startup: record.launch_at_startup,
            launch_after_setup: record.launch_after_setup,
            consented_at: record.consented_at,
        }
    }
}

/// Result of the desktop shell's Windows installation/integration step, as
/// last reported via `POST /v1/setup/installation`.
#[derive(Serialize)]
pub(super) struct InstallationResponse {
    installation_mode: String,
    installed_exe: Option<String>,
    installed_version: Option<String>,
    installed_sha256: Option<String>,
    integration_completed: bool,
    integration_errors: Vec<String>,
    relaunch_pending: bool,
}

impl From<crate::storage::InstallationRecord> for InstallationResponse {
    fn from(record: crate::storage::InstallationRecord) -> Self {
        Self {
            installation_mode: record.installation_mode,
            installed_exe: record.installed_exe,
            installed_version: record.installed_version,
            installed_sha256: record.installed_sha256,
            integration_completed: record.integration_completed,
            integration_errors: record.integration_errors,
            relaunch_pending: record.relaunch_pending,
        }
    }
}

/// The library root the backend will use after the next restart: persisted
/// settings win over the process configuration.
async fn pending_library_root(s: &ApiState) -> Result<Option<std::path::PathBuf>> {
    if let Some(settings) = s.repository.load_persistent_settings().await? {
        if settings.library_root.is_some() {
            return Ok(settings.library_root);
        }
    }
    Ok(s.manager.config().effective_library_root())
}

fn library_layout_prepared(root: &std::path::Path) -> bool {
    root.is_dir()
        && LIBRARY_DIRECTORIES
            .iter()
            .all(|directory| root.join(directory).is_dir())
}

fn paths_equivalent(left: &std::path::Path, right: &std::path::Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn expected_application_path(installation_mode: &str) -> Result<std::path::PathBuf> {
    match installation_mode {
        "portable" | "development" => std::env::current_exe().map_err(Into::into),
        "installed" => {
            #[cfg(windows)]
            {
                let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
                    crate::error::RavynError::Unavailable(
                        "LOCALAPPDATA is unavailable, so the installed application cannot be verified"
                            .into(),
                    )
                })?;
                Ok(std::path::PathBuf::from(local_app_data)
                    .join("Ravyn")
                    .join("Ravyn.exe"))
            }
            #[cfg(not(windows))]
            {
                Err(crate::error::RavynError::Invalid(
                    "installed mode can only be verified on Windows".into(),
                ))
            }
        }
        _ => Err(crate::error::RavynError::Invalid(
            "unknown installation mode".into(),
        )),
    }
}

fn consent_matches_report(
    consent: &crate::storage::IntegrationConsentRecord,
    request: &ReportInstallationRequest,
) -> bool {
    consent.installation_mode == request.installation_mode
        && request.relaunch_pending
            == (request.installation_mode == "installed" && consent.launch_after_setup)
}

async fn verify_completed_installation(
    request: &ReportInstallationRequest,
    installed_exe: &str,
    installed_version: &str,
    installed_sha256: &str,
) -> Result<String> {
    if installed_version != env!("CARGO_PKG_VERSION") {
        return Err(crate::error::RavynError::Conflict(format!(
            "the reported application version {installed_version} does not match the running setup version {}",
            env!("CARGO_PKG_VERSION")
        )));
    }

    crate::services::checksum::validate_sha256(installed_sha256)?;
    let path = std::path::PathBuf::from(installed_exe);
    if !path.is_absolute() || !path.is_file() {
        return Err(crate::error::RavynError::Invalid(
            "installed_exe must be an absolute path to an existing file".into(),
        ));
    }
    let expected = expected_application_path(&request.installation_mode)?;
    if !paths_equivalent(&path, &expected) {
        return Err(crate::error::RavynError::Conflict(format!(
            "the reported executable does not match the trusted {} application path",
            request.installation_mode
        )));
    }

    let actual =
        crate::services::checksum::sha256(&path, &tokio_util::sync::CancellationToken::new())
            .await?;
    if !actual.eq_ignore_ascii_case(installed_sha256) {
        return Err(crate::error::RavynError::Conflict(
            "the reported executable checksum does not match the file on disk".into(),
        ));
    }
    Ok(actual)
}

fn installation_ready(installation: Option<&crate::storage::InstallationRecord>) -> bool {
    let Some(installation) = installation else {
        return false;
    };
    if !installation.integration_completed {
        return false;
    }
    let executable_ready = installation
        .installed_exe
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .is_some_and(|path| std::path::Path::new(path).is_file());
    let version_ready = installation
        .installed_version
        .as_deref()
        .is_some_and(|version| !version.trim().is_empty());
    let checksum_ready = installation
        .installed_sha256
        .as_deref()
        .is_some_and(|sha256| {
            sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        });

    INSTALLATION_MODES.contains(&installation.installation_mode.as_str())
        && executable_ready
        && version_ready
        && checksum_ready
}

fn setup_lifecycle(
    completed: bool,
    features_selected: bool,
    library_prepared: bool,
    installation_ready: bool,
    integration_consent_saved: bool,
    restart_required: bool,
) -> SetupLifecycleState {
    if restart_required {
        SetupLifecycleState::RestartRequired
    } else if completed {
        SetupLifecycleState::Completed
    } else if features_selected
        && library_prepared
        && installation_ready
        && integration_consent_saved
    {
        SetupLifecycleState::ReadyToComplete
    } else if features_selected
        || library_prepared
        || installation_ready
        || integration_consent_saved
    {
        SetupLifecycleState::InProgress
    } else {
        SetupLifecycleState::NotStarted
    }
}

pub(super) async fn get_setup_state(State(s): State<ApiState>) -> Result<Json<SetupStateResponse>> {
    let record = s.repository.load_setup_state().await?;
    let selections = s.repository.load_feature_selections().await?;
    let library_root = pending_library_root(&s).await?;
    let library_prepared = library_root.as_deref().is_some_and(library_layout_prepared);
    let runtime_library_root = s.manager.config().effective_library_root();
    let restart_required = match (library_root.as_deref(), runtime_library_root.as_deref()) {
        (Some(pending), Some(runtime)) => !paths_equivalent(pending, runtime),
        (None, None) => false,
        _ => true,
    };
    let completed = record.as_ref().is_some_and(|r| r.completed);
    let features_selected = selections.is_some();
    let installation_is_ready = installation_ready(
        record
            .as_ref()
            .and_then(|state| state.installation.as_ref()),
    );
    let integration_consent_saved = record
        .as_ref()
        .and_then(|state| state.integration_consent.as_ref())
        .is_some();
    let ready_to_complete = features_selected
        && library_prepared
        && installation_is_ready
        && integration_consent_saved
        && !restart_required;
    let lifecycle = setup_lifecycle(
        completed,
        features_selected,
        library_prepared,
        installation_is_ready,
        integration_consent_saved,
        restart_required,
    );

    Ok(Json(SetupStateResponse {
        completed,
        lifecycle,
        ready_to_complete,
        restart_required,
        completed_at: record.as_ref().and_then(|r| r.completed_at),
        completed_app_version: record.as_ref().and_then(|r| r.app_version.clone()),
        app_version: env!("CARGO_PKG_VERSION"),
        platform: crate::services::components::current_target(),
        setup_profile: selections.as_ref().map(|(profile, _)| *profile),
        features_selected,
        library_root: library_root.map(|p| p.display().to_string()),
        library_prepared,
        data_dir: s.manager.config().data_dir.display().to_string(),
        installation: record
            .as_ref()
            .and_then(|r| r.installation.clone())
            .map(Into::into),
        integration_consent: record.and_then(|r| r.integration_consent).map(Into::into),
    }))
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct SaveIntegrationConsentRequest {
    installation_mode: String,
    install_application: bool,
    register_installed_app: bool,
    start_menu_shortcut: bool,
    desktop_shortcut: bool,
    launch_at_startup: bool,
    launch_after_setup: bool,
}

/// Persist the exact native integration choices before the setup window is
/// allowed to perform Windows side effects.
pub(super) async fn save_integration_consent(
    State(s): State<ApiState>,
    Json(request): Json<SaveIntegrationConsentRequest>,
) -> Result<Json<SetupStateResponse>> {
    ensure_setup_mutable(&s).await?;
    if !INSTALLATION_MODES.contains(&request.installation_mode.as_str()) {
        return Err(crate::error::RavynError::Invalid(format!(
            "installation_mode must be one of {INSTALLATION_MODES:?}"
        )));
    }
    if request.installation_mode != "installed"
        && (request.install_application
            || request.register_installed_app
            || request.start_menu_shortcut
            || request.desktop_shortcut
            || request.launch_at_startup)
    {
        return Err(crate::error::RavynError::Invalid(
            "portable and development modes cannot request Windows installation integration".into(),
        ));
    }
    let consent = crate::storage::IntegrationConsentRecord {
        id: uuid::Uuid::new_v4(),
        installation_mode: request.installation_mode,
        install_application: request.install_application,
        register_installed_app: request.register_installed_app,
        start_menu_shortcut: request.start_menu_shortcut,
        desktop_shortcut: request.desktop_shortcut,
        launch_at_startup: request.launch_at_startup,
        launch_after_setup: request.launch_after_setup,
        consented_at: chrono::Utc::now(),
    };
    let result = s.repository.save_integration_consent(consent).await;
    audited(
        &s.repository,
        "setup.save_integration_consent",
        "setup",
        None,
        result,
    )
    .await?;
    get_setup_state(State(s)).await
}

#[derive(Deserialize)]
pub(super) struct PrepareLibraryRequest {
    path: String,
}

#[derive(Serialize)]
pub(super) struct PrepareLibraryResponse {
    path: String,
    existed: bool,
    directories: Vec<&'static str>,
    available_bytes: Option<u64>,
    restart_required: bool,
}

pub(super) async fn prepare_library(
    State(s): State<ApiState>,
    Json(request): Json<PrepareLibraryRequest>,
) -> Result<Json<PrepareLibraryResponse>> {
    ensure_setup_mutable(&s).await?;
    let result: Result<PrepareLibraryResponse> = async {
        let trimmed = request.path.trim();
        if trimmed.is_empty() {
            return Err(crate::error::RavynError::Invalid(
                "library path must not be empty".into(),
            ));
        }
        if trimmed.len() > MAX_LIBRARY_PATH_BYTES {
            return Err(crate::error::RavynError::Invalid(
                "library path is too long".into(),
            ));
        }
        let root = std::path::PathBuf::from(trimmed);
        if !root.is_absolute() {
            return Err(crate::error::RavynError::Invalid(
                "library path must be absolute".into(),
            ));
        }
        if root.is_file() {
            return Err(crate::error::RavynError::Invalid(
                "library path points to an existing file".into(),
            ));
        }

        let existed = root.is_dir();
        prepare_library_layout(&root).await?;
        let runtime_library_root = s.manager.config().effective_library_root();
        let restart_required = runtime_library_root
            .as_deref()
            .is_none_or(|runtime| !paths_equivalent(&root, runtime));

        // Persist the chosen root so the backend adopts it on next start.
        let mut values = s
            .repository
            .load_persistent_settings()
            .await?
            .unwrap_or_else(|| PersistentSettings::from_config(&s.manager.config()));
        values.library_root = Some(root.clone());
        let mut candidate = (*s.manager.config()).clone();
        values.apply_to(&mut candidate)?;
        s.repository.save_persistent_settings(&values).await?;

        Ok(PrepareLibraryResponse {
            path: root.display().to_string(),
            existed,
            directories: LIBRARY_DIRECTORIES.to_vec(),
            available_bytes: available_disk_bytes(&root),
            restart_required,
        })
    }
    .await;

    audited(
        &s.repository,
        "setup.prepare_library",
        "setup",
        Some(&request.path),
        result,
    )
    .await
    .map(Json)
}

/// Longest accepted value for a single free-text installation field.
const MAX_INSTALLATION_FIELD_BYTES: usize = 4096;
/// Longest accepted single integration error message.
const MAX_INTEGRATION_ERROR_BYTES: usize = 512;
/// Most integration error messages accepted per report.
const MAX_INTEGRATION_ERRORS: usize = 32;

const INSTALLATION_MODES: &[&str] = &["installed", "portable", "development"];

#[derive(Deserialize)]
pub(super) struct ReportInstallationRequest {
    installation_mode: String,
    installed_exe: Option<String>,
    installed_version: Option<String>,
    installed_sha256: Option<String>,
    integration_completed: bool,
    #[serde(default)]
    integration_errors: Vec<String>,
    relaunch_pending: bool,
}

fn bounded_field(value: Option<String>) -> Result<Option<String>> {
    match value {
        Some(value) if value.len() > MAX_INSTALLATION_FIELD_BYTES => Err(
            crate::error::RavynError::Invalid("installation field is too long".into()),
        ),
        other => Ok(other),
    }
}

/// Persists the desktop shell's Windows installation/integration result so
/// the backend knows the outcome, not just the frontend. Idempotent: the
/// setup UI may call this again after retrying a failed step.
pub(super) async fn report_installation(
    State(s): State<ApiState>,
    Json(request): Json<ReportInstallationRequest>,
) -> Result<Json<SetupStateResponse>> {
    ensure_setup_mutable(&s).await?;
    if !INSTALLATION_MODES.contains(&request.installation_mode.as_str()) {
        return Err(crate::error::RavynError::Invalid(format!(
            "installation_mode must be one of {INSTALLATION_MODES:?}"
        )));
    }
    if request.integration_errors.len() > MAX_INTEGRATION_ERRORS
        || request
            .integration_errors
            .iter()
            .any(|message| message.len() > MAX_INTEGRATION_ERROR_BYTES)
    {
        return Err(crate::error::RavynError::Invalid(
            "too many or too long integration error messages".into(),
        ));
    }
    let consent = s
        .repository
        .load_setup_state()
        .await?
        .and_then(|record| record.integration_consent)
        .ok_or_else(|| {
            crate::error::RavynError::Conflict(
                "installation preferences must be consented before reporting the result".into(),
            )
        })?;
    if !consent_matches_report(&consent, &request) {
        return Err(crate::error::RavynError::Conflict(
            "the installation report does not match the persisted setup consent".into(),
        ));
    }

    let installed_exe = bounded_field(request.installed_exe.clone())?;
    let installed_version = bounded_field(request.installed_version.clone())?;
    let submitted_sha256 = bounded_field(request.installed_sha256.clone())?;
    let installed_sha256 = if request.integration_completed {
        let executable = installed_exe.as_deref().ok_or_else(|| {
            crate::error::RavynError::Invalid(
                "a completed integration report requires installed_exe".into(),
            )
        })?;
        let version = installed_version.as_deref().ok_or_else(|| {
            crate::error::RavynError::Invalid(
                "a completed integration report requires installed_version".into(),
            )
        })?;
        let checksum = submitted_sha256.as_deref().ok_or_else(|| {
            crate::error::RavynError::Invalid(
                "a completed integration report requires installed_sha256".into(),
            )
        })?;
        Some(verify_completed_installation(&request, executable, version, checksum).await?)
    } else {
        if let Some(checksum) = submitted_sha256.as_deref() {
            crate::services::checksum::validate_sha256(checksum)?;
        }
        submitted_sha256.map(|checksum| checksum.to_ascii_lowercase())
    };
    let installation = crate::storage::InstallationRecord {
        installation_mode: request.installation_mode,
        installed_exe,
        installed_version,
        installed_sha256,
        integration_completed: request.integration_completed,
        integration_errors: request.integration_errors,
        relaunch_pending: request.relaunch_pending,
    };
    let result = s.repository.save_installation_report(&installation).await;
    audited(
        &s.repository,
        "setup.report_installation",
        "setup",
        Some(&installation.installation_mode),
        result,
    )
    .await?;
    get_setup_state(State(s)).await
}

pub(super) async fn complete_setup(State(s): State<ApiState>) -> Result<Json<SetupStateResponse>> {
    ensure_setup_mutable(&s).await?;
    let setup_record = s.repository.load_setup_state().await?;
    if setup_record
        .as_ref()
        .and_then(|record| record.integration_consent.as_ref())
        .is_none()
    {
        return Err(crate::error::RavynError::Conflict(
            "installation preferences must be consented before setup can be completed".into(),
        ));
    }
    if !installation_ready(
        setup_record
            .as_ref()
            .and_then(|record| record.installation.as_ref()),
    ) {
        return Err(crate::error::RavynError::Conflict(
            "a verified installed, portable, or development application must be reported before setup can be completed".into(),
        ));
    }
    let selections = s.repository.load_feature_selections().await?;
    if selections.is_none() {
        return Err(crate::error::RavynError::Conflict(
            "feature selections must be saved before setup can be completed".into(),
        ));
    }
    let library_root = pending_library_root(&s).await?;
    let Some(library_root) = library_root else {
        return Err(crate::error::RavynError::Conflict(
            "a Ravyn library must be prepared before setup can be completed".into(),
        ));
    };
    if !library_layout_prepared(&library_root) {
        return Err(crate::error::RavynError::Conflict(
            "the Ravyn library layout is incomplete".into(),
        ));
    }
    let runtime_library_root = s.manager.config().effective_library_root();
    if runtime_library_root
        .as_deref()
        .is_none_or(|runtime| !paths_equivalent(&library_root, runtime))
    {
        return Err(crate::error::RavynError::Conflict(
            "the backend must restart before setup can be completed with the selected library"
                .into(),
        ));
    }
    let library_root_str = library_root.display().to_string();
    let result = s
        .repository
        .save_setup_complete(env!("CARGO_PKG_VERSION"), Some(&library_root_str))
        .await;
    audited(&s.repository, "setup.complete", "setup", None, result).await?;
    get_setup_state(State(s)).await
}

/// Free disk space available to the current user at `path`, when resolvable.
#[cfg(windows)]
fn available_disk_bytes(path: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free: u64 = 0;
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (result != 0).then_some(free)
}

/// Free disk space available to the current user at `path`, when resolvable.
#[cfg(unix)]
fn available_disk_bytes(path: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let cstr = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(cstr.as_ptr(), &mut stat) };
    (result == 0).then(|| u64::from(stat.f_bavail).saturating_mul(u64::from(stat.f_frsize)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_layout_detection_requires_all_directories() {
        let temporary = tempfile::tempdir().unwrap();
        assert!(!library_layout_prepared(temporary.path()));
    }

    #[tokio::test]
    async fn prepared_layout_is_detected() {
        let temporary = tempfile::tempdir().unwrap();
        prepare_library_layout(temporary.path()).await.unwrap();
        assert!(library_layout_prepared(temporary.path()));
    }

    #[test]
    fn lifecycle_requires_restart_before_completion() {
        assert_eq!(
            setup_lifecycle(false, true, true, true, true, true),
            SetupLifecycleState::RestartRequired
        );
        assert_eq!(
            setup_lifecycle(false, true, true, true, true, false),
            SetupLifecycleState::ReadyToComplete
        );
        assert_eq!(
            setup_lifecycle(true, true, true, true, true, false),
            SetupLifecycleState::Completed
        );
    }

    #[test]
    fn lifecycle_stays_in_progress_until_installation_is_verified() {
        assert_eq!(
            setup_lifecycle(false, true, true, false, true, false),
            SetupLifecycleState::InProgress
        );
        assert_eq!(
            setup_lifecycle(false, true, true, true, false, false),
            SetupLifecycleState::InProgress
        );
    }

    #[test]
    fn installation_readiness_requires_a_real_executable_and_checksum() {
        let temporary = tempfile::tempdir().unwrap();
        let executable = temporary.path().join("Ravyn.exe");
        std::fs::write(&executable, b"ravyn").unwrap();
        let installation = crate::storage::InstallationRecord {
            installation_mode: "portable".into(),
            installed_exe: Some(executable.display().to_string()),
            installed_version: Some("0.2.0".into()),
            installed_sha256: Some("a".repeat(64)),
            integration_completed: true,
            integration_errors: Vec::new(),
            relaunch_pending: false,
        };
        assert!(installation_ready(Some(&installation)));
        assert!(!installation_ready(Some(
            &crate::storage::InstallationRecord {
                integration_completed: false,
                ..installation
            }
        )));
    }

    #[test]
    fn disk_space_is_reported_for_an_existing_directory() {
        let temporary = tempfile::tempdir().unwrap();
        assert!(available_disk_bytes(temporary.path()).is_some());
    }

    #[test]
    fn bounded_field_rejects_oversized_values_but_passes_through_short_ones() {
        assert_eq!(bounded_field(None).unwrap(), None);
        assert_eq!(
            bounded_field(Some("Ravyn.exe".into())).unwrap(),
            Some("Ravyn.exe".into())
        );
        let oversized = "x".repeat(MAX_INSTALLATION_FIELD_BYTES + 1);
        assert!(bounded_field(Some(oversized)).is_err());
    }

    #[test]
    fn installation_modes_are_exactly_the_three_detected_by_the_desktop_shell() {
        assert_eq!(INSTALLATION_MODES, ["installed", "portable", "development"]);
    }

    #[test]
    fn installation_report_must_match_the_persisted_mode_and_relaunch_choice() {
        let consent = crate::storage::IntegrationConsentRecord {
            id: uuid::Uuid::new_v4(),
            installation_mode: "installed".into(),
            install_application: true,
            register_installed_app: true,
            start_menu_shortcut: true,
            desktop_shortcut: false,
            launch_at_startup: false,
            launch_after_setup: true,
            consented_at: chrono::Utc::now(),
        };
        let matching = ReportInstallationRequest {
            installation_mode: "installed".into(),
            installed_exe: None,
            installed_version: None,
            installed_sha256: None,
            integration_completed: false,
            integration_errors: Vec::new(),
            relaunch_pending: true,
        };
        assert!(consent_matches_report(&consent, &matching));
        assert!(!consent_matches_report(
            &consent,
            &ReportInstallationRequest {
                installation_mode: "portable".into(),
                relaunch_pending: false,
                ..matching
            }
        ));
    }

    #[tokio::test]
    async fn portable_installation_verification_hashes_the_actual_running_file() {
        let executable = std::env::current_exe().unwrap();
        let checksum = crate::services::checksum::sha256(
            &executable,
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
        let request = ReportInstallationRequest {
            installation_mode: "portable".into(),
            installed_exe: Some(executable.display().to_string()),
            installed_version: Some(env!("CARGO_PKG_VERSION").into()),
            installed_sha256: Some(checksum.clone()),
            integration_completed: true,
            integration_errors: Vec::new(),
            relaunch_pending: false,
        };
        assert_eq!(
            verify_completed_installation(
                &request,
                request.installed_exe.as_deref().unwrap(),
                request.installed_version.as_deref().unwrap(),
                request.installed_sha256.as_deref().unwrap(),
            )
            .await
            .unwrap(),
            checksum
        );
    }
}
