# Conventions
- English docs/comments/API/migration/status text; rustfmt and strict Clippy.
- Prefer typed errors, checked conversions for external values, transactions for related DB changes, bounded channels/inputs, deterministic tests, backward-compatible API evolution.
- Avoid runtime unwrap/expect/panic in production, warning suppression, placeholder handlers, and sensitive config exposure.
- Growing API collections require bounded pagination; retriable mutations require idempotency plus fingerprint conflict detection.
- Non-critical background work (library indexing, cleanup) logs warnings and audit records but never aborts the primary operation.
- Filesystem mutations that accompany DB state changes use compensating rollbacks (trash/restore/purge in `services/library/trash.rs`).
- Directory scans (import, relocation, cleanup) are bounded by depth and entry count ceilings, skip symlinks, and exclude internal directories (Trash, Temporary).
- Preserve dirty worktree changes and established HTTP/media/torrent/automation behavior.
- Releases remain GitHub-only; do not introduce Azure, external signing services, certificate infrastructure, MSI, or non-GitHub release services.
