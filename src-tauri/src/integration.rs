//! Windows shell integration applied during setup.
//!
//! Installs the application executable, creates Start Menu / desktop
//! shortcuts, registers startup-with-Windows, and records the Installed Apps
//! entry. Every step reports an individual result so the setup can surface
//! partial failures honestly instead of pretending success.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct IntegrationRequest {
    pub install_application: bool,
    pub register_installed_app: bool,
    pub start_menu_shortcut: bool,
    pub desktop_shortcut: bool,
    pub launch_at_startup: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepResult {
    pub step: String,
    pub applied: bool,
    pub skipped_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationReport {
    pub steps: Vec<StepResult>,
    pub install_dir: Option<String>,
    pub installed_exe: Option<String>,
}

fn ok(step: &str) -> StepResult {
    StepResult {
        step: step.into(),
        applied: true,
        skipped_reason: None,
        error: None,
    }
}

fn skipped(step: &str, reason: &str) -> StepResult {
    StepResult {
        step: step.into(),
        applied: false,
        skipped_reason: Some(reason.into()),
        error: None,
    }
}

fn failed(step: &str, error: String) -> StepResult {
    StepResult {
        step: step.into(),
        applied: false,
        skipped_reason: None,
        error: Some(error),
    }
}

/// Apply the requested Windows integration steps sequentially.
///
/// Individual failures never abort the remaining steps.
pub fn apply(request: &IntegrationRequest) -> IntegrationReport {
    let mut steps = Vec::new();
    let source_exe = std::env::current_exe().ok();
    let mut installed_exe: Option<std::path::PathBuf> = None;
    let install_dir = crate::installation::default_install_dir();

    // 1. Install (copy) the application executable.
    if request.install_application {
        match (&source_exe, &install_dir) {
            (Some(source), Some(dir)) => {
                let target_dir = std::path::PathBuf::from(dir);
                let target = target_dir.join("Ravyn.exe");
                if source == &target {
                    installed_exe = Some(target);
                    steps.push(skipped(
                        "install_application",
                        "already running from the installed location",
                    ));
                } else if cfg!(debug_assertions) {
                    // Development builds stay in the target directory; copying
                    // a debug binary into Programs would misrepresent install.
                    installed_exe = Some(source.clone());
                    steps.push(skipped(
                        "install_application",
                        "development build runs in place",
                    ));
                } else {
                    match install_executable(source, &target) {
                        Ok(()) => {
                            installed_exe = Some(target);
                            steps.push(ok("install_application"));
                        }
                        Err(error) => steps.push(failed("install_application", error)),
                    }
                }
            }
            _ => steps.push(failed(
                "install_application",
                "cannot resolve the executable or install directory".into(),
            )),
        }
    } else {
        steps.push(skipped("install_application", "not requested"));
    }

    let effective_exe = installed_exe.clone().or(source_exe);

    // 2. Installed Apps registration.
    if request.register_installed_app {
        match &effective_exe {
            Some(exe) => match register_installed_app(exe, install_dir.as_deref()) {
                Ok(()) => steps.push(ok("register_installed_app")),
                Err(error) => steps.push(failed("register_installed_app", error)),
            },
            None => steps.push(failed(
                "register_installed_app",
                "no executable to register".into(),
            )),
        }
    } else {
        steps.push(skipped("register_installed_app", "not requested"));
    }

    // 3. Start Menu shortcut.
    if request.start_menu_shortcut {
        match &effective_exe {
            Some(exe) => match create_start_menu_shortcut(exe) {
                Ok(()) => steps.push(ok("start_menu_shortcut")),
                Err(error) => steps.push(failed("start_menu_shortcut", error)),
            },
            None => steps.push(failed(
                "start_menu_shortcut",
                "no executable for the shortcut".into(),
            )),
        }
    } else {
        steps.push(skipped("start_menu_shortcut", "not requested"));
    }

    // 4. Desktop shortcut.
    if request.desktop_shortcut {
        match &effective_exe {
            Some(exe) => match create_desktop_shortcut(exe) {
                Ok(()) => steps.push(ok("desktop_shortcut")),
                Err(error) => steps.push(failed("desktop_shortcut", error)),
            },
            None => steps.push(failed(
                "desktop_shortcut",
                "no executable for the shortcut".into(),
            )),
        }
    } else {
        steps.push(skipped("desktop_shortcut", "not requested"));
    }

    // 5. Startup with Windows.
    if request.launch_at_startup {
        match &effective_exe {
            Some(exe) => match register_startup(exe) {
                Ok(()) => steps.push(ok("launch_at_startup")),
                Err(error) => steps.push(failed("launch_at_startup", error)),
            },
            None => steps.push(failed(
                "launch_at_startup",
                "no executable to register".into(),
            )),
        }
    } else {
        steps.push(skipped("launch_at_startup", "not requested"));
    }

    IntegrationReport {
        steps,
        install_dir,
        installed_exe: installed_exe.map(|p| p.display().to_string()),
    }
}

fn install_executable(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    let dir = target
        .parent()
        .ok_or_else(|| "install target has no parent directory".to_owned())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Copy to a temporary name first, then rename for an atomic-ish swap that
    // tolerates a running previous version (Windows allows renaming a mapped
    // executable but not overwriting it in place).
    let staged = dir.join(".ravyn.install.tmp");
    std::fs::copy(source, &staged).map_err(|e| e.to_string())?;
    if target.exists() {
        let backup = dir.join(".ravyn.previous.exe");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(target, &backup).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&staged, target).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn register_installed_app(exe: &std::path::Path, install_dir: Option<&str>) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(crate::installation::UNINSTALL_KEY)
        .map_err(|e| e.to_string())?;
    key.set_value("DisplayName", &"Ravyn")
        .map_err(|e| e.to_string())?;
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))
        .map_err(|e| e.to_string())?;
    key.set_value("Publisher", &"Ravyn")
        .map_err(|e| e.to_string())?;
    key.set_value("DisplayIcon", &exe.display().to_string())
        .map_err(|e| e.to_string())?;
    if let Some(dir) = install_dir {
        key.set_value("InstallLocation", &dir)
            .map_err(|e| e.to_string())?;
    }
    key.set_value(
        "UninstallString",
        &format!("\"{}\" --uninstall", exe.display()),
    )
    .map_err(|e| e.to_string())?;
    key.set_value("NoModify", &1u32)
        .map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
fn register_installed_app(_exe: &std::path::Path, _dir: Option<&str>) -> Result<(), String> {
    Err("installed app registration is only supported on Windows".into())
}

#[cfg(windows)]
fn register_startup(exe: &std::path::Path) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;
    key.set_value("Ravyn", &format!("\"{}\"", exe.display()))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
fn register_startup(_exe: &std::path::Path) -> Result<(), String> {
    Err("startup registration is only supported on Windows".into())
}

#[cfg(windows)]
fn create_start_menu_shortcut(exe: &std::path::Path) -> Result<(), String> {
    let appdata = std::env::var("APPDATA").map_err(|e| e.to_string())?;
    let link =
        std::path::PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Ravyn.lnk");
    create_shortcut(exe, &link)
}

#[cfg(not(windows))]
fn create_start_menu_shortcut(_exe: &std::path::Path) -> Result<(), String> {
    Err("shortcuts are only supported on Windows".into())
}

#[cfg(windows)]
fn create_desktop_shortcut(exe: &std::path::Path) -> Result<(), String> {
    let profile = std::env::var("USERPROFILE").map_err(|e| e.to_string())?;
    let link = std::path::PathBuf::from(profile).join(r"Desktop\Ravyn.lnk");
    create_shortcut(exe, &link)
}

#[cfg(not(windows))]
fn create_desktop_shortcut(_exe: &std::path::Path) -> Result<(), String> {
    Err("shortcuts are only supported on Windows".into())
}

/// Create a `.lnk` shortcut via the Windows Script Host COM object.
///
/// PowerShell is the most reliable dependency-free way to drive `IShellLink`
/// from a user process; paths are passed through single-quoted PowerShell
/// strings with quote doubling to prevent injection.
#[cfg(windows)]
fn create_shortcut(target: &std::path::Path, link: &std::path::Path) -> Result<(), String> {
    let escape = |value: &str| value.replace('\'', "''");
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath='{}'; $s.WorkingDirectory='{}'; $s.Save()",
        escape(&link.display().to_string()),
        escape(&target.display().to_string()),
        escape(
            &target
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        ),
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
