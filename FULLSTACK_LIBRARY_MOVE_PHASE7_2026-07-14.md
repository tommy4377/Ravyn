# Ravyn Full-Stack Library Move — Phase 7

Date: 2026-07-14

## Implemented

- Transactional physical Library-root relocation.
- Destination/root validation and nesting prevention.
- Disk-space preflight with a safety reserve.
- Conflict policies for fail-fast or checksum-identical reuse.
- Durable SQLite transaction and per-file journal.
- Chunked copy through temporary files with SHA-256 verification and fsync.
- Cooperative cancellation with cleanup of Ravyn-created destination files.
- New download rejection and queued-job claim blocking during relocation.
- Serialized Library maintenance to prevent import/cleanup/move races.
- Persistent settings and Library path activation in one SQLite transaction.
- Startup recovery for interrupted copies.
- Restart-time destination verification, source cleanup, or automatic rollback.
- Separate logical restore paths and physical payload paths for trashed entries.
- Typed Axum/OpenAPI/Svelte client contract and Settings UI.
- Static audit now applies every SQLite migration to an in-memory database.

## API

- `POST /v1/library/move/preflight`
- `GET /v1/library/move`
- `POST /v1/library/move`
- `DELETE /v1/library/move`

## Local verification

- Axum/OpenAPI: 149 operations in exact parity.
- Typed frontend client: 131 operations, all routed.
- SQLite migrations: 26 applied in memory.
- Rust syntax: 114 files parsed.
- Svelte check: 0 errors, 0 warnings.
- Vitest: 101/101 tests passed.
- Vite production build: completed.

## Native validation still required

Cargo/Tauri and Windows filesystem testing are not available in this environment.
The release gate still requires real compile, locked-file, low-space, interruption,
long-path, removable-drive, antivirus, restart, and forced-rollback scenarios.
