# Ravyn Firefox Extension Checkpoint — 2026-07-16

This checkpoint contains the current Ravyn desktop application and the Firefox-first browser integration implementation.

## Implemented

- Firefox Manifest V3 extension written in TypeScript.
- Deterministic extension build, XPI packaging, source packaging, ESLint, Prettier, TypeScript, Vitest, and `web-ext` validation.
- Native Messaging protocol hosted by the installed Ravyn executable.
- Windows Firefox native-host registration, repair, status, and uninstall cleanup.
- Desktop settings UI for Firefox integration.
- Download interception modes with pause-first handoff and loop prevention.
- Link, image, media, selection, and page context-menu actions.
- DOM, mutation, performance-entry, frame, and optional network resource discovery.
- Compact toolbar resource picker with filtering, previews, monitoring, and batch submission.
- Media overlay and yt-dlp fallback for compatible non-protected media.
- Optional per-site cookie access without persistent cookie-value storage.
- Popup, options, confirmation UI, local fixture pages, privacy documentation, threat model, and AMO submission notes.
- CI packaging, reproducibility checks, AMO signing path, native-host installer checks, and release artifact upload.

## Validation completed in this environment

- Extension TypeScript typecheck passed.
- Extension ESLint and Prettier checks passed.
- Firefox `web-ext` validation passed with 0 errors, 0 warnings, and 0 notices.
- 23 extension tests passed across 8 test files.
- Deterministic XPI and source archive verification passed.
- Frontend `svelte-check` passed with 0 errors and 0 warnings.
- 110 frontend tests passed across 22 test files.
- Frontend production build passed.
- Static source audit passed:
  - 149 Axum/OpenAPI operations in exact parity.
  - 131 typed frontend API operations backed by Axum routes.
  - 18 Tauri commands invoked, registered, and capability-permitted.
  - 26 SQLite migrations applied in memory.
  - 117 Rust source files parsed successfully.

## Remaining external validation and release work

The source implementation has no known unfinished Firefox phase. The following work requires infrastructure unavailable in this environment:

1. Run `cargo fmt`, `cargo check`, `cargo clippy`, and `cargo test` with a real Rust toolchain.
2. Run the Windows NSIS/MSI installation, repair, update, rollback, native-host registration, and uninstall smoke tests.
3. Run a real Firefox end-to-end test against an installed Ravyn build, including interception, popup resource scanning, context menus, cookies, private windows, containers, HLS/DASH, and backend restart recovery.
4. Configure Mozilla AMO credentials, submit/sign the XPI, and complete any Mozilla review feedback.
5. Approve final extension store icons, screenshots, listing copy, privacy URL, and support URL.
6. Run the tagged release workflow and verify the signed desktop and extension artifacts on clean Windows 10 and Windows 11 machines.

Chrome support remains deliberately deferred and is not part of this Firefox checkpoint.
