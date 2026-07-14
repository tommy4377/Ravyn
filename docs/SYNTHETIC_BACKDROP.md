# Synthetic Windows Backdrop

Ravyn renders its primary window material in Svelte/CSS instead of relying on
native Mica or Acrylic. This keeps the same visual language on Windows 10 and
Windows 11 while still following the user's desktop.

## Native bridge

The main-window-only `desktop_appearance` command:

- reads the current wallpaper and wallpaper layout from the current-user
  Windows desktop preferences;
- copies the wallpaper into `$APPCACHE/backdrop`, the only local directory
  exposed through Tauri's asset protocol;
- returns monitor, virtual-desktop, window, frame-offset, and DPI metadata;
- reads the current DWM colorization color and Windows transparency preference.

The source wallpaper path is never exposed to the webview. Only the private
cached copy is returned.

## Frontend behavior

`systemAppearance` converts the cached file through Tauri's asset protocol and
updates CSS custom properties for the desktop plane. Window movement updates
only the CSS offset immediately; a debounced native refresh then detects monitor
or DPI transitions. Focus, theme, resize, scale, and a slow polling interval
refresh wallpaper and accent metadata.

Supported Windows wallpaper modes:

- Center
- Tile
- Stretch
- Fit
- Fill
- Span

A custom backdrop URL in Settings overrides the Windows wallpaper. Solid mode
removes wallpaper, glow, and noise entirely. Forced-colors mode removes the
synthetic material and dynamic accent overrides.

## Color handling

The DWM accent is transformed into separate light- and dark-theme palettes.
Light mode clamps overly bright accents to a darker, readable control color;
dark mode raises overly dark accents to a brighter value. Hover, pressed,
subtle, border, text, and foreground-on-accent tokens are derived separately.

## Known Windows limitation

The bridge uses the desktop preference path exposed for the current user. A
configuration where each monitor uses a different wallpaper may require a future
`IDesktopWallpaper` provider to select a separate cached image for each monitor.
The positioning and Span geometry already support the virtual desktop.
