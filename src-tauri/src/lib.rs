//! Ravyn desktop shell.
//!
//! Embeds the Ravyn backend in-process, decides whether to open the setup or
//! the main window, and exposes the native commands the setup flow needs.

mod app_updates;
mod appearance;
mod backend;
mod browser_integration;
mod installation;
mod integration;
mod native_messaging;
mod setup_guard;
mod shell_paths;
mod silent_command;
mod torrent_association;
mod tray;
mod uninstall;
mod webview_runtime;

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
    let consent = setup.integration_consent.ok_or_else(|| {
        "installation preferences must be confirmed before Windows integration".to_owned()
    })?;
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

    let result = prepare_setup_handoff(installed_exe, launch_after_setup);
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

    // Managed component paths are resolved when the backend starts. Portable
    // and development setups therefore need the same fresh-process handoff as
    // installed setups; otherwise newly provisioned media and torrent engines
    // remain unavailable until the user happens to restart manually.
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to resolve the Ravyn executable: {error}"))?;
    let working_directory = executable
        .parent()
        .ok_or_else(|| "Ravyn executable has no parent directory".to_owned())?;
    std::process::Command::new(&executable)
        .current_dir(working_directory)
        .spawn()
        .map_err(|error| format!("failed to launch the refreshed Ravyn process: {error}"))?;
    Ok(true)
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

/// Return the current Firefox native-messaging registration state.
#[tauri::command]
fn browser_integration_status(
    window: tauri::WebviewWindow,
) -> Result<browser_integration::BrowserIntegrationStatus, String> {
    require_window(&window, "main")?;
    Ok(browser_integration::status())
}

/// Repair the per-user Firefox native-messaging manifest and registry entry.
#[tauri::command]
async fn repair_browser_integration(
    window: tauri::WebviewWindow,
) -> Result<browser_integration::BrowserIntegrationStatus, String> {
    require_window(&window, "main")?;
    tauri::async_runtime::spawn_blocking(browser_integration::repair_for_current_executable)
        .await
        .map_err(|error| format!("the browser integration worker failed: {error}"))?
}

/// Remove the Firefox native-messaging registration for the current user.
#[tauri::command]
async fn remove_browser_integration(
    window: tauri::WebviewWindow,
) -> Result<browser_integration::BrowserIntegrationStatus, String> {
    require_window(&window, "main")?;
    tauri::async_runtime::spawn_blocking(browser_integration::unregister)
        .await
        .map_err(|error| format!("the browser integration worker failed: {error}"))?
}

/// Consume a browser action delivered before the main webview was ready.
#[tauri::command]
fn take_browser_action(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, browser_integration::BrowserActionState>,
) -> Result<Option<browser_integration::BrowserAction>, String> {
    require_window(&window, "main")?;
    Ok(state.take())
}

/// Register Ravyn as a candidate for torrent files and let Windows ask the
/// user to choose the default application.
#[tauri::command]
async fn prompt_torrent_default_app(window: tauri::WebviewWindow) -> Result<(), String> {
    require_window(&window, "main")?;
    tauri::async_runtime::spawn_blocking(torrent_association::register_and_prompt)
        .await
        .map_err(|error| format!("the torrent association worker failed: {error}"))?
}

/// Open an existing file in its Windows default application or open a folder.
#[tauri::command]
async fn open_native_path(window: tauri::WebviewWindow, path: String) -> Result<(), String> {
    require_window(&window, "main")?;
    tauri::async_runtime::spawn_blocking(move || shell_paths::open(&path))
        .await
        .map_err(|error| format!("the native open worker failed: {error}"))?
}

/// Reveal an existing file in Explorer, or open the directory itself.
#[tauri::command]
async fn reveal_native_path(window: tauri::WebviewWindow, path: String) -> Result<(), String> {
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
    if !matches!(window.label(), "main" | "setup" | "compact") {
        return Err("command is not available from this window".into());
    }
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
    if let Err(error) = tray::ensure(&app) {
        tracing::warn!(%error, "failed to create the system tray icon");
    }
    app_updates::confirm_update_readiness(&app)?;
    app_updates::start_background_check(app);
    Ok(())
}

/// Opens (or focuses) the compact download progress window. Called by the
/// main window when a transfer starts while Ravyn is minimized or unfocused.
///
/// Must be async: creating a webview from a synchronous command deadlocks
/// WebView2 initialization on Windows (wry#583), leaving a blank window and
/// stalling IPC for every other webview.
#[tauri::command]
async fn open_compact_window(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    require_window(&window, "main")?;
    if let Some(existing) = app.get_webview_window("compact") {
        existing.show().map_err(|error| error.to_string())?;
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "compact", tauri::WebviewUrl::App("index.html".into()))
        .title("Ravyn downloads")
        .inner_size(400.0, 130.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .decorations(false)
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Brings the main window to the foreground; used by the compact window.
#[tauri::command]
fn focus_main_window(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    if !matches!(window.label(), "main" | "compact") {
        return Err("command is not available from this window".into());
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window does not exist".to_owned())?;
    main.show().map_err(|error| error.to_string())?;
    main.unminimize().map_err(|error| error.to_string())?;
    main.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

/// Parses a browser-launch or torrent/magnet-association action out of a
/// process argument list, checking both forms since either can arrive on the
/// command line (a Firefox-launched relaunch vs. a Windows file/URL
/// association double-click).
fn parse_launch_action(arguments: &[String]) -> Option<browser_integration::BrowserAction> {
    browser_integration::parse_browser_action(arguments)
        .or_else(|| browser_integration::parse_torrent_association_action(arguments))
}

/// Brings whichever top-level window currently exists (main or setup) to the
/// foreground. Used when a second process launch is redirected here by the
/// single-instance plugin, so the OS focuses the running app instead of
/// leaving the user staring at nothing.
fn focus_main_window_or_setup(app: &tauri::AppHandle) {
    let window = app
        .get_webview_window("main")
        .or_else(|| app.get_webview_window("setup"));
    let Some(window) = window else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Show a native desktop notification for a download event. The message
/// content is provided by the main window, which owns the job metadata.
#[tauri::command]
fn notify_native(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    title: String,
    body: Option<String>,
) -> Result<(), String> {
    require_window(&window, "main")?;
    use tauri_plugin_notification::NotificationExt;
    let mut builder = app.notification().builder().title(title);
    if let Some(body) = body {
        builder = builder.body(body);
    }
    builder.show().map_err(|error| error.to_string())
}

fn create_setup_window(app: &tauri::AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let builder =
        tauri::WebviewWindowBuilder::new(app, "setup", tauri::WebviewUrl::App("index.html".into()))
            .title("Ravyn Setup")
            .inner_size(760.0, 580.0)
            .min_inner_size(640.0, 500.0)
            .resizable(true)
            .maximizable(false)
            .center();
    #[cfg(target_os = "windows")]
    let builder = builder.transparent(true).effects(
        tauri::window::EffectsBuilder::new()
            .effect(tauri::window::Effect::Acrylic)
            // The web layer owns the theme tint. A nearly transparent native
            // color keeps Windows 10 acrylic active without double-tinting it.
            .color(tauri::window::Color(0, 0, 0, 1))
            .build(),
    );
    builder.build()
}

fn create_main_window(
    app: &tauri::AppHandle,
    visible: bool,
) -> tauri::Result<tauri::WebviewWindow> {
    let builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("Ravyn")
            .inner_size(1100.0, 720.0)
            .min_inner_size(800.0, 560.0)
            .visible(visible)
            .center();
    #[cfg(target_os = "windows")]
    let builder = builder.transparent(true).effects(
        tauri::window::EffectsBuilder::new()
            .effect(tauri::window::Effect::Acrylic)
            // The web layer owns the theme tint. A nearly transparent native
            // color keeps Windows 10 acrylic active without double-tinting it.
            .color(tauri::window::Color(0, 0, 0, 1))
            .build(),
    );
    builder.build()
}

pub fn run() {
    if native_messaging::try_handle_command_line() {
        return;
    }
    if browser_integration::try_handle_command_line() {
        return;
    }
    if uninstall::try_handle_command_line() {
        return;
    }
    if !webview_runtime::ensure_available() {
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ravyn=info,ravyn_desktop=info".into()),
        )
        .init();

    let initial_arguments = std::env::args().collect::<Vec<_>>();
    let initial_browser_action = parse_launch_action(&initial_arguments);
    let browser_action_state = browser_integration::BrowserActionState::default();
    if let Some(action) = initial_browser_action {
        browser_action_state.replace(action);
    }

    let (handle, _receiver) = backend::start();

    #[allow(unused_mut)] // Mutable only when the debug-only MCP bridge is enabled.
    let mut builder = tauri::Builder::default()
        // Must be the first plugin registered: a magnet link or .torrent file
        // opened while Ravyn is already running launches a second OS process
        // (the registered "%1" handler in torrent_association.rs), which
        // would otherwise boot a fully redundant backend, rqbit child, and
        // window against the same database. This plugin detects that a
        // primary instance already owns the app and forwards the new
        // process's argv here instead, so the second process exits
        // immediately without ever starting Tauri.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(action) = parse_launch_action(&argv) {
                app.state::<browser_integration::BrowserActionState>()
                    .replace(action);
            }
            focus_main_window_or_setup(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());
    // MCP automation bridge for explicitly enabled development-time testing only.
    #[cfg(all(debug_assertions, feature = "mcp-automation"))]
    {
        builder = builder.plugin(
            tauri_plugin_mcp_bridge::Builder::new()
                .bind_address("127.0.0.1")
                .build(),
        );
    }

    builder
        .manage(handle.clone())
        .manage(browser_action_state)
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
            browser_integration_status,
            repair_browser_integration,
            remove_browser_integration,
            take_browser_action,
            prompt_torrent_default_app,
            open_native_path,
            reveal_native_path,
            notify_native,
            open_compact_window,
            focus_main_window,
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
            if crate::installation::current_executable_is_installed() {
                tauri::async_runtime::spawn_blocking(|| {
                    if let Err(error) = crate::browser_integration::repair_for_current_executable() {
                        tracing::warn!(%error, "failed to repair Firefox browser integration at startup");
                    }
                });
            }
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
