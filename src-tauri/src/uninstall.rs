//! Windows uninstall lifecycle for the registered `Ravyn.exe --uninstall` command.

/// Handles the early command-line uninstall path. Returns `true` when the
/// caller must exit instead of starting the Tauri application.
pub fn try_handle_command_line() -> bool {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    if !arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--uninstall")
    {
        return false;
    }
    let purge_data = arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--purge-data");
    if let Err(error) = uninstall(purge_data) {
        eprintln!("Ravyn uninstall failed: {error}");
    }
    true
}

#[cfg(windows)]
fn uninstall(purge_data: bool) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = crate::browser_integration::unregister();
    let _ = hkcu.delete_subkey_all(crate::installation::UNINSTALL_KEY);
    if let Ok(run) = hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        winreg::enums::KEY_WRITE,
    ) {
        let _ = run.delete_value("Ravyn");
    }

    remove_shortcuts()?;
    if purge_data {
        let data_dir = crate::backend::resolve_data_dir();
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir).map_err(|error| error.to_string())?;
        }
    }
    schedule_self_delete(&executable)
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
    for link in links {
        match std::fs::remove_file(&link) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn schedule_self_delete(executable: &std::path::Path) -> Result<(), String> {
    let path = executable.display().to_string().replace('\'', "''");
    // PowerShell is spawned independently, waits for this process to exit,
    // then deletes the binary with a literal path (no shell interpolation).
    let script = format!(
        "Start-Sleep -Seconds 2; Remove-Item -LiteralPath '{path}' -Force -ErrorAction SilentlyContinue"
    );
    std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}
