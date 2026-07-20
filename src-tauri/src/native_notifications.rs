//! Native operating-system notifications exposed through Tauri.
//!
//! The frontend owns in-app notification history. This module is reserved for
//! native desktop notifications that should remain visible when Ravyn is not
//! focused, such as terminal download states or a silently staged update.

use tauri::{Manager, plugin::PermissionState};
use tauri_plugin_notification::NotificationExt;

const MAX_TITLE_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 1_000;

/// Shows a native notification after resolving the platform permission state.
/// Returns `false` when the user or operating system denied notifications.
pub fn show(app: &tauri::AppHandle, title: &str, body: Option<&str>) -> Result<bool, String> {
    let title = sanitize_text(title, MAX_TITLE_CHARS, "notification title")?;
    let body = body
        .map(|value| sanitize_text(value, MAX_BODY_CHARS, "notification body"))
        .transpose()?;

    let manager = app.notification();
    let mut permission = manager
        .permission_state()
        .map_err(|error| format!("failed to read notification permission: {error}"))?;
    if permission == PermissionState::Unknown {
        manager
            .request_permission()
            .map_err(|error| format!("failed to request notification permission: {error}"))?;
        permission = manager
            .permission_state()
            .map_err(|error| format!("failed to refresh notification permission: {error}"))?;
    }
    if permission != PermissionState::Granted {
        return Ok(false);
    }

    let mut builder = manager.builder().title(title);
    if let Some(body) = body {
        builder = builder.body(body);
    }
    builder
        .show()
        .map_err(|error| format!("failed to show native notification: {error}"))?;
    Ok(true)
}

/// Shows a native notification only when the primary Ravyn window is absent
/// or not focused, avoiding duplicate in-app and system toasts while the user
/// is actively looking at Ravyn.
pub fn show_if_background(
    app: &tauri::AppHandle,
    title: &str,
    body: Option<&str>,
) -> Result<bool, String> {
    let focused = app
        .get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    if focused {
        return Ok(false);
    }
    show(app, title, body)
}

fn sanitize_text(value: &str, max_chars: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label} cannot be empty"));
    }
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(max_chars + 1)
        .collect::<String>();
    if sanitized.chars().count() > max_chars {
        return Err(format!("{label} exceeds {max_chars} characters"));
    }
    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_rejects_oversized_notification_text() {
        assert_eq!(sanitize_text("  Ready  ", 10, "title").unwrap(), "Ready");
        assert!(sanitize_text("123456", 5, "title").is_err());
    }

    #[test]
    fn strips_non_whitespace_control_characters() {
        assert_eq!(sanitize_text("A\u{0007}B\nC", 10, "body").unwrap(), "AB\nC");
    }
}
