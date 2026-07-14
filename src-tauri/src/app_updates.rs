//! Silent, signed application updates for installed Windows builds.
//!
//! Ravyn downloads and verifies an installer in the background, then starts a
//! detached helper when the main window closes. The helper waits for Ravyn to
//! exit, runs the current-user NSIS installer silently, and relaunches the app.

use std::{
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use futures_util::StreamExt;
use ravyn::services::app_updates::{AppUpdateManifest, SignedAppUpdateManifest};
use semver::Version;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;

const METADATA_LIMIT: u64 = 512 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const UPDATE_FILENAME: &str = "ravyn-pending-update.exe";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdatePhase {
    Disabled,
    Idle,
    Checking,
    UpToDate,
    Downloading,
    Ready,
    Installing,
    Error,
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
        }
    }
}

struct PendingUpdate {
    manifest: AppUpdateManifest,
    installer_path: PathBuf,
}

struct Inner {
    status: AppUpdateStatus,
    pending: Option<PendingUpdate>,
    in_flight: bool,
}

pub struct AppUpdateState(Mutex<Inner>);

impl Default for AppUpdateState {
    fn default() -> Self {
        let status = match configuration() {
            Ok(Some(_)) => AppUpdateStatus::idle(),
            Ok(None) => AppUpdateStatus::disabled(
                "application updates are not configured for this build",
            ),
            Err(error) => AppUpdateStatus::disabled(error),
        };
        Self(Mutex::new(Inner {
            status,
            pending: None,
            in_flight: false,
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
            "both RAVYN_APP_UPDATE_ENDPOINT and RAVYN_APP_UPDATE_PUBLIC_KEY are required"
                .into(),
        );
    }
    let endpoint = url::Url::parse(endpoint)
        .map_err(|error| format!("invalid app update endpoint: {error}"))?;
    if endpoint.scheme() != "https" || endpoint.host_str().is_none() {
        return Err("the app update endpoint must use HTTPS".into());
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
    let inner = state
        .0
        .lock()
        .map_err(|_| "application update state is unavailable".to_owned())?;
    Ok(inner.status.clone())
}

pub fn start_background_check(app: AppHandle) {
    let installation = crate::installation::detect();
    let automatic = installation.installed && !installation.portable && !installation.development;
    if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
        inner.status.automatic = automatic;
        if !automatic && inner.status.configured {
            inner.status.phase = AppUpdatePhase::Disabled;
            inner.status.last_error = Some(
                "automatic application updates are available only for installed builds".into(),
            );
        }
    }
    if !automatic {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = check_and_stage(app.clone(), false).await {
            set_error(&app, error);
        }
    });
}

pub async fn check_now(app: AppHandle) -> Result<AppUpdateStatus, String> {
    let installation = crate::installation::detect();
    if !installation.installed || installation.portable || installation.development {
        if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
            inner.status.automatic = false;
            inner.status.phase = AppUpdatePhase::Disabled;
            inner.status.last_error = Some(
                "application updates are available only for installed Windows builds".into(),
            );
        }
        return status(&app);
    }
    match check_and_stage(app.clone(), true).await {
        Ok(()) => status(&app),
        Err(error) => {
            set_error(&app, error.clone());
            Err(error)
        }
    }
}

async fn check_and_stage(app: AppHandle, force: bool) -> Result<(), String> {
    let Some(configuration) = configuration()? else {
        return Err("application updates are not configured for this build".into());
    };

    {
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
        }
    }

    let result = perform_check_and_stage(&app, &configuration).await;
    let state = app.state::<AppUpdateState>();
    if let Ok(mut inner) = state.0.lock() {
        inner.in_flight = false;
    }
    result
}

async fn perform_check_and_stage(
    app: &AppHandle,
    configuration: &UpdateConfiguration,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("Ravyn/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(DOWNLOAD_TIMEOUT)
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

    let response = client
        .get(configuration.endpoint.clone())
        .send()
        .await
        .map_err(|error| format!("failed to check for an app update: {error}"))?
        .error_for_status()
        .map_err(|error| format!("the app update service returned an error: {error}"))?;
    if response.content_length().is_some_and(|size| size > METADATA_LIMIT) {
        return Err("app update metadata exceeds the maximum size".into());
    }
    let metadata = response
        .bytes()
        .await
        .map_err(|error| format!("failed to read app update metadata: {error}"))?;
    if metadata.len() as u64 > METADATA_LIMIT {
        return Err("app update metadata exceeds the maximum size".into());
    }
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
    if available <= current {
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
        return Ok(());
    }

    let response = client
        .get(&manifest.artifact.url)
        .send()
        .await
        .map_err(|error| format!("failed to download Ravyn {}: {error}", manifest.version))?
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
        inner.status.notes = manifest.notes.clone();
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
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("app update download failed: {error}"))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| "app update size overflow".to_owned())?;
        if downloaded > manifest.artifact.size_bytes {
            return Err("app update download exceeded the signed installer size".into());
        }
        output
            .write_all(&chunk)
            .await
            .map_err(|error| format!("failed to write the staged app update: {error}"))?;
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
    let _ = tokio::fs::remove_file(&final_path).await;
    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|error| format!("failed to activate the staged app update: {error}"))?;

    let state = app.state::<AppUpdateState>();
    let mut inner = state
        .0
        .lock()
        .map_err(|_| "application update state is unavailable".to_owned())?;
    inner.pending = Some(PendingUpdate {
        manifest: manifest.clone(),
        installer_path: final_path,
    });
    inner.status.phase = AppUpdatePhase::Ready;
    inner.status.available_version = Some(manifest.version);
    inner.status.downloaded_bytes = downloaded;
    inner.status.total_bytes = Some(downloaded);
    inner.status.install_on_exit = true;
    inner.status.last_error = None;
    Ok(())
}

fn set_error(app: &AppHandle, error: String) {
    tracing::warn!(%error, "application update check failed");
    if let Ok(mut inner) = app.state::<AppUpdateState>().0.lock() {
        inner.in_flight = false;
        inner.status.phase = AppUpdatePhase::Error;
        inner.status.last_error = Some(error);
        inner.status.install_on_exit = inner.pending.is_some();
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

    let install_result = verify_staged_installer(&pending)
        .and_then(|()| launch_installer_helper(&pending.installer_path));
    if let Err(error) = install_result {
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

#[cfg(windows)]
fn launch_installer_helper(installer_path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let installed_exe = crate::installation::default_install_dir()
        .map(PathBuf::from)
        .ok_or_else(|| "failed to resolve the installed Ravyn directory".to_owned())?
        .join("Ravyn.exe");
    if !installed_exe.is_file() {
        return Err("the installed Ravyn executable could not be found".into());
    }

    let installer = powershell_literal(installer_path);
    let executable = powershell_literal(&installed_exe);
    let script = format!(
        "$ErrorActionPreference='Stop'; \
         $ravyn=Get-Process -Id {} -ErrorAction SilentlyContinue; \
         if ($null -ne $ravyn) {{ $ravyn.WaitForExit() }}; \
         $exitCode=0; \
         try {{ $setup=Start-Process -FilePath {installer} -ArgumentList '/S' -Wait -PassThru; $exitCode=$setup.ExitCode }} catch {{ $exitCode=1 }}; \
         Start-Process -FilePath {executable}; \
         if ($exitCode -eq 0) {{ Remove-Item -LiteralPath {installer} -Force -ErrorAction SilentlyContinue }}; \
         exit $exitCode",
        std::process::id()
    );
    std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
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
fn launch_installer_helper(_installer_path: &Path) -> Result<(), String> {
    Err("application update installation is supported only on Windows".into())
}

#[cfg(windows)]
fn powershell_literal(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
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
    }

    #[cfg(windows)]
    #[test]
    fn powershell_paths_escape_single_quotes() {
        let value = powershell_literal(Path::new(r"C:\Users\O'Brien\setup.exe"));
        assert_eq!(value, r"'C:\Users\O''Brien\setup.exe'");
    }
}
