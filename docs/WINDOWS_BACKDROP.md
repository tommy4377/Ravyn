# Windows compositor backdrop

Ravyn's main and setup windows use Tauri's `Acrylic` window effect on Windows.
Unlike the wallpaper-based fallback, this effect is produced by the Desktop
Window Manager and therefore samples the composed pixels behind the Ravyn
window, including other applications.

The implementation deliberately uses Tauri's window-effects abstraction rather
than calling Win32 functions from Ravyn. Tauri selects the supported compositor
path for the installed Windows build: acrylic composition on Windows 10 and the
system transient-window backdrop on current Windows 11 releases.

The Svelte document and webview are transparent while the effect is active.
Ravyn's Fluent tint, translucent surfaces, and noise layer are then composited
above the native blur. The old wallpaper reconstruction remains available as a
deterministic fallback when:

- Windows transparency effects are disabled;
- the UI is running in a normal browser instead of the desktop shell;
- the user configures a custom backdrop image;
- the user selects the solid material.

CSS `backdrop-filter` alone cannot blur pixels owned by another top-level
window. Cross-window blur necessarily has to be performed by the operating
system compositor; the browser only receives pixels inside its own webview.

The local `ExplorerBlurMica-main/` checkout was used only as behavioral
reference and is excluded by the repository `.gitignore`.
