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

fn setup_lifecycle(
    completed: bool,
    features_selected: bool,
    library_prepared: bool,
    restart_required: bool,
) -> SetupLifecycleState {
    if restart_required {
        SetupLifecycleState::RestartRequired
    } else if completed {
        SetupLifecycleState::Completed
    } else if features_selected && library_prepared {
        SetupLifecycleState::ReadyToComplete
    } else if features_selected || library_prepared {
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
    let ready_to_complete = features_selected && library_prepared && !restart_required;
    let lifecycle = setup_lifecycle(
        completed,
        features_selected,
        library_prepared,
        restart_required,
    );

    Ok(Json(SetupStateResponse {
        completed,
        lifecycle,
        ready_to_complete,
        restart_required,
        completed_at: record.as_ref().and_then(|r| r.completed_at),
        completed_app_version: record.and_then(|r| r.app_version),
        app_version: env!("CARGO_PKG_VERSION"),
        platform: crate::services::components::current_target(),
        setup_profile: selections.as_ref().map(|(profile, _)| *profile),
        features_selected,
        library_root: library_root.map(|p| p.display().to_string()),
        library_prepared,
        data_dir: s.manager.config().data_dir.display().to_string(),
    }))
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

pub(super) async fn complete_setup(State(s): State<ApiState>) -> Result<Json<SetupStateResponse>> {
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
            "the backend must restart before setup can be completed with the selected library".into(),
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
            setup_lifecycle(false, true, true, true),
            SetupLifecycleState::RestartRequired
        );
        assert_eq!(
            setup_lifecycle(false, true, true, false),
            SetupLifecycleState::ReadyToComplete
        );
        assert_eq!(
            setup_lifecycle(true, true, true, false),
            SetupLifecycleState::Completed
        );
    }

    #[test]
    fn disk_space_is_reported_for_an_existing_directory() {
        let temporary = tempfile::tempdir().unwrap();
        assert!(available_disk_bytes(temporary.path()).is_some());
    }
}
