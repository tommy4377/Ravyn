//! Embedded Ravyn backend lifecycle.
//!
//! The desktop shell runs the full Ravyn backend in-process on an ephemeral
//! loopback port. The bound address is published through a watch channel so
//! frontend windows can ask for it with `backend_info`.

use serde::Serialize;
use tokio::sync::watch;

/// Snapshot handed to the frontend once the backend is listening.
#[derive(Debug, Clone, Serialize)]
pub struct BackendInfo {
    pub base_url: String,
    pub api_token: String,
    pub data_dir: String,
    pub setup_completed: bool,
}

/// Shared handle to the embedded backend state.
#[derive(Clone)]
pub struct BackendHandle {
    receiver: watch::Receiver<Option<BackendInfo>>,
}

impl BackendHandle {
    /// Wait until the backend reports its bound address, up to `timeout`.
    pub async fn wait_ready(&self, timeout: std::time::Duration) -> Result<BackendInfo, String> {
        let mut receiver = self.receiver.clone();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(info) = receiver.borrow().clone() {
                return Ok(info);
            }
            tokio::select! {
                changed = receiver.changed() => {
                    if changed.is_err() {
                        return Err("backend task ended before reporting readiness".into());
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    return Err("timed out waiting for the embedded backend".into());
                }
            }
        }
    }
}

/// Resolve the persistent data directory for the desktop application.
///
/// `RAVYN_DATA_DIR` always wins so development and portable installations can
/// redirect state; otherwise the per-user local application data directory is
/// used, matching the documented installed-mode layout.
pub fn resolve_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("RAVYN_DATA_DIR") {
        if !dir.trim().is_empty() {
            return std::path::PathBuf::from(dir);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return std::path::PathBuf::from(local).join("Ravyn");
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("ravyn");
        }
    }
    std::path::PathBuf::from("./ravyn-data")
}

/// Start the embedded backend and return a handle plus the first-window label
/// decision channel.
pub fn start() -> (BackendHandle, watch::Receiver<Option<BackendInfo>>) {
    let (sender, receiver) = watch::channel(None);
    let handle = BackendHandle {
        receiver: receiver.clone(),
    };

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_backend(sender).await {
            tracing::error!(%error, "embedded Ravyn backend failed");
        }
    });

    (handle, receiver)
}

async fn run_backend(sender: watch::Sender<Option<BackendInfo>>) -> Result<(), String> {
    use clap::Parser as _;

    let data_dir = resolve_data_dir();
    let data_dir_str = data_dir.display().to_string();
    // This token is process-local and is passed only through the Tauri IPC
    // response to Ravyn's own webview. It is never written to settings or disk.
    let api_token = uuid::Uuid::new_v4().to_string();

    // Managed component installation is driven explicitly by the setup flow,
    // so startup auto-provisioning stays off in the desktop shell.
    let config = ravyn::config::Config::try_parse_from([
        "ravyn",
        "--data-dir",
        &data_dir_str,
        "--listen",
        "127.0.0.1:0",
        "--api-token",
        &api_token,
        "--auto-provision",
        "false",
    ])
    .map_err(|e| e.to_string())?;

    let app = ravyn::Ravyn::bootstrap(config)
        .await
        .map_err(|e| e.to_string())?;
    app.manager
        .clone()
        .start_workers()
        .await
        .map_err(|e| e.to_string())?;

    let setup_completed = app
        .repository
        .load_setup_state()
        .await
        .map_err(|e| e.to_string())?
        .is_some_and(|state| state.completed);

    let listener = tokio::net::TcpListener::bind(app.config.listen)
        .await
        .map_err(|e| e.to_string())?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;

    let info = BackendInfo {
        base_url: format!("http://{bound}"),
        api_token,
        data_dir: data_dir_str,
        setup_completed,
    };
    let descriptor_guard = crate::native_messaging::BackendDescriptorGuard::publish(&info)?;
    let _ = sender.send(Some(info.clone()));
    write_desktop_ready_marker(&info);
    crate::integration::confirm_installed_copy_ready();

    let result = ravyn::api::serve_with_listener(app, listener)
        .await
        .map_err(|e| e.to_string());
    drop(descriptor_guard);
    result
}

#[derive(Serialize)]
struct DesktopReadyMarker<'a> {
    schema: u32,
    process_id: u32,
    version: &'a str,
    base_url: &'a str,
    data_dir: &'a str,
    setup_completed: bool,
}

/// Writes an opt-in, non-secret readiness marker for desktop automation.
/// Production builds do nothing unless the test runner explicitly provides
/// `RAVYN_DESKTOP_READY_FILE` for the current process.
fn write_desktop_ready_marker(info: &BackendInfo) {
    let Ok(path) = std::env::var("RAVYN_DESKTOP_READY_FILE") else {
        return;
    };
    if path.trim().is_empty() {
        return;
    }
    let path = std::path::PathBuf::from(path);
    let marker = DesktopReadyMarker {
        schema: 1,
        process_id: std::process::id(),
        version: env!("CARGO_PKG_VERSION"),
        base_url: &info.base_url,
        data_dir: &info.data_dir,
        setup_completed: info.setup_completed,
    };
    let result = (|| -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "the desktop readiness marker has no parent directory".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create the readiness marker directory: {error}"))?;
        let bytes = serde_json::to_vec_pretty(&marker).map_err(|error| {
            format!("failed to serialize the desktop readiness marker: {error}")
        })?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, bytes)
            .map_err(|error| format!("failed to write the desktop readiness marker: {error}"))?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| {
                format!("failed to replace the desktop readiness marker: {error}")
            })?;
        }
        std::fs::rename(&temporary, &path)
            .map_err(|error| format!("failed to activate the desktop readiness marker: {error}"))?;
        Ok(())
    })();
    if let Err(error) = result {
        tracing::warn!(%error, path = %path.display(), "failed to publish desktop readiness marker");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_readiness_marker_contains_no_api_token() {
        let marker = DesktopReadyMarker {
            schema: 1,
            process_id: 42,
            version: "0.2.0",
            base_url: "http://127.0.0.1:12345",
            data_dir: r"C:\Users\Tester\AppData\Local\Ravyn",
            setup_completed: true,
        };
        let json = serde_json::to_string(&marker).unwrap();
        assert!(json.contains("127.0.0.1:12345"));
        assert!(!json.contains("api_token"));
        assert!(!json.contains("authorization"));
    }
}
