# Ravyn Full-Stack Hardening — Phase 6

Date: 2026-07-14

## Backend changes

- Added cooperative cancellation for Library folder imports through
  `DELETE /v1/library/import`.
- Added `truncated`, `cancel_requested`, and `cancelled` status fields to the
  Rust model, OpenAPI schema, and TypeScript wire contract.
- Made Library scans continue after unreadable directories and individual
  filesystem errors while retaining bounded warnings.
- Propagated cancellation during SHA-256 calculation instead of recording the
  file as an ordinary skip.
- Associated active cancellation tokens with the import `run_id`, preventing an
  old worker from clearing a newer import's cancellation token.
- Added backend tests for clean cancellation and scan-limit truncation.
- Updated OpenAPI operations and tests.

## Frontend changes

- Added dedicated Metalink and batch-import dialogs.
- Added deterministic batch analysis for comments, duplicate lines, and complete
  JSON job arrays, with unit tests.
- Added Library import tags, maximum-entry and maximum-depth controls.
- Added live Library import cancellation, recovery of status after view reload,
  and clear completion/truncation/warning messages.
- Added typed Library statistics with category storage and monthly activity.
- Added persistent notification history, unread state, status-bar entry, and a
  right-side drawer coordinated with Batch queue and Escape behavior.
- Added notification and navigation tests.

## Source integrity tooling

`tools/static_source_audit.py` checks:

- exact Axum/OpenAPI method-path parity;
- that every typed frontend client call has a backend route;
- absence of React, Tailwind, shadcn, and duplicate UI stacks;
- JSON and TOML syntax;
- Rust syntax when `tree-sitter` and `tree-sitter-rust` are available.

## Verification

- Svelte diagnostics: 0 errors, 0 warnings.
- Vitest: 97/97 tests passed across 18 files.
- Vite production build: successful.
- Static audit: 145 backend/OpenAPI operations, 127 frontend client operations,
  12 JSON files, 17 TOML files, and 112 Rust files parsed successfully.

## Native validation boundary

Cargo, rustc, rustfmt, Windows SDK, and WebView2 were unavailable in this
workspace. Rust syntax and contract audits passed, but native compilation,
installer execution, real downloads, and updater rollback must still be run in
Windows CI or a clean Windows VM.
