# Ravyn Project Status

Last source audit: 2026-07-14

## Implemented in source

- Advanced download backend, library, media, torrent, automation, component,
  backup, diagnostics, and loopback API layers.
- Multipage Svelte frontend connected to the existing backend APIs.
- Responsive Downloads shell, contextual actions, resizable details, and typed
  direct/media/torrent creation flows.
- Bright light theme, substantially darker dark theme, High Contrast and reduced
  motion fallbacks, density controls, and persistent shell preferences.
- Synthetic Fluent material driven by the Windows wallpaper, wallpaper layout,
  monitor/window geometry, DPI, and DWM accent color.
- Native open-file, open-folder, and Explorer reveal actions for validated local
  output paths.
- Setup installation reporting and backend-authoritative completion validation.
- Separate setup/main Tauri capabilities, caller validation, CSP, restricted
  asset-protocol scope, and process-level setup transition guard.
- Tauri NSIS/MSI/portable release pipeline with install/start/uninstall smoke
  checks.
- Signed silent installed-app updates staged in the background and applied after
  normal application close.

## Locally verified in this environment

- `svelte-check`: 0 errors, 0 warnings.
- Vitest: 67/67 tests passed.
- Vite production build: completed.
- Tauri JSON, command-permission mapping, and workflow YAML: statically valid.

Rust/Tauri compilation was not available in this Linux analysis environment
because `cargo`, `rustc`, and `rustfmt` are not installed. Windows CI remains the
native source of truth.

## Release blockers still open

1. Authenticode signing for the executable, NSIS installer, and MSI.
2. Production remote component-manifest provider with caching, ETag, expiry,
   replay/downgrade protection, and last-known-good recovery.
3. Final 7-Zip/archive extraction distribution decision.
4. Application-update readiness confirmation, retained previous-version
   rollback, persisted update result, and repair mode.
5. Full clean-machine WebView2 UI automation through setup, a real download,
   update N to N+1, rollback, DPI, keyboard, and accessibility scenarios.
6. Persisted setup consent/idempotency records that survive process restart.
7. Browser-extension capture and secure secret-entry product workflows.
8. Monitor-specific different-wallpaper selection through `IDesktopWallpaper`;
   current code supports the active wallpaper and virtual-desktop Span layout.

## Recommended next implementation order

1. Run and fix the complete Rust/Tauri build on Windows.
2. Add Authenticode signing and verify signatures in CI.
3. Add updater health confirmation, rollback, and repair.
4. Finish remote component manifests and the 7-Zip decision.
5. Add Windows product E2E automation.
6. Complete browser capture, secrets, and remaining advanced API surfaces.
