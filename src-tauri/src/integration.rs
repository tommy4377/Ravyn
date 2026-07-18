//! Windows shell integration applied during setup.
//!
//! Installs the application executable, creates Start Menu / desktop
//! shortcuts, registers startup-with-Windows, and records the Installed Apps
//! entry. Every step reports an individual result so the setup can surface
//! partial failures honestly instead of pretending success.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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
    pub installed_version: Option<String>,
    pub installed_sha256: Option<String>,
    pub integration_completed: bool,
    pub integration_errors: Vec<String>,
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

fn finish_report(
    steps: Vec<StepResult>,
    install_dir: Option<String>,
    installed_exe: Option<std::path::PathBuf>,
    registration_required: bool,
) -> IntegrationReport {
    let installed_sha256 = installed_exe
        .as_deref()
        .and_then(|path| crate::installation::sha256_file(path).ok());
    let registration_completed = !registration_required
        || steps
            .iter()
            .any(|step| step.step == "register_installed_app" && step.applied);
    let integration_errors = steps
        .iter()
        .filter_map(|step| {
            step.error
                .as_ref()
                .map(|error| format!("{}: {error}", step.step))
        })
        .collect::<Vec<_>>();
    let native_host_completed = installed_exe.is_none()
        || steps
            .iter()
            .any(|step| step.step == "register_firefox_native_host" && step.applied);
    let integration_completed = installed_exe
        .as_deref()
        .is_some_and(std::path::Path::is_file)
        && installed_sha256.is_some()
        && registration_completed
        && native_host_completed;

    IntegrationReport {
        steps,
        install_dir,
        installed_exe: installed_exe.map(|path| path.display().to_string()),
        installed_version: integration_completed.then(|| env!("CARGO_PKG_VERSION").to_owned()),
        installed_sha256,
        integration_completed,
        integration_errors,
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
    } else if source_exe
        .as_deref()
        .is_some_and(|_| crate::installation::current_executable_is_installed())
    {
        installed_exe = source_exe.clone();
        steps.push(skipped(
            "install_application",
            "the application is already managed by the Windows installer",
        ));
    } else {
        steps.push(skipped("install_application", "not requested"));
    }

    // Native registrations must always target a verified installed executable.
    // Never fall back to the setup, portable, or development binary because
    // those paths can disappear and would leave broken Windows registrations.
    let dependent_registration_requested = request.register_installed_app
        || request.start_menu_shortcut
        || request.desktop_shortcut
        || request.launch_at_startup;
    if dependent_registration_requested && installed_exe.is_none() {
        for (step, requested) in [
            ("register_installed_app", request.register_installed_app),
            ("start_menu_shortcut", request.start_menu_shortcut),
            ("desktop_shortcut", request.desktop_shortcut),
            ("launch_at_startup", request.launch_at_startup),
            ("register_firefox_native_host", true),
            ("register_torrent_association", true),
        ] {
            steps.push(skipped(
                step,
                if requested {
                    "no verified installed executable is available"
                } else {
                    "not requested"
                },
            ));
        }
        return finish_report(steps, install_dir, None, request.register_installed_app);
    }

    let effective_exe = installed_exe.clone();

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

    // 6. Firefox native-messaging host. Registration is per-user and safe
    // even when Firefox is not installed yet; the extension becomes usable as
    // soon as it is added to the browser.
    match &effective_exe {
        Some(exe) => match crate::browser_integration::register(exe) {
            Ok(()) => steps.push(ok("register_firefox_native_host")),
            Err(error) => steps.push(failed("register_firefox_native_host", error)),
        },
        None => steps.push(failed(
            "register_firefox_native_host",
            "no executable to register as the native host".into(),
        )),
    }

    // 7. Torrent/magnet association. Registers Ravyn as an available choice
    // for `.torrent` files and `magnet:` links (a per-user, no-admin registry
    // write) so it works as a torrent client out of the box; it never forces
    // itself as the default, matching how register_installed_app never opens
    // Default Apps here either — Windows still owns that final choice.
    match &effective_exe {
        Some(exe) => match crate::torrent_association::register(exe) {
            Ok(()) => steps.push(ok("register_torrent_association")),
            Err(error) => steps.push(failed("register_torrent_association", error)),
        },
        None => steps.push(failed(
            "register_torrent_association",
            "no executable to register for torrent/magnet handling".into(),
        )),
    }

    finish_report(
        steps,
        install_dir,
        installed_exe,
        request.register_installed_app,
    )
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
    if crate::installation::sha256_file(source)? != crate::installation::sha256_file(&staged)? {
        let _ = std::fs::remove_file(&staged);
        return Err("staged executable checksum does not match the source".into());
    }
    let backup = dir.join(".ravyn.previous.exe");
    let replaced_existing = target.exists();
    if target.exists() {
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(target, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(error) = std::fs::rename(&staged, target) {
        if replaced_existing && backup.exists() {
            if let Err(restore_error) = std::fs::rename(&backup, target) {
                let _ = std::fs::remove_file(&staged);
                return Err(format!(
                    "failed to activate the staged executable ({error}) and restore the previous executable ({restore_error})"
                ));
            }
        }
        let _ = std::fs::remove_file(&staged);
        return Err(error.to_string());
    }
    Ok(())
}

/// A freshly launched installed copy calls this only after its backend is
/// ready. Retaining the previous binary until this point leaves a manual
/// recovery path if startup fails.
pub fn confirm_installed_copy_ready() {
    let Some(dir) = crate::installation::default_install_dir() else {
        return;
    };
    let current = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return,
    };
    let expected = std::path::PathBuf::from(dir).join("Ravyn.exe");
    if current
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected.to_string_lossy())
    {
        let _ = std::fs::remove_file(expected.with_file_name(".ravyn.previous.exe"));
    }
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
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    crate::silent_command::hide_console_window(&mut command);
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
