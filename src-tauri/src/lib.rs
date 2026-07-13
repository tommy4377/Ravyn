//! Ravyn desktop shell.
//!
//! Embeds the Ravyn backend in-process, decides whether to open the setup or
//! the main window, and exposes the native commands the setup flow needs.

mod backend;
mod installation;
mod integration;

use tauri::Manager;

use backend::{BackendHandle, BackendInfo};

/// How long window bootstrap waits for the embedded backend.
const BACKEND_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Base URL and state of the embedded backend, awaited until ready.
#[tauri::command]
async fn backend_info(state: tauri::State<'_, BackendHandle>) -> Result<BackendInfo, String> {
    state.wait_ready(BACKEND_READY_TIMEOUT).await
}

/// Detect the installation state of the running executable.
#[tauri::command]
fn setup_installation_info() -> installation::InstallationInfo {
    installation::detect()
}

/// Apply the Windows integration selected during setup.
///
/// Runs on a blocking thread because shortcut creation shells out.
#[tauri::command]
async fn apply_windows_integration(
    request: integration::IntegrationRequest,
) -> Result<integration::IntegrationReport, String> {
    tauri::async_runtime::spawn_blocking(move || integration::apply(&request))
        .await
        .map_err(|e| e.to_string())
}

/// Begin the deterministic setup-to-main handoff: create the main window
/// hidden. The main window calls `main_window_ready` once its frontend has a
/// live backend connection, which shows it and closes the setup window.
#[tauri::command]
async fn finish_setup_handoff(app: tauri::AppHandle) -> Result<(), String> {
    if app.get_webview_window("main").is_some() {
        return Ok(());
    }
    create_main_window(&app, false).map_err(|e| e.to_string())?;
    Ok(())
}

/// Called by the main window frontend once it has verified the backend
/// connection. Shows the main window, focuses it, then closes setup.
#[tauri::command]
async fn main_window_ready(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window does not exist".to_owned())?;
    main.show().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    if let Some(setup) = app.get_webview_window("setup") {
        setup.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn create_setup_window(app: &tauri::AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    tauri::WebviewWindowBuilder::new(app, "setup", tauri::WebviewUrl::App("index.html".into()))
        .title("Ravyn Setup")
        .inner_size(760.0, 580.0)
        .min_inner_size(640.0, 500.0)
        .resizable(true)
        .maximizable(false)
        .center()
        .build()
}

fn create_main_window(
    app: &tauri::AppHandle,
    visible: bool,
) -> tauri::Result<tauri::WebviewWindow> {
    tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title("Ravyn")
        .inner_size(1100.0, 720.0)
        .min_inner_size(800.0, 560.0)
        .visible(visible)
        .center()
        .build()
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ravyn=info,ravyn_desktop=info".into()),
        )
        .init();

    let (handle, _receiver) = backend::start();

    let mut builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // MCP automation bridge for development-time testing only, loopback-bound.
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(
            tauri_plugin_mcp_bridge::Builder::new()
                .bind_address("127.0.0.1")
                .build(),
        );
    }

    builder
        .manage(handle.clone())
        .invoke_handler(tauri::generate_handler![
            backend_info,
            setup_installation_info,
            apply_windows_integration,
            finish_setup_handoff,
            main_window_ready,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let backend = handle.clone();
            // Open the correct first window once the backend reports state.
            tauri::async_runtime::spawn(async move {
                let setup_completed = match backend.wait_ready(BACKEND_READY_TIMEOUT).await {
                    Ok(info) => info.setup_completed,
                    Err(error) => {
                        // Without a backend the setup window still opens and
                        // surfaces the connection error to the user.
                        tracing::error!(%error, "backend not ready; opening setup window");
                        false
                    }
                };
                let result = if setup_completed {
                    create_main_window(&app_handle, true).map(|_| ())
                } else {
                    create_setup_window(&app_handle).map(|_| ())
                };
                if let Err(error) = result {
                    tracing::error!(%error, "failed to create the initial window");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Ravyn desktop application");
}
