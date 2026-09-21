# Windows compositor backdrop

Ravyn's main and setup windows use Tauri's `Acrylic` window effect on Windows
11 22H2 (build 22621) and later, where it maps to the compositor-owned system
backdrop (`DWMWA_SYSTEMBACKDROP_TYPE`). Unlike the wallpaper-based material,
this effect is produced by the Desktop Window Manager and therefore samples the
composed pixels behind the Ravyn window, including other applications. Because
the compositor owns the backdrop, it stays aligned with the window during
moves and resizes.

Older builds — all of Windows 10 and pre-22H2 Windows 11 — only offer the
undocumented accent-policy blur (`SetWindowCompositionAttribute`). That path
was built for immovable shell surfaces: it desynchronizes from the window
rectangle while dragging (stutter and trailing) and shows through any client
area the page does not cover, so a viewport-sizing bug renders as a "bleeding"
band of material. On these builds Ravyn keeps the windows opaque and skips the
native effect entirely (`native_backdrop_supported` in
`src-tauri/src/appearance.rs`); the webview renders the synthetic
wallpaper-based material instead, positioned from the desktop wallpaper,
window/monitor geometry, and accent color.

The Svelte document and webview are transparent only while the native effect
is active. Ravyn's Fluent tint, translucent surfaces, and noise layer are
composited above whichever backdrop is in use. The wallpaper reconstruction is
the deterministic material when:

- the Windows build predates the 22H2 compositor backdrop (all of Windows 10);
- Windows transparency effects are disabled;
- the UI is running in a normal browser instead of the desktop shell;
- the user configures a custom backdrop image;
- the user selects the solid material.

CSS `backdrop-filter` alone cannot blur pixels owned by another top-level
window. Cross-window blur necessarily has to be performed by the operating
system compositor; the browser only receives pixels inside its own webview.

The local `ExplorerBlurMica-main/` checkout was used only as behavioral
reference and is excluded by the repository `.gitignore`.
