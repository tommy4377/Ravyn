# Desktop Release Checklist

This file tracks the current release-critical desktop state. “Implemented”
means the source path exists; Windows CI remains the authority for native
compilation, installer behavior, and clean-machine verification.

| Item | Status | Notes |
|---|---|---|
| Frontend validation | **Implemented and locally verified** | `svelte-check` reports 0 errors/0 warnings, 67/67 Vitest tests pass, and the Vite production build succeeds. |
| Tauri dev commands | **Fixed in source** | `beforeDevCommand` and `beforeBuildCommand` resolve the frontend from `src-tauri`. Native execution still requires a Windows Rust/Tauri environment. |
| Tauri desktop bundle | **Implemented in CI** | Bundle generation is enabled for current-user NSIS and MSI installers. The raw desktop executable is also packaged as a portable ZIP. |
| Windows installer smoke test | **Implemented in CI** | Tagged/manual Windows builds silently install NSIS, verify executable/registry/uninstaller, launch Ravyn, then silently uninstall and verify cleanup. |
| Version consistency | **Implemented in CI** | Root, desktop, Tauri, frontend, and release-tag versions must match. |
| Tauri command isolation | **Implemented** | Setup and main windows have separate capabilities; sensitive commands also validate the caller window in Rust. |
| Content Security Policy | **Implemented** | Production CSP limits scripts, frames, objects, forms, assets, and loopback backend connections. |
| Setup installation reporting | **Implemented** | Installed/portable/development mode, executable path, version, SHA-256, integration result, and relaunch state are persisted before setup can complete. |
| Portable mode | **Implemented for setup/runtime policy** | Portable mode is explicit and never uses the installed-app updater. A dedicated portable updater remains intentionally disabled. |
| Synthetic Windows backdrop | **Implemented in source** | Main-window-only bridge caches the Windows wallpaper in a restricted asset scope, aligns it to window/monitor geometry, reads the DWM accent, and supplies safe Explorer/file actions. Native Windows verification remains part of product E2E. |
| Silent signed app updater | **Implemented in source and release workflow** | Installed builds check in the background, stream and verify a signed NSIS installer, and apply it after a normal close. See `docs/APP_UPDATES.md`. |
| Updater rollback after failed new-version boot | **Still open** | The installer relaunches Ravyn and falls back to relaunching the existing path if installation returns an error, but a full previous-binary rollback/health-confirmation protocol is not yet implemented. |
| Windows code signing | **Still open** | Authenticode signing for the executable and installers still needs a certificate/provider and CI secrets. |
| Remote signed component manifest | **Still open** | Managed engine signature verification exists, but the production remote provider/cache/ETag/replay policy is not complete. |
| 7-Zip capability | **Still open** | No production managed 7-Zip artifact has been selected. |
| Clean-machine WebView2 E2E | **Partial** | CI covers installer startup and uninstall. Full UI automation, setup completion, updater-to-next-version, DPI, and accessibility tests remain. |
