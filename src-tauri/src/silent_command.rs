//! Shared helper so shell-spawned console children (PowerShell, registry
//! helpers) never flash a terminal window over the GUI.

#[cfg(windows)]
pub fn hide_console_window(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_console_window(_command: &mut std::process::Command) {}
