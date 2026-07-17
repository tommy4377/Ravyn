//! Startup guard for the Microsoft Edge WebView2 runtime.
//!
//! Ravyn ships as a single self-installing executable, so no installer
//! bootstraps WebView2 anymore. On the rare machine without the Evergreen
//! runtime, explain the requirement in a native dialog and open Microsoft's
//! download page instead of failing with an opaque webview error.

#[cfg(windows)]
const WEBVIEW2_DOWNLOAD_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// Returns true when the WebView2 runtime is available and startup may
/// continue. Otherwise informs the user and returns false.
pub fn ensure_available() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }
    #[cfg(windows)]
    {
        let choice = message_box(
            "Ravyn needs the Microsoft Edge WebView2 runtime",
            "Ravyn uses the Microsoft Edge WebView2 runtime, which is not \
             installed on this PC.\n\nOpen the Microsoft download page now? \
             After installing WebView2, start Ravyn again.",
        );
        const IDYES: i32 = 6;
        if choice == IDYES {
            let mut command = std::process::Command::new("rundll32.exe");
            command
                .arg("url.dll,FileProtocolHandler")
                .arg(WEBVIEW2_DOWNLOAD_URL);
            crate::silent_command::hide_console_window(&mut command);
            let _ = command.spawn();
        }
    }
    false
}

/// Minimal user32 Yes/No message box; avoids new dependencies for the one
/// dialog that must work before any webview exists.
#[cfg(windows)]
fn message_box(caption: &str, text: &str) -> i32 {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, utype: u32) -> i32;
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    const MB_YESNO: u32 = 0x0000_0004;
    const MB_ICONWARNING: u32 = 0x0000_0030;
    let text = wide(text);
    let caption = wide(caption);
    unsafe {
        MessageBoxW(
            0,
            text.as_ptr(),
            caption.as_ptr(),
            MB_YESNO | MB_ICONWARNING,
        )
    }
}
