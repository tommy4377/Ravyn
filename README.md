# Ravyn Backend

Ravyn is a backend-only download manager written from scratch in Rust. It exposes a local HTTP API and supports HTTP downloads, media through yt-dlp, BitTorrent through rqbit, automation, and post-processing.

## Current capabilities

- Strict range validation, segmented HTTP transfers, safe fallback, and persistent resume.
- Independent file handles, dynamic work stealing, and bounded global/per-host concurrency.
- Global and per-job bandwidth limits, persistent host profiles, and circuit breakers.
- Persistent SQLite queue, priorities, tags, rules, schedules, and checksums.
- Pause, resume, cancel, retry, delete, recovery, and graceful shutdown.
- yt-dlp probing/downloads with persistent playlist-item state, archive-based deduplication, partial completion, and selective retry.
- rqbit lifecycle/statistics/file selection with persisted ratio/time seeding policies and API capability reporting.
- FFmpeg conversion, dedicated AVIF fallback, 7-Zip extraction, move, open, and original-file retention actions.
- Bulk JSON jobs and text URL imports with per-item results.
- Static HTML resource sniffing with extension filters and “only new” history.
- One-shot, interval, and UTC cron schedules, including scheduled page imports.
- Scoped browser tokens with origin allow-lists and a browser import bridge.
- CRUD APIs for rules, tags, schedules, browser tokens, and page history.
- REST API and server-sent events. No UI is included.
- Configurable API timeout, bounded global concurrency, overload rejection, and token-bucket rate limiting.
- DNS-pinned HTTP connections with redirect-by-redirect SSRF validation and HTTP mirror failover.
- Capability-reduced browser imports and redacted sensitive job options in API responses.
- Staged, bounded archive extraction plus a durable, idempotent post-action journal.
- Replayable sequenced SSE, cursor-based job filtering, idempotent job creation, renewable schedule leases, readiness checks, OpenMetrics, integrity checks, and online database backups.

## External programs

- `yt-dlp` for supported media sites.
- `rqbit` for BitTorrent and magnet links.
- `ffmpeg` for audio/video conversion.
- `7z` for archive extraction.

Paths are configurable through environment variables or command-line arguments.

## Validation

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

## Run

```bash
ravyn --data-dir ./ravyn-data --listen 127.0.0.1:47821
```

The API binds to loopback by default. Non-loopback binding requires explicit opt-in, a global bearer token, and `--remote-api-behind-tls-proxy`; the listener must sit behind a trusted TLS reverse proxy and must not be exposed directly. Browser-scoped endpoints additionally require a scoped token and matching origin.

`GET /v1/jobs` returns `{ "items": [...], "next_cursor": "..." }` and accepts `cursor`, `status`, `kind`, `search`, and `limit` query parameters. `POST /v1/jobs` accepts `Idempotency-Key`. Event clients can reconnect with `Last-Event-ID`. Operational endpoints are available at `/health/live`, `/health/ready`, `/metrics`, `/v1/system/database`, and `/v1/system/database/backup`. Media lifecycle endpoints include `/v1/jobs/{id}/media-items`, `/v1/jobs/{id}/media-summary`, `/v1/jobs/{id}/media-items/{item_id}/retry`, `/v1/jobs/{id}/media-items/retry-failed`, and `/v1/media/archive`. Torrent seeding policy state is available at `/v1/torrents/{id}/seeding`.