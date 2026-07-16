//! Windows appearance bridge for the synthetic Fluent backdrop.
//!
//! Ravyn deliberately renders its material in the webview so the visual result
//! stays consistent on Windows 10 and Windows 11. This module supplies the
//! current desktop wallpaper, positioning metadata, window/monitor geometry,
//! and the Windows accent color without applying native Mica or Acrylic.

use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct DesktopAppearance {
    pub supported: bool,
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
        .map_or(true, |value| value != 0);

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
