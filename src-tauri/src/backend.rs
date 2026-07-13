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

    // Managed component installation is driven explicitly by the setup flow,
    // so startup auto-provisioning stays off in the desktop shell.
    let config = ravyn::config::Config::try_parse_from([
        "ravyn",
        "--data-dir",
        &data_dir_str,
        "--listen",
        "127.0.0.1:0",
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

    let _ = sender.send(Some(BackendInfo {
        base_url: format!("http://{bound}"),
        data_dir: data_dir_str,
        setup_completed,
    }));

    ravyn::api::serve_with_listener(app, listener)
        .await
        .map_err(|e| e.to_string())
}
