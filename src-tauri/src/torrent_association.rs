//! Windows torrent association registration.
//!
//! Windows owns the final default-app choice. Ravyn therefore only registers
//! a truthful, per-user capability and opens the system Default Apps page for
//! an explicit user choice; it never overwrites another application's choice.

const DEFAULT_APPS_URI: &str = "ms-settings:defaultapps?registeredAppUser=Ravyn";

#[cfg(windows)]
pub fn register_and_prompt() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{
        SHCNE_ASSOCCHANGED, SHCNF_FLUSH, SHCNF_IDLIST, SHChangeNotify,
    };
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
    if let Err(error) = classes.delete_value("URL Protocol")
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.to_string());
    }
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

    // Protocol handlers use a distinct ProgID so the torrent file ProgID is
    // never marked as a URL protocol by the Windows association resolver.
    let (magnet, _) = hkcu
        .create_subkey(r"Software\Classes\Ravyn.Magnet")
        .map_err(|error| error.to_string())?;
    magnet
        .set_value("", &"Ravyn magnet link")
        .map_err(|error| error.to_string())?;
    magnet
        .set_value("URL Protocol", &"")
        .map_err(|error| error.to_string())?;
    let (magnet_icon, _) = magnet
        .create_subkey("DefaultIcon")
        .map_err(|error| error.to_string())?;
    magnet_icon
        .set_value("", &format!("{},0", executable.display()))
        .map_err(|error| error.to_string())?;
    let (magnet_open, _) = magnet
        .create_subkey(r"shell\open\command")
        .map_err(|error| error.to_string())?;
    magnet_open
        .set_value("", &command)
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
    urls.set_value("magnet", &"Ravyn.Magnet")
        .map_err(|error| error.to_string())?;
    let (registered, _) = hkcu
        .create_subkey(r"Software\RegisteredApplications")
        .map_err(|error| error.to_string())?;
    registered
        .set_value("Ravyn", &r"Software\Ravyn\Capabilities")
        .map_err(|error| error.to_string())?;

    // The Shell caches registered handlers. Flush the association change
    // before opening Settings so Ravyn appears immediately in the choices.
    // SAFETY: this event requires null item pointers and performs no memory access.
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST | SHCNF_FLUSH,
            std::ptr::null(),
            std::ptr::null(),
        );
    }

    std::process::Command::new("explorer.exe")
        .arg(DEFAULT_APPS_URI)
        .spawn()
        .map_err(|error| format!("registered Ravyn but could not open Default Apps: {error}"))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn register_and_prompt() -> Result<(), String> {
    Err("torrent file association is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_apps_uri_targets_the_per_user_ravyn_registration() {
        assert_eq!(
            DEFAULT_APPS_URI,
            "ms-settings:defaultapps?registeredAppUser=Ravyn"
        );
    }
}
