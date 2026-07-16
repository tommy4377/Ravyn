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
pub const INSTALL_SUBDIR: &str = r"Ravyn";

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
    /// SHA-256 of the running executable, when it could be read.
    pub exe_sha256: Option<String>,
}

pub fn detect() -> InstallationInfo {
    let executable = std::env::current_exe().ok();
    let exe_path = executable
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let exe_sha256 = executable
        .as_deref()
        .and_then(|path| sha256_file(path).ok());
    let (installed, installed_version, install_dir) = read_registration();

    let in_install_dir = match &install_dir {
        Some(dir) if !dir.is_empty() => path_is_within(&exe_path, dir),
        _ => default_install_dir()
            .map(|dir| path_is_within(&exe_path, &dir))
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
        exe_sha256,
    }
}

/// Compute the SHA-256 of an executable or staged application file.
pub fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Default installed-mode directory, e.g. `%LOCALAPPDATA%\Ravyn`.
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
    path.trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase()
}

/// Return whether an executable path is inside the supplied installation root.
/// Registry values written by NSIS may be quoted, so both values are sanitized
/// before the comparison.
pub fn path_is_within(path: &str, root: &str) -> bool {
    let path = normalized(path);
    let mut root = normalized(root);
    while root.ends_with('\\') {
        root.pop();
    }
    path == root || path.starts_with(&format!("{root}\\"))
}

/// Whether the current process is already running from Ravyn's trusted
/// per-user installation directory. This covers both the Tauri NSIS installer
/// and Ravyn's portable-to-installed handoff without relying solely on a
/// registry entry.
pub fn current_executable_is_installed() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let executable = executable.display().to_string();
    default_install_dir().is_some_and(|root| path_is_within(&executable, &root))
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

#[cfg(test)]
mod tests {
    use super::path_is_within;

    #[test]
    fn accepts_quoted_installer_paths_case_insensitively() {
        assert!(path_is_within(
            r#"C:\Users\Tommy\AppData\Local\Ravyn\Ravyn.exe"#,
            r#""C:\Users\Tommy\AppData\Local\Ravyn""#,
        ));
    }

    #[test]
    fn rejects_sibling_directories_with_the_same_prefix() {
        assert!(!path_is_within(
            r#"C:\Users\Tommy\AppData\Local\Ravyn-old\Ravyn.exe"#,
            r#"C:\Users\Tommy\AppData\Local\Ravyn"#,
        ));
    }

    #[test]
    fn treats_the_root_itself_as_inside() {
        assert!(path_is_within(
            r#"C:\Users\Tommy\AppData\Local\Ravyn"#,
            r#"C:\Users\Tommy\AppData\Local\Ravyn\"#,
        ));
    }
}
