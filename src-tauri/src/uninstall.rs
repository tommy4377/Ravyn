//! Windows uninstall lifecycle for the registered `Ravyn.exe --uninstall` command.

/// Handles the early command-line uninstall path. Returns an exit code when
/// the caller must exit instead of starting the Tauri application.
pub fn try_handle_command_line() -> Option<i32> {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    if !arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--uninstall")
    {
        return None;
    }
    let purge_data = arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--purge-data");
    match uninstall(purge_data) {
        Ok(()) => Some(0),
        Err(error) => {
            eprintln!("Ravyn uninstall failed: {error}");
            Some(1)
        }
    }
}

#[cfg(windows)]
fn uninstall(purge_data: bool) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut errors = Vec::new();

    if let Err(error) = crate::browser_integration::unregister() {
        errors.push(format!("Firefox native messaging: {error}"));
    }
    if let Err(error) = crate::torrent_association::unregister() {
        errors.push(format!("torrent/magnet associations: {error}"));
    }
    if let Err(error) = hkcu.delete_subkey_all(crate::installation::UNINSTALL_KEY)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        errors.push(format!("Installed Apps registration: {error}"));
    }
    match hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        KEY_WRITE,
    ) {
        Ok(run) => {
            if let Err(error) = run.delete_value("Ravyn")
                && error.kind() != std::io::ErrorKind::NotFound
            {
                errors.push(format!("startup registration: {error}"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => errors.push(format!("startup registry key: {error}")),
    }

    if let Err(error) = remove_shortcuts() {
        errors.push(format!("shortcuts: {error}"));
    }
    if purge_data {
        let data_dir = crate::backend::resolve_data_dir();
        if data_dir.exists()
            && let Err(error) = std::fs::remove_dir_all(&data_dir)
        {
            errors.push(format!("data directory {}: {error}", data_dir.display()));
        }
    }
    if let Err(error) = schedule_self_delete(&executable) {
        errors.push(format!("self-delete helper: {error}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "uninstall completed with cleanup errors: {}",
            errors.join("; ")
        ))
    }
}

#[cfg(not(windows))]
fn uninstall(_purge_data: bool) -> Result<(), String> {
    Err("uninstall is only supported on Windows".into())
}

#[cfg(windows)]
fn remove_shortcuts() -> Result<(), String> {
    let mut links = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        links.push(
            std::path::PathBuf::from(appdata)
                .join(r"Microsoft\Windows\Start Menu\Programs\Ravyn.lnk"),
        );
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        links.push(std::path::PathBuf::from(profile).join(r"Desktop\Ravyn.lnk"));
    }
    let mut errors = Vec::new();
    for link in links {
        match std::fs::remove_file(&link) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => errors.push(format!("{}: {error}", link.display())),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(windows)]
fn schedule_self_delete(executable: &std::path::Path) -> Result<(), String> {
    let path = executable.display().to_string().replace('\'', "''");
    // PowerShell is spawned independently, waits for this process to exit,
    // then deletes the binary with a literal path (no shell interpolation).
    let script = format!(
        "Start-Sleep -Seconds 2; Remove-Item -LiteralPath '{path}' -Force -ErrorAction SilentlyContinue"
    );
    let mut command = std::process::Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);
    crate::silent_command::hide_console_window(&mut command);
    command.spawn().map_err(|error| error.to_string())?;
    Ok(())
}
