//! Windows appearance bridge for the Fluent backdrop.
//!
//! On Windows 11 22H2+ the shell windows use the compositor-owned acrylic
//! backdrop and this module only supplies accent/transparency metadata. On
//! Windows 10 (and pre-22H2 Windows 11) no compositor backdrop exists — the
//! only native option is the undocumented accent-policy blur, which stutters
//! and trails while the window moves — so the windows stay opaque and the
//! webview renders a synthetic material instead, positioned from the desktop
//! wallpaper, window/monitor geometry, and accent color provided here.

use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct DesktopAppearance {
    pub supported: bool,
    /// Whether the window actually carries a native compositor backdrop.
    /// False on Windows 10, where the webview must draw the material itself.
    pub native_backdrop: bool,
    pub wallpaper_path: Option<String>,
    pub wallpaper_revision: Option<String>,
    pub wallpaper_position: String,
    pub plane_x: f64,
    pub plane_y: f64,
    pub plane_width: f64,
    pub plane_height: f64,
    pub window_x: f64,
    pub window_y: f64,
    pub frame_offset_x: f64,
    pub frame_offset_y: f64,
    pub scale_factor: f64,
    pub accent_color: Option<String>,
    pub transparency_enabled: bool,
}

/// First build with the documented compositor backdrop
/// (`DWMWA_SYSTEMBACKDROP_TYPE`, Windows 11 22H2). Everything older — all of
/// Windows 10 and early Windows 11 — only offers the undocumented
/// accent-policy blur, which desynchronizes from the window during move and
/// resize operations.
const FIRST_COMPOSITOR_BACKDROP_BUILD: u32 = 22_621;

/// Whether this Windows build supports a compositor-owned window backdrop
/// that stays glitch-free while the window is dragged.
pub fn native_backdrop_supported() -> bool {
    has_compositor_backdrop(windows_build_number())
}

fn has_compositor_backdrop(build: u32) -> bool {
    build >= FIRST_COMPOSITOR_BACKDROP_BUILD
}

#[cfg(target_os = "windows")]
fn windows_build_number() -> u32 {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    static BUILD: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *BUILD.get_or_init(|| {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
            .ok()
            .and_then(|key| {
                key.get_value::<String, _>("CurrentBuild")
                    .or_else(|_| key.get_value::<String, _>("CurrentBuildNumber"))
                    .ok()
            })
            .and_then(|value| value.trim().parse().ok())
            // An unreadable build number falls back to the synthetic backdrop,
            // which works everywhere.
            .unwrap_or(0)
    })
}

#[cfg(not(target_os = "windows"))]
fn windows_build_number() -> u32 {
    0
}

#[derive(Debug, Clone, Copy)]
struct PhysicalRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PhysicalRect {
    fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
}

/// Read the current Windows desktop appearance and cache the wallpaper inside
/// Ravyn's asset-protocol scope. The command is restricted to the main window.
pub async fn read(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<DesktopAppearance, String> {
    let outer_position = window
        .outer_position()
        .map_err(|error| format!("failed to read the Ravyn frame position: {error}"))?;
    let inner_position = window.inner_position().unwrap_or(outer_position);
    let monitor = window
        .current_monitor()
        .map_err(|error| format!("failed to read the current monitor: {error}"))?
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| "Windows did not report a monitor for the Ravyn window".to_owned())?;
    let monitor_rect = PhysicalRect {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    };
    let monitors = window
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|monitor| PhysicalRect {
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
        })
        .collect::<Vec<_>>();
    let virtual_desktop = virtual_desktop_rect(&monitors).unwrap_or(monitor_rect);
    let scale_factor = monitor.scale_factor();
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("failed to resolve the Ravyn cache directory: {error}"))?
        .join("backdrop");

    tauri::async_runtime::spawn_blocking(move || {
        platform_appearance(
            &cache_dir,
            monitor_rect,
            virtual_desktop,
            inner_position.x,
            inner_position.y,
            inner_position.x - outer_position.x,
            inner_position.y - outer_position.y,
            scale_factor,
        )
    })
    .await
    .map_err(|error| format!("the Windows appearance worker failed: {error}"))?
}

fn virtual_desktop_rect(monitors: &[PhysicalRect]) -> Option<PhysicalRect> {
    let first = *monitors.first()?;
    let mut left = i64::from(first.x);
    let mut top = i64::from(first.y);
    let mut right = first.right();
    let mut bottom = first.bottom();
    for monitor in &monitors[1..] {
        left = left.min(i64::from(monitor.x));
        top = top.min(i64::from(monitor.y));
        right = right.max(monitor.right());
        bottom = bottom.max(monitor.bottom());
    }
    Some(PhysicalRect {
        x: left.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: top.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        width: (right - left).clamp(1, i64::from(u32::MAX)) as u32,
        height: (bottom - top).clamp(1, i64::from(u32::MAX)) as u32,
    })
}

#[cfg(target_os = "windows")]
// Internal plumbing between the command handler and Win32 introspection; the
// window geometry cannot be collapsed further without losing clarity.
#[allow(clippy::too_many_arguments)]
fn platform_appearance(
    cache_dir: &std::path::Path,
    monitor: PhysicalRect,
    virtual_desktop: PhysicalRect,
    window_x: i32,
    window_y: i32,
    frame_offset_x: i32,
    frame_offset_y: i32,
    scale_factor: f64,
) -> Result<DesktopAppearance, String> {
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let desktop = current_user
        .open_subkey(r"Control Panel\Desktop")
        .map_err(|error| format!("failed to open Windows desktop preferences: {error}"))?;
    let source: String = desktop.get_value("WallPaper").unwrap_or_default();
    let wallpaper_style: String = desktop
        .get_value("WallpaperStyle")
        .unwrap_or_else(|_| "10".to_owned());
    let tile_wallpaper: String = desktop
        .get_value("TileWallpaper")
        .unwrap_or_else(|_| "0".to_owned());
    let wallpaper_position = wallpaper_position(&wallpaper_style, &tile_wallpaper);

    let personalization = current_user
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .ok();
    let transparency_enabled = personalization
        .as_ref()
        .and_then(|key| key.get_value::<u32, _>("EnableTransparency").ok())
        .is_none_or(|value| value != 0);

    let accent_color = current_user
        .open_subkey(r"Software\Microsoft\Windows\DWM")
        .ok()
        .and_then(|key| key.get_value::<u32, _>("ColorizationColor").ok())
        .map(argb_to_css);

    let source_path = PathBuf::from(source.trim_matches('\0'));
    let (wallpaper_path, wallpaper_revision) = if source_path.is_file() {
        std::fs::create_dir_all(cache_dir)
            .map_err(|error| format!("failed to create the wallpaper cache: {error}"))?;
        let extension = source_path
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("jpg")
            .to_ascii_lowercase();
        let cached = cache_dir.join(format!("desktop-wallpaper.{extension}"));
        copy_if_changed(&source_path, &cached)?;
        remove_stale_wallpapers(cache_dir, &cached);
        let metadata = source_path.metadata().ok();
        let modified = metadata
            .as_ref()
            .and_then(|value| value.modified().ok())
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis())
            .unwrap_or_default();
        let revision = format!(
            "{}-{modified}",
            metadata.as_ref().map_or(0, std::fs::Metadata::len)
        );
        (Some(cached.to_string_lossy().into_owned()), Some(revision))
    } else {
        (None, None)
    };

    let plane = if wallpaper_position == "span" {
        virtual_desktop
    } else {
        monitor
    };
    let scale = scale_factor.max(0.5);
    Ok(DesktopAppearance {
        supported: true,
        native_backdrop: native_backdrop_supported(),
        wallpaper_path,
        wallpaper_revision,
        wallpaper_position: wallpaper_position.to_owned(),
        plane_x: f64::from(plane.x) / scale,
        plane_y: f64::from(plane.y) / scale,
        plane_width: f64::from(plane.width) / scale,
        plane_height: f64::from(plane.height) / scale,
        window_x: f64::from(window_x) / scale,
        window_y: f64::from(window_y) / scale,
        frame_offset_x: f64::from(frame_offset_x) / scale,
        frame_offset_y: f64::from(frame_offset_y) / scale,
        scale_factor,
        accent_color,
        transparency_enabled,
    })
}

#[cfg(target_os = "windows")]
fn copy_if_changed(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    let source_metadata = source.metadata().map_err(|error| {
        format!(
            "failed to inspect the Windows wallpaper {}: {error}",
            source.display()
        )
    })?;
    let unchanged = destination
        .metadata()
        .ok()
        .is_some_and(|destination_metadata| {
            destination_metadata.len() == source_metadata.len()
                && destination_metadata.modified().ok() == source_metadata.modified().ok()
        });
    if unchanged {
        return Ok(());
    }
    let temporary = destination.with_extension("ravyn-part");
    std::fs::copy(source, &temporary).map_err(|error| {
        format!(
            "failed to cache the Windows wallpaper {}: {error}",
            source.display()
        )
    })?;
    if destination.exists() {
        let _ = std::fs::remove_file(destination);
    }
    std::fs::rename(&temporary, destination)
        .map_err(|error| format!("failed to activate the cached Windows wallpaper: {error}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_stale_wallpapers(cache_dir: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path != keep
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("desktop-wallpaper."))
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(target_os = "windows")]
fn wallpaper_position(style: &str, tiled: &str) -> &'static str {
    if tiled.trim() == "1" {
        return "tile";
    }
    match style.trim() {
        "0" => "center",
        "2" => "stretch",
        "6" => "fit",
        "22" => "span",
        _ => "fill",
    }
}

#[cfg(target_os = "windows")]
fn argb_to_css(value: u32) -> String {
    let red = (value >> 16) & 0xff;
    let green = (value >> 8) & 0xff;
    let blue = value & 0xff;
    format!("#{red:02x}{green:02x}{blue:02x}")
}

#[cfg(not(target_os = "windows"))]
fn platform_appearance(
    _cache_dir: &std::path::Path,
    monitor: PhysicalRect,
    _virtual_desktop: PhysicalRect,
    window_x: i32,
    window_y: i32,
    frame_offset_x: i32,
    frame_offset_y: i32,
    scale_factor: f64,
) -> Result<DesktopAppearance, String> {
    let scale = scale_factor.max(0.5);
    Ok(DesktopAppearance {
        supported: false,
        native_backdrop: false,
        wallpaper_path: None,
        wallpaper_revision: None,
        wallpaper_position: "fill".to_owned(),
        plane_x: f64::from(monitor.x) / scale,
        plane_y: f64::from(monitor.y) / scale,
        plane_width: f64::from(monitor.width) / scale,
        plane_height: f64::from(monitor.height) / scale,
        window_x: f64::from(window_x) / scale,
        window_y: f64::from(window_y) / scale,
        frame_offset_x: f64::from(frame_offset_x) / scale,
        frame_offset_y: f64::from(frame_offset_y) / scale,
        scale_factor,
        accent_color: None,
        transparency_enabled: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_backdrop_requires_windows_11_22h2() {
        assert!(!has_compositor_backdrop(0)); // unreadable build number
        assert!(!has_compositor_backdrop(19_045)); // Windows 10 22H2
        assert!(!has_compositor_backdrop(22_000)); // Windows 11 21H2 (accent acrylic)
        assert!(has_compositor_backdrop(22_621)); // Windows 11 22H2
        assert!(has_compositor_backdrop(26_100)); // Windows 11 24H2
    }

    #[test]
    fn virtual_desktop_supports_negative_monitor_coordinates() {
        let result = virtual_desktop_rect(&[
            PhysicalRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            PhysicalRect {
                x: 0,
                y: -200,
                width: 2560,
                height: 1440,
            },
        ])
        .expect("desktop bounds");
        assert_eq!(result.x, -1920);
        assert_eq!(result.y, -200);
        assert_eq!(result.width, 4480);
        assert_eq!(result.height, 1440);
    }
}
