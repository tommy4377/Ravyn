# Conventions
- English docs/comments/API/migration/status text; rustfmt and strict Clippy.
- Prefer typed errors, checked conversions for external values, transactions for related DB changes, bounded channels/inputs, deterministic tests, backward-compatible API evolution.
- Avoid runtime unwrap/expect/panic in production, warning suppression, placeholder handlers, and sensitive config exposure.
- Growing API collections require bounded pagination; retriable mutations require idempotency plus fingerprint conflict detection.
- Preserve dirty worktree changes and established HTTP/media/torrent/automation behavior.
- Releases remain GitHub-only; do not introduce Azure, external signing services, certificate infrastructure, MSI, or non-GitHub release services.