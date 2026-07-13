//! Installation detection for the Ravyn setup.
//!
//! Determines whether Ravyn is registered as an installed application, where
//! the running executable lives, and whether this launch is portable. The
//! setup frontend combines this with the backend setup state to pick the
//! install / update / repair / first-run mode.

use serde::Serialize;

/// Registry path (under HKCU) where installed-mode Ravyn registers itself.
#[cfg(windows)]
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Ravyn";

/// Default installed-mode application directory below `%LOCALAPPDATA%`.
pub const INSTALL_SUBDIR: &str = r"Programs\Ravyn";

#[derive(Debug, Clone, Serialize)]
pub struct InstallationInfo {
    /// Version of the running executable.
    pub app_version: String,
    /// Full path of the running executable.
    pub exe_path: String,
    /// Whether Ravyn is registered in Windows Installed Apps.
    pub installed: bool,
    /// Version recorded in the Installed Apps registration, when present.
    pub installed_version: Option<String>,
    /// Install location recorded in the registration, when present.
    pub install_dir: Option<String>,
    /// Whether the running executable is outside the installed location.
    pub portable: bool,
    /// True for debug builds running from the development tree.
    pub development: bool,
}

pub fn detect() -> InstallationInfo {
    let exe_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let (installed, installed_version, install_dir) = read_registration();

    let in_install_dir = match &install_dir {
        Some(dir) if !dir.is_empty() => normalized(&exe_path).starts_with(&normalized(dir)),
        _ => default_install_dir()
            .map(|dir| normalized(&exe_path).starts_with(&normalized(&dir)))
            .unwrap_or(false),
    };

    InstallationInfo {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        exe_path,
        installed,
        installed_version,
        install_dir,
        portable: !in_install_dir,
        development: cfg!(debug_assertions),
    }
}

/// Default installed-mode directory, e.g. `%LOCALAPPDATA%\Programs\Ravyn`.
pub fn default_install_dir() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|local| format!("{local}\\{INSTALL_SUBDIR}"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn normalized(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

#[cfg(windows)]
fn read_registration() -> (bool, Option<String>, Option<String>) {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(UNINSTALL_KEY) {
        Ok(key) => {
            let version: Option<String> = key.get_value("DisplayVersion").ok();
            let location: Option<String> = key.get_value("InstallLocation").ok();
            (true, version, location)
        }
        Err(_) => (false, None, None),
    }
}

#[cfg(not(windows))]
fn read_registration() -> (bool, Option<String>, Option<String>) {
    (false, None, None)
}
