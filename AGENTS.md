# AGEMTS.MD

This file provides guidance to the AI agent.

## Repository overview

Ravyn is a download manager with three parts in one Cargo workspace:

- **Root crate `ravyn`** (`src/`) — the backend: a local HTTP API (axum) with download engines, automation, post-processing, and a persistent organized library over SQLite (sqlx).
- **`src-tauri/` (`ravyn-desktop`)** — the Tauri 2 shell. It embeds the backend **in-process** on an ephemeral loopback port and decides whether to open the custom setup window or the main window based on the backend's `/v1/setup` state.
- **`frontend/`** — Svelte 5 (runes) + Vite + TypeScript, Fluent Design 2 styling. Serves both the setup flow and the main application.

`fuzz/` is excluded from the workspace. The dev database lives at `ravyn-data/ravyn.sqlite3` (the desktop shell uses `%LOCALAPPDATA%\Ravyn`; both respect `RAVYN_DATA_DIR`).

## Commands

### Backend (repo root)

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --test http_integration        # HTTP integration suite (real loopback server)
cargo test --locked <test_name>                    # single test by name filter
cargo build --locked --release                     # release build gate
```

All of the above must pass before claiming backend completion.

### Frontend (`cd frontend`)

```bash
npm install
npm run check                  # svelte-check, strict TypeScript
npm run test                   # vitest run
npx vitest run src/lib/stores/jobs.test.ts   # single test file
npm run build                  # production bundle
npm run dev                    # Vite dev server
```

### Desktop shell

```bash
cargo build -p ravyn-desktop
target/debug/ravyn-desktop.exe
```

Before claiming frontend/setup completion: formatting, `npm run check`, tests, production build, and a real Tauri smoke test; verify all visible controls use real backend data and setup-to-main-app handoff is deterministic.

### Running the backend standalone

```bash
cargo run -- --data-dir ./ravyn-data --listen 127.0.0.1:47821
```

## Architecture

### Backend (`src/`)

Bootstrap is `Ravyn::bootstrap` in `src/lib.rs`: it validates `config::Config` (CLI/env via clap), applies pending database recovery, connects the `storage::Repository`, recovers interrupted library moves, overlays persistent settings from the database, and builds the `core::manager::JobManager` plus background services. Three config layers exist and are all kept on the `Ravyn` struct: `base_config` (CLI/env), `configured_config` (after persistent DB overrides), and `config` (after managed-engine path substitution).

- `src/api/` — axum routes (`routes/jobs.rs`, `library.rs`, `media.rs`, `torrents.rs`, `automation.rs`, `system.rs`, `setup.rs`, `components.rs`, `browser.rs`), generated OpenAPI (`/openapi.json` is the authoritative contract), pagination. The API also serves replayable server-sent events, OpenMetrics, readiness, and backups.
- `src/core/` — job lifecycle: `manager.rs`/`dispatcher.rs` (queue, priorities, bounded global/per-host concurrency), `bandwidth.rs`/`rate_limit.rs`, `events.rs` (event bus behind SSE), `automation.rs`, `metrics.rs`, `models.rs`.
- `src/download/` — direct HTTP engine: `probe.rs`, `planner.rs`, `segmented.rs` (segmented transfers with safe fallback and persistent resume).
- `src/adapters/` — external engines: `media` (yt-dlp) and `torrent` (rqbit), driven as child processes via `services/process.rs` / `services/rqbit_process.rs`.
- `src/services/` — cross-cutting services: `components.rs` (managed component provisioning/verification), `library/` (organized library: categories, templates, trash, scan, root moves), `scheduler.rs`/`cron.rs`, `rules.rs`, `trust.rs` (Ed25519 verification), `secrets.rs` (keyring), `sniffer.rs` (MIME/magic-byte classification).
- `src/storage/` — one module per aggregate over sqlx/SQLite; migrations live in `migrations/` (sqlx migrate, `0001`–`0026`) and must stay in sync with code changes.
- `src/postprocess/` — FFmpeg conversion, 7-Zip extraction, move/retention pipeline.

### Desktop shell (`src-tauri/src/`)

`lib.rs` spawns the backend (`backend.rs`) and exposes native commands. The webview obtains the backend base URL and per-run bearer token through the `backend_info` command; all setup-state decisions are re-read from the authenticated backend API (never trusted from the webview). Other modules: `installation.rs`/`integration.rs` (installed-app registration, shortcuts, startup — gated on recorded consent in the backend), `setup_guard.rs`, `app_updates.rs`, `uninstall.rs`, `appearance.rs`. A debug-only `tauri-plugin-mcp-bridge` enables the tauri-mcp tooling.

### Frontend (`frontend/src/`)

- `lib/api/` — `transport.ts` (fetch/timeout/abort), `client.ts` (typed methods per endpoint), `types.ts` (mirrors backend contracts), `errors.ts` (stable backend error codes; network failures map to `NETWORK_UNAVAILABLE`), `events.svelte.ts` (SSE).
- `lib/native/tauri.ts` — the only place Tauri commands are invoked.
- `lib/stores/` — Svelte 5 rune-based stores (`*.svelte.ts`): connection, jobs, navigation, notifications, selection.
- `lib/setup/` — setup flow (`SetupApp.svelte`, `controller.svelte.ts`, `stages/`, installation policy).
- `lib/shell/` — main app chrome (`AppShell.svelte`, navigation, command bar, status bar, notifications).
- Feature areas: `downloads/`, `library/`, `media/`, `torrents/`, `automation/`, `basket/`, `settings/`, `diagnostics/`, `appearance/`.
- Tests are colocated `*.test.ts` files run by vitest.

## Working rules

- Do not commit, do not push. This is a very important command!
- Current scope: Rust backend, Tauri/Svelte frontend, and the custom Ravyn setup. Do not work on the browser extension unless explicitly requested.
- Build every frontend feature as a complete vertical slice: inspect the backend contract, implement the UI, wire the real API/Tauri command and events immediately, handle loading/empty/error/recovery states, add tests, and update documentation. Never leave production mock data, placeholder actions, or visual-only screens.
- Preserve security defaults: loopback binding, output-root confinement, private-network blocking, bounded inputs, verified managed components, atomic replacement, least privilege, and no silent installation of disabled features.
- Keep OpenAPI, migrations, event contracts, setup documentation (`docs/SETUP_CAPABILITY_MATRIX.md`, `docs/COMPONENT_MANIFESTS.md`, `docs/APP_UPDATES.md`), and the README synchronized with every completed change.
- The install directory equals the data directory, so updater rollback must remain binaries-only.
- Write code comments in English and only where they improve maintainability; follow existing architecture and style.

## MCP servers

Use the configured MCP servers when available; if one is unavailable, say so briefly and continue with local tooling:

- `serena` — activate the project and prefer its LSP-backed symbol search/references/edits for Rust, Svelte, and TypeScript.
- `git` — status, diffs, history, staging, commits.
- `sqlite` — inspect `ravyn-data/ravyn.sqlite3` (respect `RAVYN_DATA_DIR` overrides).
- `context7` / `svelte-docs` — current library and Svelte documentation before implementing against third-party APIs.
- `shadcn-svelte` — inspect components before hand-authoring equivalents; skip anything conflicting with Ravyn's native Windows/Fluent design.
- `tauri-mcp` — drive/inspect/debug the running Tauri app (UI, DOM, IPC, events, logs).
