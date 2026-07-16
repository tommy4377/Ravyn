//! Silent, signed application updates for installed Windows builds.
//!
//! Ravyn downloads and verifies an installer in the background, then starts a
//! detached helper when the main window closes. The helper waits for Ravyn to
//! exit, runs the current-user NSIS installer silently, and relaunches the app.

use std::{
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use ravyn::services::app_updates::{AppUpdateManifest, SignedAppUpdateManifest};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

const METADATA_LIMIT: u64 = 512 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const AUTOMATIC_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const AUTOMATIC_RETRY_BASE: Duration = Duration::from_secs(15 * 60);
const AUTOMATIC_RETRY_MAX: Duration = Duration::from_secs(2 * 60 * 60);
const UPDATE_CANCELLED_ERROR: &str = "application update cancelled";
const UPDATE_FILENAME: &str = "ravyn-pending-update.exe";
const UPDATE_PENDING_STATE_FILENAME: &str = "ravyn-pending-update.json";
const UPDATE_TRANSACTION_FILENAME: &str = "ravyn-update-transaction.json";
const UPDATE_RESULT_FILENAME: &str = "ravyn-update-result.json";
const UPDATE_JOURNAL_FILENAME: &str = "ravyn-update-journal.txt";
/// Schema-2 single-file backup, still removed during cleanup of old state.
const LEGACY_UPDATE_BACKUP_FILENAME: &str = ".ravyn.update.previous.exe";
const UPDATE_PENDING_STATE_SCHEMA: u32 = 2;
const UPDATE_TRANSACTION_SCHEMA: u32 = 3;
const READINESS_TIMEOUT_SECS: u64 = 180;
const PENDING_UPDATE_MAX_AGE_SECS: u64 = 14 * 24 * 60 * 60;
/// A transaction older than this without a surviving helper is interrupted:
/// close + installer run + readiness wait all complete well inside it.
const INTERRUPTED_TRANSACTION_STALE_MS: u64 = 30 * 60 * 1000;
const REGISTRY_UNINSTALL_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Ravyn";
const REGISTRY_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdatePhase {
    Disabled,
    Idle,
    Checking,
    UpToDate,
    Downloading,
    Cancelling,
    Cancelled,
    Ready,
    Installing,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppUpdateResult {
    pub outcome: String,
    pub from_version: String,
    pub to_version: String,
    pub completed_at_unix_ms: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateStatus {
    pub configured: bool,
    pub automatic: bool,
    pub phase: AppUpdatePhase,
    pub current_version: String,
    pub available_version: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub notes: Option<String>,
    pub last_error: Option<String>,
    pub install_on_exit: bool,
    pub repair_mode: bool,
    pub last_result: Option<AppUpdateResult>,
    pub last_checked_at_unix_ms: Option<u64>,
    pub next_check_at_unix_ms: Option<u64>,
    pub automatic_check_interval_secs: Option<u64>,
}

impl AppUpdateStatus {
    fn disabled(reason: impl Into<String>) -> Self {
        Self {
            configured: false,
            automatic: false,
            phase: AppUpdatePhase::Disabled,
            current_version: env!("CARGO_PKG_VERSION").into(),
            available_version: None,
            downloaded_bytes: 0,
            total_bytes: None,
            notes: None,
            last_error: Some(reason.into()),
            install_on_exit: false,
            repair_mode: false,
            last_result: None,
            last_checked_at_unix_ms: None,
            next_check_at_unix_ms: None,
            automatic_check_interval_secs: None,
        }
    }

    fn idle() -> Self {
        Self {
            configured: true,
            automatic: false,
            phase: AppUpdatePhase::Idle,
            current_version: env!("CARGO_PKG_VERSION").into(),
            available_version: None,
            downloaded_bytes: 0,
            total_bytes: None,
            notes: None,
            last_error: None,
            install_on_exit: false,
            repair_mode: false,
            last_result: None,
            last_checked_at_unix_ms: None,
            next_check_at_unix_ms: None,
            automatic_check_interval_secs: Some(AUTOMATIC_CHECK_INTERVAL.as_secs()),
        }
    }
}

struct PendingUpdate {
    manifest: AppUpdateManifest,
    installer_path: PathBuf,
    repair: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPendingUpdate {
    schema: u32,
    signed_manifest: SignedAppUpdateManifest,
    staged_at_unix_ms: u64,
    #[serde(default)]
    repair: bool,
}

/// Versioned journal of a close-time update. Schema 3 covers the whole
/// installation: directory snapshot, registry keys, and shortcuts, plus a
/// phase journal the helper advances so an interrupted run can be diagnosed.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingUpdateTransaction {
    schema: u32,
    token: String,
    from_version: String,
    to_version: String,
    #[serde(default)]
    install_dir: PathBuf,
    installed_exe: PathBuf,
    /// Pre-update snapshot of the application binaries (top-level `.exe` and
    /// `.dll` files). The install directory doubles as the backend data
    /// directory, so backup and rollback deliberately never touch anything
    /// else: user data written after the snapshot must survive a rollback.
    #[serde(default)]
    backup_dir: PathBuf,
    /// Pre-update copies of the shortcut files, indexed by `shortcuts` order.
    #[serde(default)]
    shortcuts_backup_dir: PathBuf,
    #[serde(default)]
    registry_uninstall_backup: PathBuf,
    #[serde(default)]
    registry_run_backup: PathBuf,
    /// Shortcut files the installer may rewrite (Start Menu, Desktop).
    #[serde(default)]
    shortcuts: Vec<PathBuf>,
    /// Single-line phase marker written by the helper (backup, install,
    /// verify, rollback, finalize) for diagnostics after interruptions.
    #[serde(default)]
    journal_path: PathBuf,
    installer_path: PathBuf,
    readiness_marker: PathBuf,
    #[serde(default)]
    pending_state_path: PathBuf,
    transaction_path: PathBuf,
    result_path: PathBuf,
    created_at_unix_ms: u64,
}

struct Inner {
    status: AppUpdateStatus,
    pending: Option<PendingUpdate>,
    in_flight: bool,
    cancellation: Option<CancellationToken>,
    scheduler_started: bool,
}

pub struct AppUpdateState(Mutex<Inner>);

impl Default for AppUpdateState {
    fn default() -> Self {
        let status = match configuration() {
            Ok(Some(_)) => AppUpdateStatus::idle(),
            Ok(None) => {
                AppUpdateStatus::disabled("application updates are not configured for this build")
            }
            Err(error) => AppUpdateStatus::disabled(error),
        };
        Self(Mutex::new(Inner {
            status,
            pending: None,
            in_flight: false,
            cancellation: None,
            scheduler_started: false,
        }))
    }
}

#[derive(Clone)]
struct UpdateConfiguration {
    endpoint: url::Url,
    public_key: [u8; 32],
}

fn configuration() -> Result<Option<UpdateConfiguration>, String> {
    let endpoint = option_env!("RAVYN_APP_UPDATE_ENDPOINT")
        .unwrap_or_default()
        .trim();
    let public_key = option_env!("RAVYN_APP_UPDATE_PUBLIC_KEY")
        .unwrap_or_default()
        .trim();
    if endpoint.is_empty() && public_key.is_empty() {
        return Ok(None);
    }
    if endpoint.is_empty() || public_key.is_empty() {
        return Err(
            "both RAVYN_APP_UPDATE_ENDPOINT and RAVYN_APP_UPDATE_PUBLIC_KEY are required".into(),
        );
    }
    let endpoint = url::Url::parse(endpoint)
        .map_err(|error| format!("invalid app update endpoint: {error}"))?;
    if endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(
            "the app update endpoint must use HTTPS without credentials or fragments".into(),
        );
    }
    let public_key: [u8; 32] = hex::decode(public_key)
        .map_err(|_| "the app update public key must be hexadecimal".to_owned())?
        .try_into()
        .map_err(|_| "the app update public key must contain 32 bytes".to_owned())?;
    Ok(Some(UpdateConfiguration {
        endpoint,
        public_key,
    }))
}

pub fn status(app: &AppHandle) -> Result<AppUpdateStatus, String> {
    let state = app.state::<AppUpdateState>();
    let mut status = {
        let inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        inner.status.clone()
    };
    match read_last_result(app) {
        Ok(result) => status.last_result = result,
        Err(error) => {
            status.last_error.get_or_insert(error);
        }
    }
    Ok(status)
}

/// Confirms that an updated installed copy reached both backend and webview
/// readiness. The detached helper watches this marker before deleting the
/// retained previous binary or deciding to roll back.
pub fn confirm_update_readiness(app: &AppHandle) -> Result<(), String> {
    crate::integration::confirm_installed_copy_ready();
    let transaction_path = update_directory(app)?.join(UPDATE_TRANSACTION_FILENAME);
    let Some(transaction) = read_json_file::<PendingUpdateTransaction>(&transaction_path)? else {
        return Ok(());
    };
    if transaction.schema != UPDATE_TRANSACTION_SCHEMA {
        return Err("the pending app update transaction has an unsupported schema".into());
    }
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("failed to resolve the running Ravyn executable: {error}"))?;
    if !same_path(&current_exe, &transaction.installed_exe)
        || env!("CARGO_PKG_VERSION") != transaction.to_version.trim_start_matches('v')
    {
        return Ok(());
    }
    write_bytes_atomic_sync(&transaction.readiness_marker, b"ready\n")?;
    if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
        inner.status.phase = AppUpdatePhase::UpToDate;
        inner.status.available_version = None;
        inner.status.install_on_exit = false;
        inner.status.repair_mode = false;
        inner.status.last_error = None;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryAction {
    /// The transaction is fresh; a helper may still be driving it.
    LeaveAlone,
    /// We are running the target version, so the install succeeded but the
    /// helper died before finalizing. Complete its bookkeeping.
    FinalizeSucceeded,
    /// The helper died and the installation did not move to the target
    /// version (or the journal is unreadable). Clean up and require an
    /// explicit retry.
    CleanupFailed,
}

/// Decides how to recover a persisted update transaction found at startup.
fn plan_recovery(
    schema: u32,
    to_version: &str,
    created_at_unix_ms: u64,
    current_version: &str,
    now_unix_ms: u64,
) -> RecoveryAction {
    if schema != UPDATE_TRANSACTION_SCHEMA {
        return RecoveryAction::CleanupFailed;
    }
    let plausible_timestamp = created_at_unix_ms <= now_unix_ms.saturating_add(10 * 60 * 1000);
    let fresh = plausible_timestamp
        && now_unix_ms.saturating_sub(created_at_unix_ms) <= INTERRUPTED_TRANSACTION_STALE_MS;
    if fresh {
        return RecoveryAction::LeaveAlone;
    }
    if current_version == to_version.trim_start_matches('v') {
        RecoveryAction::FinalizeSucceeded
    } else {
        RecoveryAction::CleanupFailed
    }
}

/// Removes every on-disk artifact a transaction may have left behind.
fn remove_transaction_artifacts(update_dir: &Path, transaction: Option<&PendingUpdateTransaction>) {
    if let Some(transaction) = transaction {
        let _ = std::fs::remove_file(&transaction.readiness_marker);
        let _ = std::fs::remove_file(&transaction.journal_path);
        let _ = std::fs::remove_dir_all(&transaction.backup_dir);
        let _ = std::fs::remove_dir_all(&transaction.shortcuts_backup_dir);
        let _ = std::fs::remove_file(&transaction.registry_uninstall_backup);
        let _ = std::fs::remove_file(&transaction.registry_run_backup);
    }
    let _ = std::fs::remove_file(update_dir.join(UPDATE_JOURNAL_FILENAME));
    let _ = std::fs::remove_file(update_dir.join(LEGACY_UPDATE_BACKUP_FILENAME));
    let _ = std::fs::remove_file(update_dir.join(UPDATE_PENDING_STATE_FILENAME));
    let _ = std::fs::remove_file(update_dir.join(UPDATE_FILENAME));
    let _ = std::fs::remove_file(update_dir.join(format!("{UPDATE_FILENAME}.partial")));
    let _ = std::fs::remove_file(update_dir.join(UPDATE_TRANSACTION_FILENAME));
}

/// Detects an update transaction whose helper was interrupted (crash, kill,
/// or power loss between installer, readiness wait, and final cleanup) and
/// either finalizes or cleans it so updates never stay blocked.
pub fn recover_interrupted_update(app: &AppHandle) -> Result<(), String> {
    let update_dir = update_directory(app)?;
    let transaction_path = update_dir.join(UPDATE_TRANSACTION_FILENAME);
    if !transaction_path.exists() {
        return Ok(());
    }
    let transaction = match read_json_file::<PendingUpdateTransaction>(&transaction_path) {
        Ok(Some(transaction)) => Some(transaction),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "found an unreadable app update transaction; cleaning it up");
            None
        }
    };
    let journal_phase = std::fs::read_to_string(update_dir.join(UPDATE_JOURNAL_FILENAME))
        .map(|phase| phase.trim().to_owned())
        .unwrap_or_default();
    let action = transaction
        .as_ref()
        .map_or(RecoveryAction::CleanupFailed, |transaction| {
            plan_recovery(
                transaction.schema,
                &transaction.to_version,
                transaction.created_at_unix_ms,
                env!("CARGO_PKG_VERSION"),
                unix_timestamp_ms(),
            )
        });
    match action {
        RecoveryAction::LeaveAlone => Ok(()),
        RecoveryAction::FinalizeSucceeded => {
            let transaction = transaction.expect("finalize requires a parsed transaction");
            let result = AppUpdateResult {
                outcome: "succeeded".into(),
                from_version: transaction.from_version.clone(),
                to_version: transaction.to_version.clone(),
                completed_at_unix_ms: unix_timestamp_ms(),
                message: format!(
                    "The update installed, but its helper was interrupted (last phase: {}). Ravyn finalized it on startup.",
                    if journal_phase.is_empty() {
                        "unknown"
                    } else {
                        &journal_phase
                    },
                ),
            };
            write_json_atomic_sync(&transaction.result_path, &result)?;
            remove_transaction_artifacts(&update_dir, Some(&transaction));
            tracing::info!(
                to_version = %transaction.to_version,
                "finalized an interrupted app update after startup verification"
            );
            Ok(())
        }
        RecoveryAction::CleanupFailed => {
            let (from_version, to_version) = transaction
                .as_ref()
                .map(|transaction| {
                    (
                        transaction.from_version.clone(),
                        transaction.to_version.clone(),
                    )
                })
                .unwrap_or_else(|| (env!("CARGO_PKG_VERSION").to_owned(), "unknown".to_owned()));
            let result = AppUpdateResult {
                outcome: "failed".into(),
                from_version,
                to_version,
                completed_at_unix_ms: unix_timestamp_ms(),
                message: format!(
                    "The update helper was interrupted (last phase: {}) and the update did not complete. Use Check now to retry, or Repair to reinstall the current version.",
                    if journal_phase.is_empty() {
                        "unknown"
                    } else {
                        &journal_phase
                    },
                ),
            };
            write_json_atomic_sync(&update_dir.join(UPDATE_RESULT_FILENAME), &result)?;
            remove_transaction_artifacts(&update_dir, transaction.as_ref());
            tracing::warn!("cleaned up an interrupted app update transaction at startup");
            Ok(())
        }
    }
}

fn automatic_retry_delay(consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return AUTOMATIC_CHECK_INTERVAL;
    }
    let exponent = consecutive_failures.saturating_sub(1).min(3);
    AUTOMATIC_RETRY_BASE
        .checked_mul(1_u32 << exponent)
        .unwrap_or(AUTOMATIC_RETRY_MAX)
        .min(AUTOMATIC_RETRY_MAX)
}

fn schedule_next_check(app: &AppHandle, delay: Duration) {
    if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
        inner.status.next_check_at_unix_ms = Some(
            unix_timestamp_ms().saturating_add(delay.as_millis().try_into().unwrap_or(u64::MAX)),
        );
    }
}

pub fn start_background_check(app: AppHandle) {
    let installation = crate::installation::detect();
    let installed_build =
        installation.installed && !installation.portable && !installation.development;
    let automatic = {
        let state = app.state::<AppUpdateState>();
        let Ok(mut inner) = state.0.lock() else {
            return;
        };
        let automatic = installed_build && inner.status.configured;
        inner.status.automatic = automatic;
        inner.status.automatic_check_interval_secs =
            automatic.then_some(AUTOMATIC_CHECK_INTERVAL.as_secs());
        if !installed_build && inner.status.configured {
            inner.status.phase = AppUpdatePhase::Disabled;
            inner.status.last_error = Some(
                "automatic application updates are available only for installed builds".into(),
            );
        }
        if !automatic || inner.scheduler_started {
            false
        } else {
            inner.scheduler_started = true;
            true
        }
    };
    if !automatic {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let configuration = match configuration() {
            Ok(Some(configuration)) => configuration,
            Ok(None) => return,
            Err(error) => {
                set_error(&app, error);
                return;
            }
        };
        if let Err(error) = recover_interrupted_update(&app) {
            tracing::warn!(%error, "failed to recover an interrupted app update");
        }
        let restored = match restore_pending_update(&app, &configuration) {
            Ok(restored) => restored,
            Err(error) => {
                tracing::warn!(%error, "discarded an invalid persisted app update");
                false
            }
        };

        let mut consecutive_failures = 0_u32;
        if !restored {
            match check_and_stage(app.clone(), false, false).await {
                Ok(()) => consecutive_failures = 0,
                Err(error) => {
                    consecutive_failures = 1;
                    set_error(&app, error);
                }
            }
        }

        loop {
            let delay = automatic_retry_delay(consecutive_failures);
            schedule_next_check(&app, delay);
            tokio::time::sleep(delay).await;
            match check_and_stage(app.clone(), false, false).await {
                Ok(()) => consecutive_failures = 0,
                Err(error) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    set_error(&app, error);
                }
            }
        }
    });
}

pub async fn check_now(app: AppHandle) -> Result<AppUpdateStatus, String> {
    let installation = crate::installation::detect();
    if !installation.installed || installation.portable || installation.development {
        if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
            inner.status.automatic = false;
            inner.status.phase = AppUpdatePhase::Disabled;
            inner.status.last_error =
                Some("application updates are available only for installed Windows builds".into());
        }
        return status(&app);
    }
    match check_and_stage(app.clone(), true, false).await {
        Ok(()) => status(&app),
        Err(error) => {
            set_error(&app, error.clone());
            Err(error)
        }
    }
}

/// Download a signed installer even when the release feed contains the
/// currently running version. The normal close-time transaction then
/// reinstalls it with the same readiness and rollback guarantees as updates.
pub async fn repair_now(app: AppHandle) -> Result<AppUpdateStatus, String> {
    let installation = crate::installation::detect();
    if !installation.installed || installation.portable || installation.development {
        return Err("application repair is available only for installed Windows builds".into());
    }
    match check_and_stage(app.clone(), true, true).await {
        Ok(()) => status(&app),
        Err(error) => {
            set_error(&app, error.clone());
            Err(error)
        }
    }
}

/// Requests cancellation of an active metadata/download operation, or removes
/// an already verified installer that was waiting for the next close.
pub fn cancel(app: &AppHandle) -> Result<AppUpdateStatus, String> {
    let (cancellation, clear_staged, changed) = {
        let state = app.state::<AppUpdateState>();
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        if inner.in_flight {
            inner.status.phase = AppUpdatePhase::Cancelling;
            inner.status.last_error = None;
            (inner.cancellation.clone(), false, true)
        } else if inner.pending.take().is_some() || inner.status.phase == AppUpdatePhase::Ready {
            inner.status.phase = AppUpdatePhase::Cancelled;
            inner.status.available_version = None;
            inner.status.downloaded_bytes = 0;
            inner.status.total_bytes = None;
            inner.status.notes = None;
            inner.status.last_error = None;
            inner.status.install_on_exit = false;
            inner.status.repair_mode = false;
            (None, true, true)
        } else {
            (None, false, false)
        }
    };

    if !changed {
        return status(app);
    }
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
    }
    if clear_staged {
        clear_persisted_pending(app)?;
    }
    status(app)
}

fn ensure_update_not_cancelled(cancellation: &CancellationToken) -> Result<(), String> {
    if cancellation.is_cancelled() {
        Err(UPDATE_CANCELLED_ERROR.into())
    } else {
        Ok(())
    }
}

async fn check_and_stage(app: AppHandle, force: bool, repair: bool) -> Result<(), String> {
    let Some(configuration) = configuration()? else {
        return Err("application updates are not configured for this build".into());
    };
    // A stale interrupted transaction would otherwise block staging forever.
    if let Err(error) = recover_interrupted_update(&app) {
        tracing::warn!(%error, "failed to recover an interrupted app update");
    }

    let cancellation = CancellationToken::new();
    let clear_existing = {
        let state = app.state::<AppUpdateState>();
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        if inner.in_flight {
            return Ok(());
        }
        if inner.pending.is_some() && !force {
            return Ok(());
        }
        inner.in_flight = true;
        inner.cancellation = Some(cancellation.clone());
        inner.status.configured = true;
        inner.status.phase = AppUpdatePhase::Checking;
        inner.status.last_error = None;
        inner.status.downloaded_bytes = 0;
        inner.status.total_bytes = None;
        if force {
            inner.pending = None;
            inner.status.available_version = None;
            inner.status.notes = None;
            inner.status.install_on_exit = false;
            inner.status.repair_mode = false;
        }
        force
    };

    if clear_existing {
        if let Err(error) = clear_persisted_pending(&app) {
            if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
                inner.in_flight = false;
                inner.cancellation = None;
            }
            return Err(error);
        }
    }

    let result = perform_check_and_stage(&app, &configuration, force, repair, &cancellation).await;
    let cancelled = cancellation.is_cancelled()
        || result
            .as_ref()
            .is_err_and(|error| error == UPDATE_CANCELLED_ERROR);
    if cancelled {
        let _ = clear_persisted_pending(&app);
    }

    let state = app.state::<AppUpdateState>();
    if let Ok(mut inner) = state.0.lock() {
        inner.in_flight = false;
        inner.cancellation = None;
        inner.status.last_checked_at_unix_ms = Some(unix_timestamp_ms());
        if cancelled {
            inner.pending = None;
            inner.status.phase = AppUpdatePhase::Cancelled;
            inner.status.available_version = None;
            inner.status.downloaded_bytes = 0;
            inner.status.total_bytes = None;
            inner.status.notes = None;
            inner.status.last_error = None;
            inner.status.install_on_exit = false;
            inner.status.repair_mode = false;
        }
    }

    if cancelled { Ok(()) } else { result }
}

async fn perform_check_and_stage(
    app: &AppHandle,
    configuration: &UpdateConfiguration,
    force: bool,
    repair: bool,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("Ravyn/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(DOWNLOAD_TIMEOUT)
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects while downloading an app update")
            } else if attempt.url().scheme() == "https" {
                attempt.follow()
            } else {
                attempt.error("app update redirects must remain on HTTPS")
            }
        }))
        .build()
        .map_err(|error| format!("failed to initialize the app update client: {error}"))?;

    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err(UPDATE_CANCELLED_ERROR.into()),
        response = client.get(configuration.endpoint.clone()).send() => {
            response.map_err(|error| format!("failed to check for an app update: {error}"))?
        }
    }
    .error_for_status()
    .map_err(|error| format!("the app update service returned an error: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size > METADATA_LIMIT)
    {
        return Err("app update metadata exceeds the maximum size".into());
    }
    let metadata = read_response_bounded(
        response,
        METADATA_LIMIT,
        "app update metadata",
        cancellation,
    )
    .await?;
    let signed: SignedAppUpdateManifest = serde_json::from_slice(&metadata)
        .map_err(|error| format!("app update metadata is invalid: {error}"))?;
    let manifest = signed
        .verify(&configuration.public_key)
        .map_err(|error| error.to_string())?
        .clone();

    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("the current Ravyn version is invalid: {error}"))?;
    let available = Version::parse(manifest.version.trim_start_matches('v'))
        .map_err(|error| format!("the available Ravyn version is invalid: {error}"))?;
    if available < current || (available == current && !repair) {
        clear_persisted_pending(app)?;
        let state = app.state::<AppUpdateState>();
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        inner.pending = None;
        inner.status.phase = AppUpdatePhase::UpToDate;
        inner.status.available_version = None;
        inner.status.notes = None;
        inner.status.install_on_exit = false;
        inner.status.repair_mode = false;
        return Ok(());
    }
    let repair_current_version = repair && available == current;

    if !force && retry_is_blocked_by_last_result(app, &manifest.version)? {
        let state = app.state::<AppUpdateState>();
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        inner.pending = None;
        inner.status.phase = AppUpdatePhase::Error;
        inner.status.available_version = Some(manifest.version.clone());
        inner.status.notes = manifest.notes.clone();
        inner.status.install_on_exit = false;
        inner.status.repair_mode = false;
        inner.status.last_error = Some(
            "this update was rolled back or failed previously; use Check now to retry it manually"
                .into(),
        );
        return Ok(());
    }

    ensure_update_not_cancelled(cancellation)?;
    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err(UPDATE_CANCELLED_ERROR.into()),
        response = client.get(&manifest.artifact.url).send() => {
            response.map_err(|error| format!("failed to download Ravyn {}: {error}", manifest.version))?
        }
    }
    .error_for_status()
    .map_err(|error| format!("the app update download returned an error: {error}"))?;
    if response
        .content_length()
        .is_some_and(|size| size != manifest.artifact.size_bytes)
    {
        return Err("the app update server reported an unexpected installer size".into());
    }

    {
        let state = app.state::<AppUpdateState>();
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        inner.status.phase = AppUpdatePhase::Downloading;
        inner.status.available_version = Some(manifest.version.clone());
        inner.status.total_bytes = Some(manifest.artifact.size_bytes);
        inner.status.notes = if repair_current_version {
            Some("The signed installer for the current version will repair the installed application after Ravyn closes.".into())
        } else {
            manifest.notes.clone()
        };
        inner.status.downloaded_bytes = 0;
    }

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("failed to resolve the update staging directory: {error}"))?
        .join("updates");
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(|error| format!("failed to create the update staging directory: {error}"))?;
    let final_path = cache_dir.join(UPDATE_FILENAME);
    let partial_path = cache_dir.join(format!("{UPDATE_FILENAME}.partial"));
    let _ = tokio::fs::remove_file(&partial_path).await;
    let mut output = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| format!("failed to create the staged app update: {error}"))?;
    let mut stream = response.bytes_stream();
    let mut digest = Sha256::new();
    let mut downloaded = 0_u64;
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => {
                drop(output);
                let _ = tokio::fs::remove_file(&partial_path).await;
                return Err(UPDATE_CANCELLED_ERROR.into());
            }
            next = stream.next() => next,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|error| format!("app update download failed: {error}"))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "app update size overflow".to_owned())?;
        if downloaded > manifest.artifact.size_bytes {
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err("app update download exceeded the signed installer size".into());
        }
        tokio::select! {
            _ = cancellation.cancelled() => {
                drop(output);
                let _ = tokio::fs::remove_file(&partial_path).await;
                return Err(UPDATE_CANCELLED_ERROR.into());
            }
            result = output.write_all(&chunk) => {
                result.map_err(|error| format!("failed to write the staged app update: {error}"))?;
            }
        }
        digest.update(&chunk);
        if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
            inner.status.downloaded_bytes = downloaded;
        }
    }
    output
        .flush()
        .await
        .map_err(|error| format!("failed to flush the staged app update: {error}"))?;
    output
        .sync_all()
        .await
        .map_err(|error| format!("failed to persist the staged app update: {error}"))?;
    drop(output);

    if downloaded != manifest.artifact.size_bytes {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(format!(
            "app update size mismatch: expected {}, downloaded {downloaded}",
            manifest.artifact.size_bytes
        ));
    }
    let sha256 = hex::encode(digest.finalize());
    if !sha256.eq_ignore_ascii_case(&manifest.artifact.sha256) {
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err("app update SHA-256 verification failed".into());
    }
    ensure_update_not_cancelled(cancellation)?;
    let _ = tokio::fs::remove_file(&final_path).await;
    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|error| format!("failed to activate the staged app update: {error}"))?;

    if let Err(error) = persist_pending_update(app, &signed, repair_current_version) {
        let _ = tokio::fs::remove_file(&final_path).await;
        return Err(error);
    }
    if let Err(error) = ensure_update_not_cancelled(cancellation) {
        let _ = clear_persisted_pending(app);
        return Err(error);
    }

    let state = app.state::<AppUpdateState>();
    let mut inner = state
        .0
        .lock()
        .map_err(|_| "application update state is unavailable".to_owned())?;
    if cancellation.is_cancelled() {
        drop(inner);
        let _ = clear_persisted_pending(app);
        return Err(UPDATE_CANCELLED_ERROR.into());
    }
    inner.pending = Some(PendingUpdate {
        manifest: manifest.clone(),
        installer_path: final_path,
        repair: repair_current_version,
    });
    inner.status.phase = AppUpdatePhase::Ready;
    inner.status.available_version = Some(manifest.version);
    inner.status.downloaded_bytes = downloaded;
    inner.status.total_bytes = Some(downloaded);
    inner.status.install_on_exit = true;
    inner.status.repair_mode = repair_current_version;
    inner.status.last_error = None;
    Ok(())
}

fn restore_pending_update(
    app: &AppHandle,
    configuration: &UpdateConfiguration,
) -> Result<bool, String> {
    let update_dir = update_directory(app)?;
    if update_dir.join(UPDATE_TRANSACTION_FILENAME).exists() {
        return Ok(false);
    }
    let state_path = update_dir.join(UPDATE_PENDING_STATE_FILENAME);
    let persisted = match read_json_file::<PersistedPendingUpdate>(&state_path) {
        Ok(Some(persisted)) => persisted,
        Ok(None) => return Ok(false),
        Err(error) => {
            let _ = clear_persisted_pending(app);
            return Err(error);
        }
    };

    let restore_result = (|| {
        if persisted.schema != UPDATE_PENDING_STATE_SCHEMA {
            return Err("the persisted app update has an unsupported schema".into());
        }
        let now = unix_timestamp_ms();
        let future_limit = now.saturating_add(10 * 60 * 1000);
        let age_limit = PENDING_UPDATE_MAX_AGE_SECS.saturating_mul(1000);
        if persisted.staged_at_unix_ms > future_limit
            || now.saturating_sub(persisted.staged_at_unix_ms) > age_limit
        {
            return Err("the persisted app update is outside the allowed staging window".into());
        }
        let manifest = persisted
            .signed_manifest
            .verify(&configuration.public_key)
            .map_err(|error| error.to_string())?
            .clone();
        let current = Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|error| format!("the current Ravyn version is invalid: {error}"))?;
        let available = Version::parse(manifest.version.trim_start_matches('v'))
            .map_err(|error| format!("the persisted Ravyn version is invalid: {error}"))?;
        if available < current || (available == current && !persisted.repair) {
            return Err("the persisted app update no longer targets an eligible version".into());
        }
        if retry_is_blocked_by_last_result(app, &manifest.version)? {
            return Err("the persisted app update previously failed or was rolled back".into());
        }
        let pending = PendingUpdate {
            manifest: manifest.clone(),
            installer_path: update_dir.join(UPDATE_FILENAME),
            repair: persisted.repair,
        };
        verify_staged_installer(&pending)?;

        let state = app.state::<AppUpdateState>();
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        inner.pending = Some(pending);
        inner.status.configured = true;
        inner.status.phase = AppUpdatePhase::Ready;
        inner.status.available_version = Some(manifest.version);
        inner.status.downloaded_bytes = manifest.artifact.size_bytes;
        inner.status.total_bytes = Some(manifest.artifact.size_bytes);
        inner.status.notes = if persisted.repair {
            Some("The signed installer for the current version will repair the installed application after Ravyn closes.".into())
        } else {
            manifest.notes
        };
        inner.status.install_on_exit = true;
        inner.status.repair_mode = persisted.repair;
        inner.status.last_error = None;
        Ok(true)
    })();

    if restore_result.is_err() {
        let _ = clear_persisted_pending(app);
    }
    restore_result
}

fn persist_pending_update(
    app: &AppHandle,
    signed_manifest: &SignedAppUpdateManifest,
    repair: bool,
) -> Result<(), String> {
    let state = PersistedPendingUpdate {
        schema: UPDATE_PENDING_STATE_SCHEMA,
        signed_manifest: signed_manifest.clone(),
        staged_at_unix_ms: unix_timestamp_ms(),
        repair,
    };
    write_json_atomic_sync(
        &update_directory(app)?.join(UPDATE_PENDING_STATE_FILENAME),
        &state,
    )
}

fn clear_persisted_pending(app: &AppHandle) -> Result<(), String> {
    let update_dir = update_directory(app)?;
    remove_file_if_exists(&update_dir.join(UPDATE_PENDING_STATE_FILENAME))?;
    remove_file_if_exists(&update_dir.join(UPDATE_FILENAME))?;
    remove_file_if_exists(&update_dir.join(format!("{UPDATE_FILENAME}.partial")))?;
    Ok(())
}

fn retry_is_blocked_by_last_result(app: &AppHandle, version: &str) -> Result<bool, String> {
    let result = read_last_result(app)?;
    Ok(should_block_automatic_retry(result.as_ref(), version))
}

fn should_block_automatic_retry(result: Option<&AppUpdateResult>, version: &str) -> bool {
    let Some(result) = result else {
        return false;
    };
    matches!(result.outcome.as_str(), "failed" | "rolled_back")
        && result
            .to_version
            .trim_start_matches('v')
            .eq_ignore_ascii_case(version.trim_start_matches('v'))
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
    }
}

fn set_error(app: &AppHandle, error: String) {
    tracing::warn!(%error, "application update check failed");
    if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
        inner.in_flight = false;
        inner.cancellation = None;
        inner.status.last_checked_at_unix_ms = Some(unix_timestamp_ms());
        inner.status.phase = AppUpdatePhase::Error;
        inner.status.last_error = Some(error);
        inner.status.install_on_exit = inner.pending.is_some();
        inner.status.repair_mode = inner.pending.as_ref().is_some_and(|pending| pending.repair);
    }
}

/// Starts a detached installer helper. Returns true when the caller must
/// prevent the normal close event and terminate the process explicitly.
pub fn install_pending_on_close(app: &AppHandle) -> Result<bool, String> {
    let state = app.state::<AppUpdateState>();
    let pending = {
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        let Some(pending) = inner.pending.take() else {
            return Ok(false);
        };
        inner.status.phase = AppUpdatePhase::Installing;
        inner.status.install_on_exit = false;
        pending
    };

    let transaction =
        verify_staged_installer(&pending).and_then(|()| prepare_update_transaction(app, &pending));
    let install_result = transaction
        .as_ref()
        .map_err(|error| error.clone())
        .and_then(launch_installer_helper);
    if let Err(error) = install_result {
        if let Ok(transaction) = transaction {
            cleanup_unlaunched_transaction(&transaction);
        }
        let mut inner = state
            .0
            .lock()
            .map_err(|_| "application update state is unavailable".to_owned())?;
        inner.status.phase = AppUpdatePhase::Error;
        inner.status.last_error = Some(error.clone());
        inner.status.install_on_exit = true;
        inner.pending = Some(pending);
        return Err(error);
    }
    Ok(true)
}

fn verify_staged_installer(pending: &PendingUpdate) -> Result<(), String> {
    let file = std::fs::File::open(&pending.installer_path)
        .map_err(|error| format!("failed to open the staged app update: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("failed to inspect the staged app update: {error}"))?;
    if metadata.len() != pending.manifest.artifact.size_bytes {
        return Err("the staged app update size changed after download".into());
    }
    let mut reader = BufReader::new(file);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("failed to verify the staged app update: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let sha256 = hex::encode(digest.finalize());
    if !sha256.eq_ignore_ascii_case(&pending.manifest.artifact.sha256) {
        return Err("the staged app update hash changed after download".into());
    }
    Ok(())
}

/// Shortcut files the installer may rewrite and the rollback must restore.
fn installed_shortcut_paths() -> Vec<PathBuf> {
    let mut shortcuts = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        shortcuts
            .push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Ravyn.lnk"));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        shortcuts.push(PathBuf::from(profile).join(r"Desktop\Ravyn.lnk"));
    }
    shortcuts
}

fn prepare_update_transaction(
    app: &AppHandle,
    pending: &PendingUpdate,
) -> Result<PendingUpdateTransaction, String> {
    let install_dir = crate::installation::default_install_dir()
        .map(PathBuf::from)
        .ok_or_else(|| "failed to resolve the installed Ravyn directory".to_owned())?;
    let installed_exe = install_dir.join("Ravyn.exe");
    if !installed_exe.is_file() {
        return Err("the installed Ravyn executable could not be found".into());
    }
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("failed to resolve the running Ravyn executable: {error}"))?;
    if !same_path(&current_exe, &installed_exe) {
        return Err(
            "application updates can only start from the installed Ravyn executable".into(),
        );
    }

    let update_dir = update_directory(app)?;
    std::fs::create_dir_all(&update_dir)
        .map_err(|error| format!("failed to create the update state directory: {error}"))?;
    let transaction_path = update_dir.join(UPDATE_TRANSACTION_FILENAME);
    if transaction_path.exists() {
        return Err("a previous application update transaction is still awaiting recovery".into());
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    let transaction = PendingUpdateTransaction {
        schema: UPDATE_TRANSACTION_SCHEMA,
        token: token.clone(),
        from_version: env!("CARGO_PKG_VERSION").to_owned(),
        to_version: pending.manifest.version.clone(),
        install_dir,
        installed_exe,
        backup_dir: update_dir.join(format!("backup-{token}")),
        shortcuts_backup_dir: update_dir.join(format!("shortcuts-{token}")),
        registry_uninstall_backup: update_dir.join(format!("registry-uninstall-{token}.reg")),
        registry_run_backup: update_dir.join(format!("registry-run-{token}.reg")),
        shortcuts: installed_shortcut_paths(),
        journal_path: update_dir.join(UPDATE_JOURNAL_FILENAME),
        installer_path: pending.installer_path.clone(),
        readiness_marker: update_dir.join(format!("ravyn-update-ready-{token}.marker")),
        pending_state_path: update_dir.join(UPDATE_PENDING_STATE_FILENAME),
        transaction_path,
        result_path: update_dir.join(UPDATE_RESULT_FILENAME),
        created_at_unix_ms: unix_timestamp_ms(),
    };
    let _ = std::fs::remove_file(&transaction.readiness_marker);
    let _ = std::fs::remove_file(&transaction.journal_path);
    write_json_atomic_sync(&transaction.transaction_path, &transaction)?;
    Ok(transaction)
}

fn cleanup_unlaunched_transaction(transaction: &PendingUpdateTransaction) {
    let _ = std::fs::remove_file(&transaction.readiness_marker);
    let _ = std::fs::remove_file(&transaction.journal_path);
    let _ = std::fs::remove_file(&transaction.transaction_path);
}

#[cfg(windows)]
fn launch_installer_helper(transaction: &PendingUpdateTransaction) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = build_installer_helper_script(transaction, std::process::id());
    std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("failed to start the app update installer helper: {error}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn launch_installer_helper(_transaction: &PendingUpdateTransaction) -> Result<(), String> {
    Err("application update installation is supported only on Windows".into())
}

fn build_installer_helper_script(
    transaction: &PendingUpdateTransaction,
    parent_pid: u32,
) -> String {
    build_installer_helper_script_with_timeout(transaction, parent_pid, READINESS_TIMEOUT_SECS)
}

fn build_installer_helper_script_with_timeout(
    transaction: &PendingUpdateTransaction,
    parent_pid: u32,
    readiness_timeout_secs: u64,
) -> String {
    use std::fmt::Write as _;

    let shortcuts = transaction
        .shortcuts
        .iter()
        .map(|path| powershell_literal(path))
        .collect::<Vec<_>>()
        .join(",");
    let shortcuts = if shortcuts.is_empty() {
        "@()".to_owned()
    } else {
        format!("@({shortcuts})")
    };

    let mut script = String::new();
    writeln!(&mut script, "$ErrorActionPreference='Stop';").unwrap();
    writeln!(&mut script, "$parentPid={parent_pid};").unwrap();
    for (name, path) in [
        ("installDir", &transaction.install_dir),
        ("installed", &transaction.installed_exe),
        ("backupDir", &transaction.backup_dir),
        ("shortcutBackupDir", &transaction.shortcuts_backup_dir),
        ("regUninstallBackup", &transaction.registry_uninstall_backup),
        ("regRunBackup", &transaction.registry_run_backup),
        ("journal", &transaction.journal_path),
        ("installer", &transaction.installer_path),
        ("ready", &transaction.readiness_marker),
        ("transactionPath", &transaction.transaction_path),
        ("pendingStatePath", &transaction.pending_state_path),
        ("resultPath", &transaction.result_path),
    ] {
        writeln!(&mut script, "${name}={};", powershell_literal(path)).unwrap();
    }
    writeln!(&mut script, "$shortcuts={shortcuts};").unwrap();
    writeln!(
        &mut script,
        "$fromVersion={};",
        powershell_string(&transaction.from_version)
    )
    .unwrap();
    writeln!(
        &mut script,
        "$toVersion={};",
        powershell_string(&transaction.to_version)
    )
    .unwrap();
    writeln!(&mut script, "$timeoutSeconds={readiness_timeout_secs};").unwrap();
    writeln!(
        &mut script,
        "$regUninstallKey={};",
        powershell_string(REGISTRY_UNINSTALL_KEY)
    )
    .unwrap();
    writeln!(
        &mut script,
        "$regRunKey={};",
        powershell_string(REGISTRY_RUN_KEY)
    )
    .unwrap();
    script.push_str(
        "function Write-Journal([string]$phase) { try { Set-Content -LiteralPath $journal -Value $phase -Force } catch {} };\n\
         function Restore-Installation {\n\
           Write-Journal 'rollback';\n\
           if (Test-Path -LiteralPath $backupDir) {\n\
             Get-ChildItem -LiteralPath $installDir -File -Force -ErrorAction SilentlyContinue | Where-Object { $_.Extension -in '.exe','.dll' } | Remove-Item -Force -ErrorAction SilentlyContinue;\n\
             Get-ChildItem -LiteralPath $backupDir -File -Force | Copy-Item -Destination $installDir -Force;\n\
           };\n\
           if ($script:uninstallKeyExisted) { if (Test-Path -LiteralPath $regUninstallBackup) { reg.exe import $regUninstallBackup | Out-Null } }\n\
           else { reg.exe delete $regUninstallKey /f | Out-Null };\n\
           if ($script:runHadRavyn) { if (Test-Path -LiteralPath $regRunBackup) { reg.exe import $regRunBackup | Out-Null } }\n\
           else { Remove-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name 'Ravyn' -Force -ErrorAction SilentlyContinue };\n\
           $i=0;\n\
           foreach ($link in $shortcuts) {\n\
             $saved=Join-Path $shortcutBackupDir (\"$i.lnk\");\n\
             if (Test-Path -LiteralPath $saved) { Copy-Item -LiteralPath $saved -Destination $link -Force -ErrorAction SilentlyContinue }\n\
             elseif (Test-Path -LiteralPath $link) { Remove-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue };\n\
             $i++;\n\
           };\n\
         };\n\
         $ravyn=Get-Process -Id $parentPid -ErrorAction SilentlyContinue;\n\
         if ($null -ne $ravyn) { $ravyn.WaitForExit() };\n\
         $outcome='failed'; $message=''; $launched=$null;\n\
         $script:uninstallKeyExisted=$false; $script:runHadRavyn=$false;\n\
         try {\n\
           Write-Journal 'backup';\n\
           Remove-Item -LiteralPath $ready -Force -ErrorAction SilentlyContinue;\n\
           if (Test-Path -LiteralPath $backupDir) { Remove-Item -LiteralPath $backupDir -Recurse -Force };\n\
           New-Item -ItemType Directory -Force -Path $backupDir | Out-Null;\n\
           Get-ChildItem -LiteralPath $installDir -File -Force | Where-Object { $_.Extension -in '.exe','.dll' } | Copy-Item -Destination $backupDir -Force;\n\
           $script:uninstallKeyExisted=Test-Path 'Registry::HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ravyn';\n\
           if ($script:uninstallKeyExisted) { reg.exe export $regUninstallKey $regUninstallBackup /y | Out-Null };\n\
           $script:runHadRavyn=$null -ne (Get-ItemProperty -Path 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name 'Ravyn' -ErrorAction SilentlyContinue);\n\
           if ($script:runHadRavyn) { reg.exe export $regRunKey $regRunBackup /y | Out-Null };\n\
           New-Item -ItemType Directory -Force -Path $shortcutBackupDir | Out-Null;\n\
           $i=0;\n\
           foreach ($link in $shortcuts) {\n\
             if (Test-Path -LiteralPath $link) { Copy-Item -LiteralPath $link -Destination (Join-Path $shortcutBackupDir (\"$i.lnk\")) -Force };\n\
             $i++;\n\
           };\n\
           Write-Journal 'install';\n\
           $setup=Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru;\n\
           if ($setup.ExitCode -ne 0) { throw \"installer exited with code $($setup.ExitCode)\" };\n\
           Write-Journal 'verify';\n\
           $launched=Start-Process -FilePath $installed -PassThru;\n\
           $deadline=(Get-Date).AddSeconds($timeoutSeconds);\n\
           while ((Get-Date) -lt $deadline) {\n\
             if (Test-Path -LiteralPath $ready) { $outcome='succeeded'; $message='The updated version reached backend and UI readiness.'; break };\n\
             if ($launched.HasExited) { break };\n\
             Start-Sleep -Milliseconds 500;\n\
           };\n\
           if ($outcome -ne 'succeeded') {\n\
             $message='The updated version did not reach readiness before the safety deadline.';\n\
             if (($null -ne $launched) -and (!$launched.HasExited)) { Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue; Wait-Process -Id $launched.Id -Timeout 10 -ErrorAction SilentlyContinue };\n\
             Restore-Installation;\n\
             $outcome='rolled_back';\n\
           };\n\
         } catch {\n\
           $message=$_.Exception.Message;\n\
           if (($null -ne $launched) -and (!$launched.HasExited)) { Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue; Wait-Process -Id $launched.Id -Timeout 10 -ErrorAction SilentlyContinue };\n\
           if (Test-Path -LiteralPath $backupDir) {\n\
             Restore-Installation;\n\
             $outcome='rolled_back';\n\
           };\n\
         };\n\
         Write-Journal 'finalize';\n\
         Remove-Item -LiteralPath $backupDir -Recurse -Force -ErrorAction SilentlyContinue;\n\
         Remove-Item -LiteralPath $shortcutBackupDir -Recurse -Force -ErrorAction SilentlyContinue;\n\
         Remove-Item -LiteralPath $regUninstallBackup -Force -ErrorAction SilentlyContinue;\n\
         Remove-Item -LiteralPath $regRunBackup -Force -ErrorAction SilentlyContinue;\n\
         if ($outcome -eq 'succeeded') {\n\
           Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue;\n\
         };\n\
         try {\n\
           $resultObject=[ordered]@{outcome=$outcome;from_version=$fromVersion;to_version=$toVersion;completed_at_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();message=$message};\n\
           $json=$resultObject | ConvertTo-Json -Compress;\n\
           $resultTemp=\"$resultPath.tmp\";\n\
           [System.IO.File]::WriteAllText($resultTemp,$json,(New-Object System.Text.UTF8Encoding($false)));\n\
           Remove-Item -LiteralPath $resultPath -Force -ErrorAction SilentlyContinue;\n\
           Move-Item -LiteralPath $resultTemp -Destination $resultPath;\n\
         } catch {\n\
           $message=\"$message Result persistence failed: $($_.Exception.Message)\".Trim();\n\
         } finally {\n\
           Remove-Item -LiteralPath $ready -Force -ErrorAction SilentlyContinue;\n\
           Remove-Item -LiteralPath $pendingStatePath -Force -ErrorAction SilentlyContinue;\n\
           Remove-Item -LiteralPath $transactionPath -Force -ErrorAction SilentlyContinue;\n\
           Remove-Item -LiteralPath $journal -Force -ErrorAction SilentlyContinue;\n\
           if (($outcome -eq 'rolled_back') -or ($outcome -eq 'failed')) { if (Test-Path -LiteralPath $installed) { Start-Process -FilePath $installed | Out-Null } };\n\
         };\n\
         if ($outcome -eq 'failed') { exit 1 } else { exit 0 };\n",
    );
    script
}

fn update_directory(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_cache_dir()
        .map(|path| path.join("updates"))
        .map_err(|error| format!("failed to resolve the update state directory: {error}"))
}

fn read_last_result(app: &AppHandle) -> Result<Option<AppUpdateResult>, String> {
    read_json_file(&update_directory(app)?.join(UPDATE_RESULT_FILENAME))
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        return Err(format!("{} is empty or oversized", path.display()));
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map(Some)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn write_json_atomic_sync(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize update state: {error}"))?;
    write_bytes_atomic_sync(path, &bytes)
}

fn write_bytes_atomic_sync(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "the update state path has no parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create the update state directory: {error}"))?;
    let temporary = path.with_extension("tmp");
    let _ = std::fs::remove_file(&temporary);
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("failed to write update state: {error}"))?;
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("failed to replace update state: {error}"))?;
    }
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("failed to activate update state: {error}"))
}

async fn read_response_bounded(
    response: reqwest::Response,
    limit: u64,
    label: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, String> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => return Err(UPDATE_CANCELLED_ERROR.into()),
            next = stream.next() => next,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|error| format!("failed to read {label}: {error}"))?;
        let next_len = bytes.len().saturating_add(chunk.len());
        if u64::try_from(next_len).unwrap_or(u64::MAX) > limit {
            return Err(format!("{label} exceeds the maximum size"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(bytes)
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    normalize(left) == normalize(right)
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn powershell_literal(path: &Path) -> String {
    powershell_string(&path.to_string_lossy())
}

fn powershell_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_status_disabled_contains_current_version() {
        let status = AppUpdateStatus::disabled("not configured");
        assert_eq!(status.phase, AppUpdatePhase::Disabled);
        assert_eq!(status.current_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(status.last_error.as_deref(), Some("not configured"));
        assert!(status.last_result.is_none());
    }

    #[test]
    fn powershell_paths_escape_single_quotes() {
        let value = powershell_literal(Path::new(r"C:\Users\O'Brien\setup.exe"));
        assert_eq!(value, r"'C:\Users\O''Brien\setup.exe'");
    }

    fn sample_transaction() -> PendingUpdateTransaction {
        PendingUpdateTransaction {
            schema: UPDATE_TRANSACTION_SCHEMA,
            token: "token".into(),
            from_version: "0.2.0".into(),
            to_version: "0.3.0".into(),
            install_dir: PathBuf::from(r"C:\Ravyn"),
            installed_exe: PathBuf::from(r"C:\Ravyn\Ravyn.exe"),
            backup_dir: PathBuf::from(r"C:\cache\backup-token"),
            shortcuts_backup_dir: PathBuf::from(r"C:\cache\shortcuts-token"),
            registry_uninstall_backup: PathBuf::from(r"C:\cache\registry-uninstall-token.reg"),
            registry_run_backup: PathBuf::from(r"C:\cache\registry-run-token.reg"),
            shortcuts: vec![
                PathBuf::from(r"C:\Users\Tester\Start Menu\Programs\Ravyn.lnk"),
                PathBuf::from(r"C:\Users\Tester\Desktop\Ravyn.lnk"),
            ],
            journal_path: PathBuf::from(r"C:\cache\journal.txt"),
            installer_path: PathBuf::from(r"C:\cache\update.exe"),
            readiness_marker: PathBuf::from(r"C:\cache\ready.marker"),
            pending_state_path: PathBuf::from(r"C:\cache\pending.json"),
            transaction_path: PathBuf::from(r"C:\cache\transaction.json"),
            result_path: PathBuf::from(r"C:\cache\result.json"),
            created_at_unix_ms: 1,
        }
    }

    #[test]
    fn helper_script_contains_readiness_and_rollback_guards() {
        let script = build_installer_helper_script(&sample_transaction(), 42);
        assert!(script.contains("$parentPid=42"));
        assert!(script.contains("The updated version did not reach readiness"));
        assert!(script.contains("Restore-Installation"));
        assert!(script.contains("completed_at_unix_ms"));
        assert!(script.contains("Remove-Item -LiteralPath $pendingStatePath"));
        let result_write = script.find("Move-Item -LiteralPath $resultTemp").unwrap();
        let rollback_relaunch = script
            .rfind("Start-Process -FilePath $installed | Out-Null")
            .unwrap();
        assert!(result_write < rollback_relaunch);
    }

    #[test]
    fn helper_script_backs_up_binaries_registry_and_shortcuts() {
        let script = build_installer_helper_script(&sample_transaction(), 42);
        // Only application binaries: the install dir doubles as the data dir.
        assert!(script.contains("$_.Extension -in '.exe','.dll'"));
        assert!(!script.contains("Copy-Item -LiteralPath $installDir"));
        assert!(script.contains("reg.exe export $regUninstallKey"));
        assert!(script.contains("reg.exe import $regUninstallBackup"));
        assert!(script.contains("reg.exe delete $regUninstallKey /f"));
        assert!(script.contains("Remove-ItemProperty"));
        assert!(script.contains(r"Desktop\Ravyn.lnk"));
        // Journal phases are advanced before every state-changing step.
        for phase in [
            "'backup'",
            "'install'",
            "'verify'",
            "'rollback'",
            "'finalize'",
        ] {
            assert!(
                script.contains(&format!("Write-Journal {phase}")),
                "missing journal phase {phase}"
            );
        }
        // The journal is removed only in the final cleanup block.
        assert!(script.contains("Remove-Item -LiteralPath $journal"));
    }

    #[test]
    fn recovery_leaves_fresh_transactions_alone() {
        let now = 1_000_000_000_000_u64;
        assert_eq!(
            plan_recovery(
                UPDATE_TRANSACTION_SCHEMA,
                "0.3.0",
                now - 60_000,
                "0.2.0",
                now
            ),
            RecoveryAction::LeaveAlone
        );
        // Freshly installed target version: the helper is still waiting for
        // the readiness marker; recovery must not race it.
        assert_eq!(
            plan_recovery(
                UPDATE_TRANSACTION_SCHEMA,
                "v0.3.0",
                now - 60_000,
                "0.3.0",
                now
            ),
            RecoveryAction::LeaveAlone
        );
    }

    #[test]
    fn recovery_finalizes_stale_transactions_on_the_target_version() {
        let now = 1_000_000_000_000_u64;
        let stale = now - INTERRUPTED_TRANSACTION_STALE_MS - 1;
        assert_eq!(
            plan_recovery(UPDATE_TRANSACTION_SCHEMA, "v0.3.0", stale, "0.3.0", now),
            RecoveryAction::FinalizeSucceeded
        );
    }

    #[test]
    fn recovery_cleans_up_stale_or_foreign_transactions() {
        let now = 1_000_000_000_000_u64;
        let stale = now - INTERRUPTED_TRANSACTION_STALE_MS - 1;
        // Still on the previous version: the install never completed.
        assert_eq!(
            plan_recovery(UPDATE_TRANSACTION_SCHEMA, "0.3.0", stale, "0.2.0", now),
            RecoveryAction::CleanupFailed
        );
        // Unsupported schema is cleaned regardless of age.
        assert_eq!(
            plan_recovery(2, "0.3.0", now - 1_000, "0.2.0", now),
            RecoveryAction::CleanupFailed
        );
        // A timestamp far in the future is implausible, not fresh.
        assert_eq!(
            plan_recovery(
                UPDATE_TRANSACTION_SCHEMA,
                "0.3.0",
                now + 60 * 60 * 1000,
                "0.2.0",
                now
            ),
            RecoveryAction::CleanupFailed
        );
    }

    #[test]
    fn failed_versions_require_an_explicit_retry() {
        let rolled_back = AppUpdateResult {
            outcome: "rolled_back".into(),
            from_version: "0.2.0".into(),
            to_version: "v0.3.0".into(),
            completed_at_unix_ms: 1,
            message: "readiness failed".into(),
        };
        assert!(should_block_automatic_retry(Some(&rolled_back), "0.3.0"));
        assert!(!should_block_automatic_retry(Some(&rolled_back), "0.4.0"));

        let succeeded = AppUpdateResult {
            outcome: "succeeded".into(),
            ..rolled_back
        };
        assert!(!should_block_automatic_retry(Some(&succeeded), "0.3.0"));
        assert!(!should_block_automatic_retry(None, "0.3.0"));
    }

    #[test]
    fn automatic_update_retries_back_off_and_remain_bounded() {
        assert_eq!(automatic_retry_delay(0), AUTOMATIC_CHECK_INTERVAL);
        assert_eq!(automatic_retry_delay(1), Duration::from_secs(15 * 60));
        assert_eq!(automatic_retry_delay(2), Duration::from_secs(30 * 60));
        assert_eq!(automatic_retry_delay(3), Duration::from_secs(60 * 60));
        assert_eq!(automatic_retry_delay(4), AUTOMATIC_RETRY_MAX);
        assert_eq!(automatic_retry_delay(u32::MAX), AUTOMATIC_RETRY_MAX);
    }

    #[test]
    fn cancellation_tokens_are_reported_without_update_errors() {
        let token = CancellationToken::new();
        assert!(ensure_update_not_cancelled(&token).is_ok());
        token.cancel();
        assert_eq!(
            ensure_update_not_cancelled(&token).unwrap_err(),
            UPDATE_CANCELLED_ERROR,
        );
    }

    /// Writes the generated helper script to a file so CI or a developer can
    /// validate it with the real PowerShell parser (run with `-- --ignored`).
    #[test]
    #[ignore]
    fn dump_helper_script_for_parser_validation() {
        let output = std::env::temp_dir().join("ravyn-helper-script.ps1");
        let script = if let Some(root) = std::env::var_os("RAVYN_HELPER_TEST_ROOT") {
            let root = PathBuf::from(root);
            let install_dir = root.join("install");
            let transaction = PendingUpdateTransaction {
                schema: UPDATE_TRANSACTION_SCHEMA,
                token: "lifecycle-test".into(),
                from_version: std::env::var("RAVYN_HELPER_FROM_VERSION")
                    .unwrap_or_else(|_| "0.2.0".into()),
                to_version: std::env::var("RAVYN_HELPER_TO_VERSION")
                    .unwrap_or_else(|_| "0.3.0".into()),
                install_dir: install_dir.clone(),
                installed_exe: install_dir.join("Ravyn.exe"),
                backup_dir: root.join("backup"),
                shortcuts_backup_dir: root.join("shortcuts"),
                registry_uninstall_backup: root.join("uninstall.reg"),
                registry_run_backup: root.join("run.reg"),
                shortcuts: Vec::new(),
                journal_path: root.join("journal.txt"),
                installer_path: root.join("update.exe"),
                readiness_marker: root.join("ready.marker"),
                pending_state_path: root.join("pending.json"),
                transaction_path: root.join("transaction.json"),
                result_path: root.join("result.json"),
                created_at_unix_ms: 1,
            };
            build_installer_helper_script_with_timeout(&transaction, i32::MAX as u32, 3)
        } else {
            build_installer_helper_script(&sample_transaction(), 42)
        };
        std::fs::write(output, script).unwrap();
    }
}
