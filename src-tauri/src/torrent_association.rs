//! Windows torrent association registration.
//!
//! Windows owns the final default-app choice. Ravyn therefore only registers
//! a truthful, per-user capability and opens the system Default Apps page for
//! an explicit user choice; it never overwrites another application's choice.

#[cfg(windows)]
pub fn register_and_prompt() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    if !crate::installation::current_executable_is_installed() {
        return Err("install Ravyn before registering it for torrent files".into());
    }
    let command = format!("\"{}\" \"%1\"", executable.display());
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (classes, _) = hkcu
        .create_subkey(r"Software\Classes\Ravyn.Torrent")
        .map_err(|error| error.to_string())?;
    classes
        .set_value("", &"Ravyn torrent")
        .map_err(|error| error.to_string())?;
    // This marks the ProgID as a URL handler. The capability map below lets
    // Windows present it for `magnet:` without changing another app's active
    // handler before the user explicitly selects Ravyn.
    classes
        .set_value("URL Protocol", &"")
        .map_err(|error| error.to_string())?;
    let (icon, _) = classes
        .create_subkey("DefaultIcon")
        .map_err(|error| error.to_string())?;
    icon.set_value("", &format!("{},0", executable.display()))
        .map_err(|error| error.to_string())?;
    let (open, _) = classes
        .create_subkey(r"shell\open\command")
        .map_err(|error| error.to_string())?;
    open.set_value("", &command)
        .map_err(|error| error.to_string())?;

    let (capabilities, _) = hkcu
        .create_subkey(r"Software\Ravyn\Capabilities")
        .map_err(|error| error.to_string())?;
    capabilities
        .set_value("ApplicationName", &"Ravyn")
        .map_err(|error| error.to_string())?;
    capabilities
        .set_value("ApplicationDescription", &"Ravyn download manager")
        .map_err(|error| error.to_string())?;
    let (files, _) = capabilities
        .create_subkey("FileAssociations")
        .map_err(|error| error.to_string())?;
    files
        .set_value(".torrent", &"Ravyn.Torrent")
        .map_err(|error| error.to_string())?;
    let (urls, _) = capabilities
        .create_subkey("URLAssociations")
        .map_err(|error| error.to_string())?;
    urls.set_value("magnet", &"Ravyn.Torrent")
        .map_err(|error| error.to_string())?;
    let (registered, _) = hkcu
        .create_subkey(r"Software\RegisteredApplications")
        .map_err(|error| error.to_string())?;
    registered
        .set_value("Ravyn", &r"Software\Ravyn\Capabilities")
        .map_err(|error| error.to_string())?;

    std::process::Command::new("explorer.exe")
        .arg("ms-settings:defaultapps")
        .spawn()
        .map_err(|error| format!("registered Ravyn but could not open Default Apps: {error}"))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn register_and_prompt() -> Result<(), String> {
    Err("torrent file association is only supported on Windows".into())
}
