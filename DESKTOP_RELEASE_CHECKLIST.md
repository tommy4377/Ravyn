# Desktop Release Checklist

This file tracks release-critical desktop state. “Implemented” means the source
path exists; Windows CI remains the authority for native compilation, installer
behavior, signatures, and clean-machine verification.

| Item | Status | Notes |
|---|---|---|
| Frontend validation | **Implemented and locally verified** | `svelte-check` reports 0 errors/0 warnings, 67/67 Vitest tests pass, and the Vite production build succeeds. |
| Tauri dev/build commands | **Fixed in source** | Frontend commands resolve correctly from `src-tauri`. Native execution still requires a Windows Rust/Tauri environment. |
| Tauri desktop bundle | **Implemented in CI** | Current-user NSIS and MSI bundles plus a portable ZIP are produced. |
| Windows installer smoke test | **Implemented in CI** | CI silently installs NSIS, verifies executable/registry/uninstaller, launches Ravyn, uninstalls, and verifies cleanup. |
| Version consistency | **Implemented in CI** | Root, desktop, Tauri, frontend, and release-tag versions must match. |
| Tauri command isolation | **Implemented** | Setup and main windows have separate capabilities; sensitive commands validate the caller window in Rust. |
| Content Security Policy | **Implemented** | Production CSP limits scripts, frames, objects, forms, assets, and loopback backend connections. |
| Setup installation reporting | **Implemented** | Mode, executable path, version, SHA-256, integration result, and relaunch state are persisted before setup completes. |
| Portable mode | **Implemented for setup/runtime policy** | Portable mode is explicit and never uses the installed-app updater. A portable updater is intentionally disabled. |
| Synthetic Windows backdrop | **Implemented in source** | Wallpaper/accent sampling, geometry alignment, safe asset scope, and Explorer/file actions are connected. Native visual verification remains in product E2E. |
| Remote signed component manifest | **Implemented in source and release workflow** | HTTPS-only conditional refresh, Ed25519 verification, bounded reads, expiry, replay/downgrade protection, transactional cache activation, last-known-good fallback, API/UI status, and release generation are connected. Deployment requires the release key variables. |
| Silent signed app updater | **Implemented in source and release workflow** | Installed builds verify and stage a signed NSIS installer, apply it after close, require backend/webview readiness, retain the prior binary, roll back on failed readiness, and persist the result. |
| Updater repair and full rollback | **Partial** | Main-executable rollback is implemented. Repair of all installed files/registry/uninstaller state and a real N-to-N+1 Windows E2E are still required. |
| Component executable overrides | **Implemented** | Settings exposes validated paths for yt-dlp, FFmpeg, rqbit, and 7-Zip plus the rqbit API URL. |
| 7-Zip policy for 0.2 | **Decided and implemented** | Archive extraction uses a system/custom `7z` or `7za` path. Managed provisioning is deliberately unavailable until a trusted non-circular bootstrap is selected. |
| Windows code signing | **Implemented in workflow; credentials pending** | Tagged releases import the PFX, configure Tauri, and verify publisher signature and timestamp. Production certificate/secrets and a successful release run remain external prerequisites. |
| Persisted setup consent/idempotency | **Implemented in source** | Exact integration choices persist across restart, identical requests retain their identifier, and backend/Tauri reject mismatched operations. Native Windows validation remains. |
| Clean-machine WebView2 E2E | **Partial** | Installer process smoke tests exist. Full UI setup, download, updater rollback, DPI, keyboard, and accessibility automation remain. |
