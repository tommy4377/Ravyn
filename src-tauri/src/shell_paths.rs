//! Small, main-window-only integrations with Windows Explorer and file handlers.

use std::path::{Path, PathBuf};

pub fn open(path: &str) -> Result<(), String> {
    let path = validate_existing_path(path)?;
    platform_open(&path)
}

pub fn reveal(path: &str) -> Result<(), String> {
    let path = validate_existing_path(path)?;
    platform_reveal(&path)
}

fn validate_existing_path(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("no file or folder path was provided".into());
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        return Err("Ravyn can only open absolute local paths".into());
    }
    path.canonicalize()
        .map(|canonical| strip_verbatim_prefix(&canonical))
        .map_err(|error| format!("the requested path is unavailable: {error}"))
}

/// `canonicalize` on Windows returns `\\?\C:\...` verbatim paths, which
/// `explorer.exe /select,` does not understand — Explorer then silently
/// falls back to opening the default (Documents) folder. Strip the prefix
/// so shell integrations receive a regular Win32 path.
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(target_os = "windows")]
fn platform_open(path: &Path) -> Result<(), String> {
    let mut command = if path.is_dir() {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(path);
        command
    } else {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(path);
        command
    };
    crate::silent_command::hide_console_window(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Windows could not open {}: {error}", path.display()))
}

#[cfg(target_os = "windows")]
fn platform_reveal(path: &Path) -> Result<(), String> {
    let mut command = std::process::Command::new("explorer.exe");
    if path.is_dir() {
        command.arg(path);
    } else {
        command.arg(format!("/select,{}", path.display()));
    }
    crate::silent_command::hide_console_window(&mut command);
    command.spawn().map(|_| ()).map_err(|error| {
        format!(
            "Windows Explorer could not reveal {}: {error}",
            path.display()
        )
    })
}

#[cfg(not(target_os = "windows"))]
fn platform_open(_path: &Path) -> Result<(), String> {
    Err("native file opening is currently available only on Windows".into())
}

#[cfg(not(target_os = "windows"))]
fn platform_reveal(_path: &Path) -> Result<(), String> {
    Err("native folder reveal is currently available only on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_paths_before_platform_dispatch() {
        assert!(validate_existing_path("relative/file.txt").is_err());
    }

    #[test]
    fn rejects_empty_paths() {
        assert!(validate_existing_path("  ").is_err());
    }

    #[test]
    fn strips_verbatim_prefixes_for_explorer() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\C:\Users\demo\file.bin")),
            PathBuf::from(r"C:\Users\demo\file.bin")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share\file.bin")),
            PathBuf::from(r"\\server\share\file.bin")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"C:\plain\path")),
            PathBuf::from(r"C:\plain\path")
        );
    }
}
