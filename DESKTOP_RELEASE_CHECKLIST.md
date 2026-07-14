# Desktop Release Checklist

“Implemented” means the source path exists. Windows CI and clean-machine tests
remain authoritative for native compilation, installer behavior, signatures,
and real WebView2 operation.

| Item | Status | Notes |
|---|---|---|
| Frontend validation | **Implemented and locally verified** | `svelte-check` 0/0, 104/104 Vitest tests, Vite production build succeeds. |
| Static contract audit | **Implemented and locally verified** | 149 Axum/OpenAPI operations match; 131 frontend HTTP operations and 14 frontend Tauri invokes are backed by registered, permissioned native/backend contracts; JSON/TOML parse, 26 SQLite migrations apply in memory, and Rust syntax parses. |
| UI stack integrity | **Implemented and locally verified** | No React, Tailwind, shadcn, second icon set, or duplicate component framework. |
| Tauri dev/build commands | **Fixed in source** | Native execution still requires Windows Rust/Tauri tooling. |
| Tauri desktop bundle | **Implemented in CI** | Current-user NSIS and MSI bundles plus portable ZIP. |
| Windows installer smoke test | **Implemented in CI** | Silent install, executable/registry/uninstaller checks, generated helper parsing, backend readiness probe, uninstall, and cleanup. |
| Setup/install reporting | **Implemented** | Mode, executable, version, SHA-256, integrations, and relaunch state persist before completion. |
| Component catalog/provisioning | **Implemented** | Signed remote catalog and transactional yt-dlp/FFmpeg/rqbit operations; production keys required. |
| Library import reliability | **Implemented** | Bounded, symlink-safe, warning-tolerant, cancellable, audited, truncation-aware. |
| Physical Library relocation | **Implemented in source** | Disk preflight, conflict policy, durable journal, verified copy, cancellation, job blocking, restart recovery/finalization, Trash-path preservation and rollback. Native fault-injection E2E remains open. |
| Silent signed app updater | **Implemented in source** | Immediate plus periodic checks, bounded backoff, cancellable staging, discard, install-on-close or restart-now, readiness, journal, rollback, repair, startup recovery, and result history. |
| Updater native proof | **Open release gate** | Run real N-to-N+1, forced rollback, interruption, and repair on clean Windows VMs. |
| Windows code signing | **Workflow implemented; credentials pending** | PFX import, signing, timestamp and publisher verification exist. |
| Portable mode | **Implemented** | Explicit policy; installed-app updater disabled in portable mode. |
| Synthetic Windows backdrop | **Implemented in source** | Native visual/DPI/multi-monitor validation remains. |
| Clean-machine WebView2 E2E | **Partial** | Full UI, download, keyboard, DPI, accessibility, provisioning, repair and rollback automation remains. |
