//! System tray icon with quick download controls for the main window.

use tauri::{
    AppHandle, Emitter, Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const TRAY_ID: &str = "ravyn-tray";

/// Event delivered to the main webview when a tray control is used; the
/// payload is the action name ("pause-all" or "resume-all"). The webview owns
/// the authenticated backend client, so it executes the bulk action itself.
pub const TRAY_ACTION_EVENT: &str = "ravyn://tray-action";

/// Creates the tray icon once the main window exists. Safe to call again —
/// an existing tray icon is kept.
pub fn ensure(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let open = MenuItemBuilder::with_id("tray-open", "Open Ravyn").build(app)?;
    let pause = MenuItemBuilder::with_id("tray-pause-all", "Pause all downloads").build(app)?;
    let resume = MenuItemBuilder::with_id("tray-resume-all", "Resume all downloads").build(app)?;
    let quit = MenuItemBuilder::with_id("tray-quit", "Quit Ravyn").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .separator()
        .item(&pause)
        .item(&resume)
        .separator()
        .item(&quit)
        .build()?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Ravyn")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-open" => show_main_window(app),
            "tray-pause-all" => forward_action(app, "pause-all"),
            "tray-resume-all" => forward_action(app, "resume-all"),
            "tray-quit" => {
                // Route through the same staged-update path as a window close.
                if let Err(error) = crate::app_updates::install_pending_on_close(app) {
                    tracing::error!(%error, "failed to schedule the staged app update");
                }
                crate::request_graceful_exit(app, 0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn forward_action(app: &AppHandle, action: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit(TRAY_ACTION_EVENT, action);
    }
}
