//! Firefox native-messaging host registration and browser-launch actions.
//!
//! The native host is the installed Ravyn executable itself. Firefox starts a
//! short-lived second process in native-messaging mode while the regular Ravyn
//! desktop process owns the authenticated loopback backend.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const HOST_NAME: &str = "com.ravyn.download_manager";
pub const EXTENSION_ID: &str = "firefox-extension@ravyn.app";
pub const HOST_MANIFEST_FILE: &str = "com.ravyn.download_manager.json";
const ACTION_FILE: &str = "browser-action.json";

#[derive(Debug, Clone, Serialize)]
pub struct BrowserIntegrationStatus {
    pub supported: bool,
    pub registered: bool,
    pub host_name: String,
    pub extension_id: String,
    pub manifest_path: Option<String>,
    pub executable_path: Option<String>,
    pub installed_mode: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct BrowserAction {
    pub section: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Default)]
pub struct BrowserActionState(Mutex<Option<BrowserAction>>);

impl BrowserActionState {
    pub fn replace(&self, action: BrowserAction) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = Some(action);
        }
    }

    pub fn take(&self) -> Option<BrowserAction> {
        self.0
            .lock()
            .ok()
            .and_then(|mut pending| pending.take())
            .or_else(take_published_action)
    }
}

pub fn publish_action(action: &BrowserAction) -> Result<(), String> {
    let path = action_path();
    let parent = path
        .parent()
        .ok_or_else(|| "the browser action path has no parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create the browser action directory: {error}"))?;
    let bytes = serde_json::to_vec(action)
        .map_err(|error| format!("failed to serialize the browser action: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("failed to write the browser action: {error}"))?;
    restrict_action_file(&temporary)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|error| format!("failed to replace the browser action: {error}"))?;
    }
    std::fs::rename(&temporary, &path)
        .map_err(|error| format!("failed to publish the browser action: {error}"))
}

fn take_published_action() -> Option<BrowserAction> {
    let path = action_path();
    let bytes = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(path);
    serde_json::from_slice(&bytes).ok()
}

fn action_path() -> PathBuf {
    crate::backend::resolve_data_dir()
        .join("runtime")
        .join(ACTION_FILE)
}

#[cfg(unix)]
fn restrict_action_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to restrict the browser action file: {error}"))
}

#[cfg(not(unix))]
fn restrict_action_file(_path: &Path) -> Result<(), String> {
    Ok(())
}


/// Handles explicit installer lifecycle commands without starting Tauri.
pub fn try_handle_command_line() -> bool {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    let Some(register_requested) = integration_command(&arguments) else {
        return false;
    };
    let result = if register_requested {
        repair_for_current_executable().map(|_| ())
    } else {
        unregister().map(|_| ())
    };
    if let Err(error) = result {
        eprintln!("Ravyn Firefox integration command failed: {error}");
    }
    true
}

fn integration_command(arguments: &[std::ffi::OsString]) -> Option<bool> {
    if arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--register-firefox-native-host")
    {
        return Some(true);
    }
    arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--unregister-firefox-native-host")
        .then_some(false)
}

pub fn parse_browser_action(arguments: &[String]) -> Option<BrowserAction> {
    let requested = arguments
        .iter()
        .any(|argument| argument == "--browser-action" || argument.starts_with("--browser-section="));
    if !requested {
        return None;
    }
    let section = arguments.iter().find_map(|argument| {
        argument
            .strip_prefix("--browser-section=")
            .map(sanitize_section)
            .filter(|value| !value.is_empty())
    });
    let source_url = arguments.iter().find_map(|argument| {
        argument
            .strip_prefix("--browser-source=")
            .and_then(sanitize_source_url)
    });
    Some(BrowserAction { section, source_url })
}

fn sanitize_section(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "downloads" | "library" | "media" | "torrents" | "automation" | "components"
        | "settings" => value.trim().to_ascii_lowercase(),
        _ => "downloads".into(),
    }
}

fn sanitize_source_url(value: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .ok()?
        .into_owned();
    let parsed = url::Url::parse(&decoded).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.to_string())
}

pub fn status() -> BrowserIntegrationStatus {
    let executable = std::env::current_exe().ok();
    let manifest = manifest_path();
    let installed_mode = crate::installation::current_executable_is_installed();
    match (&manifest, &executable) {
        (Some(manifest), Some(executable)) => {
            let registered = registration_matches(manifest, executable).unwrap_or(false);
            BrowserIntegrationStatus {
                supported: true,
                registered,
                host_name: HOST_NAME.into(),
                extension_id: EXTENSION_ID.into(),
                manifest_path: Some(manifest.display().to_string()),
                executable_path: Some(executable.display().to_string()),
                installed_mode,
                error: None,
            }
        }
        _ => BrowserIntegrationStatus {
            supported: false,
            registered: false,
            host_name: HOST_NAME.into(),
            extension_id: EXTENSION_ID.into(),
            manifest_path: manifest.map(|path| path.display().to_string()),
            executable_path: executable.map(|path| path.display().to_string()),
            installed_mode,
            error: Some("Firefox native messaging is unavailable on this platform or environment".into()),
        },
    }
}

pub fn repair_for_current_executable() -> Result<BrowserIntegrationStatus, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to resolve the Ravyn executable: {error}"))?;
    if !crate::installation::current_executable_is_installed() {
        return Err("browser integration requires an installed Ravyn executable".into());
    }
    register(&executable)?;
    Ok(status())
}

pub fn register(executable: &Path) -> Result<(), String> {
    if !executable.is_file() {
        return Err("the native-messaging executable does not exist".into());
    }
    let manifest = manifest_path()
        .ok_or_else(|| "cannot resolve the Firefox native-messaging manifest path".to_owned())?;
    let parent = manifest
        .parent()
        .ok_or_else(|| "the Firefox native-messaging manifest has no parent directory".to_owned())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create the native-messaging directory: {error}"))?;
    let body = serde_json::json!({
        "name": HOST_NAME,
        "description": "Ravyn Firefox integration host",
        "path": executable,
        "type": "stdio",
        "allowed_extensions": [EXTENSION_ID]
    });
    let bytes = serde_json::to_vec_pretty(&body)
        .map_err(|error| format!("failed to serialize the native-messaging manifest: {error}"))?;
    let temporary = manifest.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|error| format!("failed to write the native-messaging manifest: {error}"))?;
    if manifest.exists() {
        std::fs::remove_file(&manifest)
            .map_err(|error| format!("failed to replace the native-messaging manifest: {error}"))?;
    }
    std::fs::rename(&temporary, &manifest)
        .map_err(|error| format!("failed to activate the native-messaging manifest: {error}"))?;
    register_manifest_location(&manifest)?;
    if !registration_matches(&manifest, executable)? {
        return Err("Firefox native-messaging registration verification failed".into());
    }
    Ok(())
}

pub fn unregister() -> Result<BrowserIntegrationStatus, String> {
    unregister_manifest_location()?;
    if let Some(path) = manifest_path() {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to remove the native-messaging manifest: {error}")),
        }
    }
    Ok(status())
}

pub fn manifest_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Ravyn").join("browser").join(HOST_MANIFEST_FILE))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".mozilla").join("native-messaging-hosts").join(HOST_MANIFEST_FILE))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join("Library")
                .join("Application Support")
                .join("Mozilla")
                .join("NativeMessagingHosts")
                .join(HOST_MANIFEST_FILE)
        })
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

fn registration_matches(manifest: &Path, executable: &Path) -> Result<bool, String> {
    if !manifest.is_file() {
        return Ok(false);
    }
    let bytes = std::fs::read(manifest)
        .map_err(|error| format!("failed to read the native-messaging manifest: {error}"))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse the native-messaging manifest: {error}"))?;
    let manifest_executable = value.get("path").and_then(serde_json::Value::as_str);
    let extension_allowed = value
        .get("allowed_extensions")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(EXTENSION_ID)));
    let body_matches = value.get("name").and_then(serde_json::Value::as_str) == Some(HOST_NAME)
        && manifest_executable.is_some_and(|path| same_path(Path::new(path), executable))
        && extension_allowed;
    Ok(body_matches && registration_location_matches(manifest)?)
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .to_string_lossy()
            .replace('/', "\\")
            .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\")),
    }
}

#[cfg(windows)]
fn register_manifest_location(manifest: &Path) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"Software\Mozilla\NativeMessagingHosts\{HOST_NAME}");
    let (key, _) = hkcu.create_subkey(key_path).map_err(|error| error.to_string())?;
    key.set_value("", &manifest.display().to_string())
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn register_manifest_location(_manifest: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn unregister_manifest_location() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"Software\Mozilla\NativeMessagingHosts\{HOST_NAME}");
    match hkcu.delete_subkey_all(key_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(not(windows))]
fn unregister_manifest_location() -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn registration_location_matches(manifest: &Path) -> Result<bool, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"Software\Mozilla\NativeMessagingHosts\{HOST_NAME}");
    let key = match hkcu.open_subkey(key_path) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    let registered: String = key.get_value("").map_err(|error| error.to_string())?;
    Ok(same_path(Path::new(&registered), manifest))
}

#[cfg(not(windows))]
fn registration_location_matches(_manifest: &Path) -> Result<bool, String> {
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_installer_lifecycle_commands() {
        assert_eq!(
            integration_command(&[
                "Ravyn.exe".into(),
                "--register-firefox-native-host".into(),
            ]),
            Some(true)
        );
        assert_eq!(
            integration_command(&[
                "Ravyn.exe".into(),
                "--unregister-firefox-native-host".into(),
            ]),
            Some(false)
        );
        assert_eq!(integration_command(&["Ravyn.exe".into()]), None);
    }

    #[test]
    fn browser_action_rejects_unknown_sections() {
        let action = parse_browser_action(&[
            "Ravyn".into(),
            "--browser-section=unexpected".into(),
        ])
        .unwrap();
        assert_eq!(action.section.as_deref(), Some("downloads"));
    }

    #[test]
    fn browser_action_accepts_http_source() {
        let action = parse_browser_action(&[
            "Ravyn".into(),
            "--browser-action".into(),
            "--browser-source=https%3A%2F%2Fexample.com%2Fvideo".into(),
        ])
        .unwrap();
        assert_eq!(action.source_url.as_deref(), Some("https://example.com/video"));
    }
}
