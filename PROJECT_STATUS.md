# Ravyn Project Status

Last source audit: 2026-07-14

## Implemented in source

- Advanced direct-download, library, media, torrent, automation, component,
  backup, diagnostics, and authenticated loopback API layers.
- Multipage Svelte 5 frontend connected to the primary backend workflows.
- Refined shell hierarchy, calmer transfer rows, restrained surfaces, clearer
  navigation groups, responsive layouts, and persistent light/dark/density
  preferences.
- Typed direct, media, torrent, basket, library, automation, component,
  diagnostics, presets, profiles, settings, and secure-secret flows.
- Settings exposes validated executable overrides for yt-dlp, FFmpeg, rqbit,
  and 7-Zip plus the rqbit API URL and an optional credential reference.
- Synthetic Fluent material driven by the Windows wallpaper, layout, virtual
  desktop/window geometry, DPI, and DWM accent color.
- Native open-file, open-folder, Explorer reveal, installation reporting, and
  backend-authoritative setup completion.
- Persisted, restart-safe setup integration consent with exact backend/Tauri
  request matching and idempotent installation reporting.
- Separate setup/main Tauri capabilities, caller validation, CSP, restricted
  asset-protocol scope, and setup transition guards.
- Tauri NSIS/MSI/portable release pipeline with install/start/uninstall smoke
  checks.
- Production signed component-catalog refresh with HTTPS-only conditional GET,
  ETag/Last-Modified, bounded reads, expiry, replay/downgrade protection,
  rollback-capable cache activation, last-known-good recovery, API status, UI
  refresh, and tagged-release generation.
- Signed installed-app updates staged in the background and applied after a
  normal close, with persisted staging, new-version backend/webview readiness,
  retained previous-binary rollback, failed-version retry suppression, current
  version repair, and a persisted result shown in Settings.
- Authenticode signing configuration and signature/timestamp verification in
  the tagged Windows release workflow.
- Ravyn 0.2 archive-tool policy: use an existing system or custom `7z`/`7za`
  executable. Managed 7-Zip provisioning is intentionally deferred.

## Locally verified in this environment

- `svelte-check`: 0 errors, 0 warnings.
- Vitest: 67/67 tests passed.
- Vite production build: completed.
- 126 Rust files parsed without syntax errors.
- Tauri JSON, TOML, command-permission mapping, and workflow YAML parsed.

Rust/Tauri compilation was not available because Cargo, rustc, rustfmt, the
Windows SDK, and WebView2 are not installed. Windows CI remains the native
source of truth.

## Effective release blockers still open

1. Complete the Rust/Tauri workspace build and runtime pass on Windows.
2. Supply the real Authenticode and manifest-signing credentials and complete a
   successful tagged release; the signing workflow itself is implemented.
3. Run clean-machine WebView2 automation through setup, real component
   provisioning, a real download, update N to N+1, deliberate rollback, DPI,
   keyboard, high contrast, and accessibility scenarios.
4. Extend updater recovery beyond the retained main executable to all installed
   files, registry/uninstaller state, and an interrupted-helper startup path.
5. Add monitor-specific different-wallpaper selection through
   `IDesktopWallpaper`.

## Secondary work, not core beta blockers

- Richer Metalink and large batch-import UX.
- Tag management, filename-template preview, and automation-rule preview.
- Deeper DHT/peer/host diagnostic tables.
- Structured per-secret-type editors; the generic credential-store flow works.
- Optional native `.7z` extraction or managed 7-Zip provisioning.
- Browser extension, intentionally excluded from this pass.

## Recommended next implementation order

1. Run the full Windows Rust/Tauri test and bundle pipeline.
2. Execute the signed tagged-release pipeline with production credentials.
3. Build clean-machine product and updater rollback E2E.
4. Finish full installed-state updater recovery.
5. Complete remaining advanced frontend surfaces.
