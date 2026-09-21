<div align="center">

# Ravyn

**A native download manager built for fast transfers, media, torrents and an organized local library.**

[![Backend CI](https://github.com/tommy4377/Ravyn/actions/workflows/backend-ci.yml/badge.svg)](https://github.com/tommy4377/Ravyn/actions/workflows/backend-ci.yml)
[![License](https://img.shields.io/github/license/tommy4377/Ravyn)](LICENSE)
![Version](https://img.shields.io/badge/version-0.3.0-4c8bf5)
![Rust](https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)
![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)
![Firefox](https://img.shields.io/badge/Firefox-extension-FF7139?logo=firefoxbrowser&logoColor=white)
![Windows](https://img.shields.io/badge/desktop-Windows-0078D4?logo=windows11&logoColor=white)

</div>

Ravyn combines a Rust download engine with a native Tauri desktop application and a Firefox extension. It handles direct HTTP transfers, supported media sites through yt-dlp, BitTorrent through rqbit, post-processing through FFmpeg, automation, and a persistent download library.

## Highlights

- **Fast, resilient transfers** — segmented HTTP downloads, range validation, resume, retry, work stealing and bounded concurrency.
- **Persistent queue** — priorities, tags, schedules, rules, checksums, recovery and graceful shutdown.
- **Media and torrents** — yt-dlp playlists/media probing, rqbit lifecycle and file selection, seeding policies and statistics.
- **Post-processing** — FFmpeg conversion, archive extraction, move/open actions and output lineage.
- **Organized library** — automatic categorization, SHA-256 identity, duplicate detection, cache reuse, templates, presets, basket and trash.
- **Native Windows app** — Tauri 2 shell, custom setup, system tray, notifications, compact progress window and Windows integration.
- **Firefox integration** — Manifest V3 extension using Native Messaging for interception, resource discovery and context actions.
- **Local API** — authenticated REST endpoints, replayable server-sent events, OpenMetrics, readiness checks and OpenAPI.

## Download

Tagged releases are built by GitHub Actions. The Windows desktop distribution is a single self-installing/portable executable:

```text
Ravyn.exe
```

The same executable can install per-user under `%LOCALAPPDATA%\Ravyn` without elevation or run in portable mode.

## Architecture

| Surface | Technology | Purpose |
| --- | --- | --- |
| `ravyn` | Rust / Axum / SQLite | transfer engine, API, automation, library |
| `src-tauri/` | Tauri 2 / Rust | native Windows shell and integration |
| `frontend/` | Svelte 5 / Vite | desktop UI |
| `extension/` | Firefox MV3 / TypeScript | browser integration via Native Messaging |

## Organized library

By default Ravyn creates a library under the user's Downloads folder:

```text
Ravyn/
├── Downloads/
├── Videos/
├── Music/
├── Documents/
├── Images/
├── Archives/
├── Torrents/
├── Playlists/
├── Temporary/
└── Trash/
```

Explicit destinations always take precedence over automatic organization.

## Build from source

Prerequisites: Rust stable, Node.js, npm and the Tauri 2 Windows prerequisites.

```bash
npm ci --prefix frontend
npm run check --prefix frontend
npm test --prefix frontend
npm run build --prefix frontend

cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets
cargo build --locked --release -p ravyn-desktop
```

For Firefox extension validation:

```bash
npm ci --prefix extension
npm run check --prefix extension
npm run package:verify --prefix extension
```

## Security model

The local API binds to loopback by default. Non-loopback binding requires explicit opt-in, authentication and deployment behind a trusted TLS reverse proxy. Browser integration uses a restricted Native Messaging host rather than exposing the extension directly to the HTTP API.

See [SECURITY.md](SECURITY.md) for vulnerability reporting guidance.

## Repository automation

- Backend, frontend, extension and supply-chain checks run in CI.
- Tagged builds run the release workflow and validate synchronized application/extension versions.
- Windows releases produce the single `Ravyn.exe` distribution plus signed update/component metadata when release credentials are configured.
- Dependabot tracks Cargo, npm and GitHub Actions dependencies.

## Maintainer

Maintained by [@tommy4377](https://github.com/tommy4377).

## License

Released under the [MIT License](LICENSE).
