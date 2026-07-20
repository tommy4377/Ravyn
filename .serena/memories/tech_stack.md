# Stack
## Backend (root crate `ravyn`, 0.3.0)
- Rust 2024 edition, MSRV 1.85.
- Axum 0.8, Tokio 1.49, Reqwest 0.13.4 with rustls ring provider, SQLx 0.8 SQLite, Tower HTTP 0.6, Serde, tracing.
- Additional: ed25519-dalek 2.2, chrono/chrono-tz, clap 4 (derive+env), sha2, hex, uuid (v4+serde), url, regex, quick-xml, futures-util, tokio-util, tokio-stream, keyring =3.6.3 (pinned), percent-encoding, zip 2 (deflate only).
- Platform: windows-sys 0.61 (Windows), libc 0.2 (Unix).
- Dev: Criterion 0.7 (async Tokio benches), proptest, tempfile; cargo-fuzz workspace (excluded from main workspace) with 12 binaries (including native_message).
- External adapters (child processes): yt-dlp, FFmpeg, 7-Zip/ImageMagick-compatible converter, rqbit HTTP API.

## Desktop shell (`src-tauri`, crate `ravyn-desktop`, 0.3.0)
- Tauri 2 (`protocol-asset` feature), tauri-plugin-dialog 2, tauri-plugin-mcp-bridge 0.12 (debug-only).
- Depends on root `ravyn` crate as a path dependency (same Cargo workspace, `members = ["src-tauri"]`).
- winreg 0.55 on Windows for registry-backed installed-app integration.
- percent-encoding 2 (for native messaging URL encoding).
- reqwest with "blocking" feature (for native messaging HTTP calls).

## Frontend (`frontend/`, `ravyn-frontend`, 0.3.0)
- Svelte 5 (runes, `^5.39`) + Vite 6 + TypeScript 5.8, `svelte-check` for strict typechecking, vitest 3 for tests.
- `@tauri-apps/api` ^2.9, `@tauri-apps/plugin-dialog` ^2.4 — the only external runtime deps (no component-library dependency; UI primitives are hand-built).
- Fluent Design 2 styling via `styles/tokens.css`.

## Extension (`extension/`, Firefox MV3, 0.3.0)
- TypeScript, web-ext for packaging/lint, vitest for tests, deterministic build via Python `deterministic-zip.py`.
- Background worker with download interception, context menus, native messaging client, network observer, rules engine.
- Content scripts for DOM resource scanning, media source collection, HLS/DASH detection, and media overlays.
- Confirmation page, popup, options page, and sidebar HTML pages.
- Test pages (`test-pages/`) served via Node `server.mjs` for integration tests.
- Static locales: en, it.

## Cross-cutting
- Windows/PowerShell primary development environment; GitHub-only CI/releases (archives, checksums, SBOM, GitHub attestations) — never introduce Azure/external signing/certificate infra/MSI.
- Managed engine infra: `EngineManager` with SHA-256 verified download, atomic install, rollback (backend); paired with the desktop-side updater in `src-tauri/src/app_updates.rs`.
- Extension CI: build, lint, deterministic package verification, version sync checks, AMO unlisted signing on tagged releases.
