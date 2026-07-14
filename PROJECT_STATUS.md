# Ravyn Project Status

Last source audit: 2026-07-14

## Implemented in source

- Rust backend for direct downloads, media, torrents, persistent Library,
  automation, basket imports, component provisioning, diagnostics, settings,
  backups, secure secrets, signed component catalogs, and authenticated loopback
  APIs.
- Svelte 5 desktop frontend connected to the primary backend workflows with a
  focused Windows-style shell, responsive navigation, persistent appearance
  preferences, list/details layouts, keyboard commands, and accessible dialogs.
- Source-first Add Download flow plus dedicated Metalink and batch import
  dialogs, drag-and-drop, duplicate-aware text import, media probing, torrent
  probing, and advanced options hidden behind disclosure controls.
- Downloads, Library, Media, Torrents, Automation, categorized Settings, Tools,
  Troubleshooting, About, secure-secret editors, tag management, filename
  template preview, automation-rule preview, and execution history.
- Library Files/Trash/Duplicates views, moved-file repair, bounded folder import,
  cooperative import cancellation, scan-limit reporting, resilient unreadable
  directory handling, verification, cleanup, typed personal statistics, and a
  transactional physical root move with preflight, checksum verification,
  cancellation, durable recovery, Trash-path preservation, and restart finalization.
- Persistent notification history with unread state and a dedicated drawer.
- Signed installed-app updates with immediate and six-hour scheduled checks,
  bounded retry backoff, cooperative cancellation, staged-package discard,
  install-on-close or explicit restart-and-install, transaction journaling,
  binary/registry/shortcut backup, readiness verification, rollback,
  interrupted-helper recovery, repair mode, and persisted results.
- Tauri setup/main capability isolation, caller validation, CSP, restricted asset
  scope, native file/Explorer actions, installation reporting, and setup
  transition guards.
- Windows release workflows for tests, bundles, generated updater-helper parsing,
  backend readiness smoke checks, Authenticode verification, signed manifests,
  checksums, SBOM, and attestations.

## Locally verified in this environment

- `svelte-check`: 0 errors, 0 warnings.
- Vitest: 104/104 tests passed across 20 test files.
- Vite production build: completed successfully.
- Static source audit:
  - 149 Axum/OpenAPI operations in exact method/path parity;
  - 131 typed frontend client operations, all backed by Axum routes;
  - no React, Tailwind, shadcn, or second UI/icon stack;
  - 14 frontend Tauri invokes are registered, permission-declared, and capability-enabled;
  - 12 JSON and 19 TOML files parsed;
  - all 26 SQLite migrations applied successfully in memory;
  - 114 Rust source files parsed without syntax errors.

Rust/Tauri compilation was not available because Cargo, rustc, rustfmt, the
Windows SDK, and WebView2 are not installed. Source parsing is not a substitute
for compilation. Windows CI and clean-machine testing remain the native source
of truth.

## Effective release blockers still open

1. Run `cargo test --locked --workspace --all-targets` and Tauri debug/release
   builds on Windows.
2. Supply production Authenticode, app-update, and component-manifest signing
   credentials and complete a successful tagged release.
3. Run clean-machine WebView2 automation through setup, component provisioning,
   direct/media/torrent downloads, updater N to N+1, forced rollback, repair,
   DPI, keyboard, high contrast, reduced motion, and accessibility scenarios.
4. Run native Windows E2E for the transactional Library-root move, including
   cancellation, destination conflicts, low disk space, crash recovery, Trash
   restore paths, restart finalization, and forced verification rollback.
5. Add per-monitor different-wallpaper selection through `IDesktopWallpaper`.

## Product decisions and deferred scope

- Browser-extension capture remains intentionally deferred until the desktop
  core is release-validated.
- Ravyn 0.2 uses a system or user-selected `7z.exe`/`7za.exe`; managed 7-Zip
  provisioning remains deferred.
- React, Tailwind, shadcn, and a second icon/component framework are intentionally
  not used.
