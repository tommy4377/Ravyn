//! Ravyn desktop shell.
//!
//! Embeds the Ravyn backend in-process, decides whether to open the setup or
//! the main window, and exposes the native commands the setup flow needs.

mod app_updates;
mod appearance;
mod backend;
mod installation;
mod integration;
mod setup_guard;
mod shell_paths;
mod uninstall;

use tauri::Manager;

use backend::{BackendHandle, BackendInfo};

/// How long window bootstrap waits for the embedded backend.
const BACKEND_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Base URL and state of the embedded backend, awaited until ready.
#[tauri::command]
async fn backend_info(state: tauri::State<'_, BackendHandle>) -> Result<BackendInfo, String> {
    state.wait_ready(BACKEND_READY_TIMEOUT).await
}

#[derive(serde::Deserialize)]
struct BackendSetupState {
    completed: bool,
    integration_consent: Option<BackendIntegrationConsent>,
    installation: Option<BackendInstallationState>,
}

#[derive(serde::Deserialize)]
struct BackendInstallationState {
    integration_completed: bool,
}

#[derive(serde::Deserialize)]
struct BackendIntegrationConsent {
    installation_mode: String,
    install_application: bool,
    register_installed_app: bool,
    start_menu_shortcut: bool,
    desktop_shortcut: bool,
    launch_at_startup: bool,
}

/// Read the backend setup state through the same authenticated loopback API
/// used by the webview. This keeps the database-backed lifecycle authoritative
/// even when a compromised setup page attempts to call native commands out of
/// order.
async fn backend_setup_state(state: &BackendHandle) -> Result<BackendSetupState, String> {
    let info = state.wait_ready(BACKEND_READY_TIMEOUT).await?;
    let response = reqwest::Client::new()
        .get(format!("{}/v1/setup", info.base_url))
        .bearer_auth(info.api_token)
        .send()
        .await
        .map_err(|error| format!("failed to read the backend setup state: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "the backend rejected the setup-state check with HTTP {}",
            response.status()
        ));
    }
    response
        .json::<BackendSetupState>()
        .await
        .map_err(|error| format!("failed to decode the backend setup state: {error}"))
}

async fn backend_setup_completed(state: &BackendHandle) -> Result<bool, String> {
    Ok(backend_setup_state(state).await?.completed)
}

async fn require_backend_setup_state(
    state: &BackendHandle,
    expected_completed: bool,
) -> Result<(), String> {
    let completed = backend_setup_completed(state).await?;
    if completed == expected_completed {
        Ok(())
    } else if expected_completed {
        Err("setup must be completed in the backend before opening Ravyn".into())
    } else {
        Err("setup has already been completed in the backend".into())
    }
}

async fn require_backend_integration_consent(
    state: &BackendHandle,
    request: &integration::IntegrationRequest,
) -> Result<(), String> {
    let setup = backend_setup_state(state).await?;
    if setup.completed {
        return Err("setup has already been completed in the backend".into());
    }
    if setup
        .installation
        .as_ref()
        .is_some_and(|installation| installation.integration_completed)
    {
        return Err("Windows integration has already been verified for this setup".into());
    }
    let consent = setup
        .integration_consent
        .ok_or_else(|| "installation preferences must be confirmed before Windows integration".to_owned())?;
    if consent.installation_mode != "installed"
        || consent.install_application != request.install_application
        || consent.register_installed_app != request.register_installed_app
        || consent.start_menu_shortcut != request.start_menu_shortcut
        || consent.desktop_shortcut != request.desktop_shortcut
        || consent.launch_at_startup != request.launch_at_startup
    {
        return Err(
            "the native integration request does not match the persisted setup consent".into(),
        );
    }
    Ok(())
}

/// Detect the installation state of the running executable.
#[tauri::command]
async fn setup_installation_info(
    window: tauri::WebviewWindow,
    backend: tauri::State<'_, BackendHandle>,
    guard: tauri::State<'_, setup_guard::SetupCommandGuard>,
) -> Result<installation::InstallationInfo, String> {
    require_window(&window, "setup")?;
    guard.ensure_setup_window_allowed()?;
    require_backend_setup_state(&backend, false).await?;
    Ok(installation::detect())
}

/// Apply the Windows integration selected during setup.
///
/// Runs on a blocking thread because shortcut creation shells out.
#[tauri::command]
async fn apply_windows_integration(
    window: tauri::WebviewWindow,
    request: integration::IntegrationRequest,
    backend: tauri::State<'_, BackendHandle>,
    guard: tauri::State<'_, setup_guard::SetupCommandGuard>,
) -> Result<integration::IntegrationReport, String> {
    require_window(&window, "setup")?;
    guard.ensure_setup_window_allowed()?;
    require_backend_integration_consent(&backend, &request).await?;
    guard.begin_integration()?;

    let result = tauri::async_runtime::spawn_blocking(move || integration::apply(&request))
        .await
        .map_err(|error| error.to_string());
    let completed = result
        .as_ref()
        .is_ok_and(|report| report.integration_completed);
    guard.finish_integration(completed)?;
    result
}

/// Complete setup by launching the verified installed copy when one was
/// created. The installed process boots a fresh backend before it opens its
/// main window, so newly provisioned engine paths and library settings are
/// deterministically applied.
#[tauri::command]
async fn finish_setup_handoff(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    installed_exe: Option<String>,
    launch_after_setup: bool,
    backend: tauri::State<'_, BackendHandle>,
    guard: tauri::State<'_, setup_guard::SetupCommandGuard>,
) -> Result<(), String> {
    require_window(&window, "setup")?;
    guard.ensure_setup_window_allowed()?;
    require_backend_setup_state(&backend, true).await?;
    guard.begin_handoff()?;

    let result = prepare_setup_handoff(&app, installed_exe, launch_after_setup);
    guard.finish_handoff(result.is_ok())?;
    let should_exit = result?;
    if should_exit {
        app.exit(0);
    }
    Ok(())
}

/// Prepare the setup handoff and report whether the current process should
/// exit after the guard has committed the transition.
fn prepare_setup_handoff(
    app: &tauri::AppHandle,
    installed_exe: Option<String>,
    launch_after_setup: bool,
) -> Result<bool, String> {
    if !launch_after_setup {
        return Ok(true);
    }
    if let Some(installed_exe) = installed_exe {
        let expected = installation::default_install_dir()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| "installed-copy handoff is only supported on Windows".to_owned())?
            .join("Ravyn.exe");
        let supplied = std::path::PathBuf::from(installed_exe);
        if !same_path(&expected, &supplied) || !expected.is_file() {
            return Err(
                "the setup handoff target is not the verified installed Ravyn executable".into(),
            );
        }
        let working_directory = expected
            .parent()
            .ok_or_else(|| "installed executable has no parent directory".to_owned())?;
        std::process::Command::new(&expected)
            .current_dir(working_directory)
            .spawn()
            .map_err(|error| format!("failed to launch the installed Ravyn copy: {error}"))?;
        return Ok(true);
    }

    // Portable/development mode remains in the current process.
    if app.get_webview_window("main").is_none() {
        create_main_window(app, false).map_err(|error| error.to_string())?;
    }
    Ok(false)
}

fn require_window(window: &tauri::WebviewWindow, expected: &str) -> Result<(), String> {
    if window.label() == expected {
        Ok(())
    } else {
        Err(format!(
            "command is only available to the {expected} window"
        ))
    }
}

/// Restart the desktop process so persisted settings that require a fresh
/// backend configuration can take effect during setup.
#[tauri::command]
async fn restart_application(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    backend: tauri::State<'_, BackendHandle>,
    guard: tauri::State<'_, setup_guard::SetupCommandGuard>,
) -> Result<(), String> {
    require_window(&window, "setup")?;
    guard.ensure_setup_window_allowed()?;
    require_backend_setup_state(&backend, false).await?;
    guard.begin_restart()?;

    let result = (|| {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to resolve the Ravyn executable: {error}"))?;
        let working_directory = executable
            .parent()
            .ok_or_else(|| "the Ravyn executable has no parent directory".to_owned())?;
        std::process::Command::new(&executable)
            .current_dir(working_directory)
            .spawn()
            .map_err(|error| format!("failed to restart Ravyn: {error}"))?;
        Ok::<(), String>(())
    })();
    guard.finish_restart(result.is_ok())?;
    result?;
    app.exit(0);
    Ok(())
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\")),
    }
}



/// Open an existing file in its Windows default application or open a folder.
#[tauri::command]
async fn open_native_path(
    window: tauri::WebviewWindow,
    path: String,
) -> Result<(), String> {
    require_window(&window, "main")?;
    tauri::async_runtime::spawn_blocking(move || shell_paths::open(&path))
        .await
        .map_err(|error| format!("the native open worker failed: {error}"))?
}

/// Reveal an existing file in Explorer, or open the directory itself.
#[tauri::command]
async fn reveal_native_path(
    window: tauri::WebviewWindow,
    path: String,
) -> Result<(), String> {
    require_window(&window, "main")?;
    tauri::async_runtime::spawn_blocking(move || shell_paths::reveal(&path))
        .await
        .map_err(|error| format!("the Explorer worker failed: {error}"))?
}

/// Return the Windows wallpaper and accent metadata used by the synthetic backdrop.
#[tauri::command]
async fn desktop_appearance(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<appearance::DesktopAppearance, String> {
    require_window(&window, "main")?;
    appearance::read(app, window).await
}

/// Return the current silent application-update state.
#[tauri::command]
fn app_update_status(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<app_updates::AppUpdateStatus, String> {
    require_window(&window, "main")?;
    app_updates::status(&app)
}

/// Manually recheck the signed application-update feed.
#[tauri::command]
async fn check_app_update(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<app_updates::AppUpdateStatus, String> {
    require_window(&window, "main")?;
    app_updates::check_now(app).await
}

/// Stage a signed installer for the current release so missing or corrupted
/// installed files can be replaced on the next normal close.
#[tauri::command]
async fn repair_application(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<app_updates::AppUpdateStatus, String> {
    require_window(&window, "main")?;
    app_updates::repair_now(app).await
}

/// Cancel an active update check/download or discard the staged installer.
#[tauri::command]
fn cancel_app_update(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<app_updates::AppUpdateStatus, String> {
    require_window(&window, "main")?;
    app_updates::cancel(&app)
}

/// Apply the staged installer immediately using the same detached helper that
/// normally runs after a regular close.
#[tauri::command]
fn install_app_update_now(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    require_window(&window, "main")?;
    if !app_updates::install_pending_on_close(&app)? {
        return Err("no verified application update is ready to install".into());
    }
    // Return the IPC response before terminating the webview so the frontend
    // does not misreport the intentional restart as a command failure.
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        app.exit(0);
    });
    Ok(())
}

/// Called by the main window frontend once it has verified the backend
/// connection. Shows the main window, focuses it, then closes setup.
#[tauri::command]
async fn main_window_ready(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    require_window(&window, "main")?;
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window does not exist".to_owned())?;
    main.show().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    if let Some(setup) = app.get_webview_window("setup") {
        setup.close().map_err(|e| e.to_string())?;
    }
    app_updates::confirm_update_readiness(&app)?;
    app_updates::start_background_check(app);
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
    if uninstall::try_handle_command_line() {
        return;
    }
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
        .manage(setup_guard::SetupCommandGuard::default())
        .manage(app_updates::AppUpdateState::default())
        .invoke_handler(tauri::generate_handler![
            backend_info,
            setup_installation_info,
            apply_windows_integration,
            finish_setup_handoff,
            restart_application,
            main_window_ready,
            app_update_status,
            check_app_update,
            repair_application,
            cancel_app_update,
            install_app_update_now,
            desktop_appearance,
            open_native_path,
            reveal_native_path,
        ])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                match app_updates::install_pending_on_close(window.app_handle()) {
                    Ok(true) => {
                        api.prevent_close();
                        window.app_handle().exit(0);
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::error!(%error, "failed to schedule the staged app update");
                    }
                }
            }
        })
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
                if let Err(error) = app_handle
                    .state::<setup_guard::SetupCommandGuard>()
                    .initialize(setup_completed)
                {
                    tracing::error!(%error, "failed to initialize the native setup guard");
                    return;
                }
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
