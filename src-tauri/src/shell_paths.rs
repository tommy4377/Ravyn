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
        .map_err(|error| format!("the requested path is unavailable: {error}"))
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
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Windows Explorer could not reveal {}: {error}", path.display()))
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
}
