# Ravyn

Ravyn is a download manager with a Rust backend and a native Windows desktop application. The backend exposes a local HTTP API and supports direct HTTP downloads, media through yt-dlp, BitTorrent through rqbit, automation, post-processing, and a persistent organized download library.

## Desktop application and setup

The repository contains four product surfaces:

- the root `ravyn` crate — the backend (HTTP API, engines, storage);
- `src-tauri/` — the `ravyn-desktop` Tauri 2 shell that embeds the backend in-process on an ephemeral loopback port and hosts the setup and main windows;
- `frontend/` — the Svelte 5 + Vite frontend (Fluent Design 2 tokens, custom setup flow);
- `extension/` — the Firefox Manifest V3 extension, resource picker, safe download interceptor, and deterministic AMO packaging.

Frontend development:

```text
cd frontend
npm install
npm run check   # svelte-check, strict TypeScript
npm run test    # vitest
npm run build   # production bundle
```

Desktop shell development (starts the Vite dev server automatically when using the Tauri CLI, or run `npm run dev` manually and then start the exe):

```text
cargo build -p ravyn-desktop
target/debug/ravyn-desktop.exe
```

On Windows 11 22H2 and later, the main and setup windows use the real
compositor acrylic backdrop. Windows 10 and earlier Windows 11 builds have no
stable compositor backdrop, so the windows stay opaque there and Ravyn renders
an equivalent wallpaper-based material itself. See
[`docs/WINDOWS_BACKDROP.md`](docs/WINDOWS_BACKDROP.md) for the rendering and
compatibility details.

Windows distribution is a single self-installing `Ravyn.exe`: running the downloaded executable opens the custom Ravyn setup, which can copy the application into the per-user location (`%LOCALAPPDATA%\Ravyn`, no elevation), register it in Installed Apps, and create the requested shortcuts — or run fully portable. There is no separate MSI/NSIS installer. The shell stores application data under `%LOCALAPPDATA%\Ravyn` (override with `RAVYN_DATA_DIR`); after setup completes it opens the main window.

## Firefox extension

The Firefox extension delegates downloads through a restricted Native Messaging mode in the installed Ravyn executable. New installations intercept compatible downloads by default, with rule-based, confirmed, and disabled modes available in Options; it also supports link/image/media context menus, page resource scanning, an optional network observer, per-site cookie grants, icon-only media overlays, and a compact popup resource picker with batch submission.

```text
cd extension
npm ci
npm run check
npm run package:verify
```

The generated unsigned XPI and human-readable source archive are written to `extension/artifacts/`. Normal Firefox installation requires Mozilla signing; tagged CI releases support unlisted AMO signing with repository credentials.

## Current capabilities

- Strict range validation, segmented HTTP transfers, safe fallback, and persistent resume.
- Independent file handles, dynamic work stealing, and bounded global/per-host concurrency.
- Global and per-job bandwidth limits, persistent host profiles, and circuit breakers.
- Persistent SQLite queue, priorities, tags, rules, schedules, checksums, and output lineage.
- Pause, resume, cancel, retry, delete, recovery, and graceful shutdown.
- yt-dlp probing/downloads with persistent playlist-item state, archive-based deduplication, partial completion, and selective retry.
- rqbit lifecycle/statistics/file selection with persisted ratio/time seeding policies and API capability reporting.
- FFmpeg conversion, dedicated AVIF fallback, 7-Zip extraction, move, open, and original-file retention actions.
- Bulk jobs and bounded text imports with per-item results.
- One-shot, interval, and cron schedules, including scheduled page imports.
- REST API, replayable server-sent events, OpenMetrics, readiness checks, integrity checks, and online database backups.

## Organized Ravyn library

On first startup Ravyn creates an organized library automatically. Unless `--library-root` or `RAVYN_LIBRARY_ROOT` is set, the root is:

- Windows: `%USERPROFILE%\Downloads\Ravyn`
- Linux and macOS: `$HOME/Downloads/Ravyn`
- Portable/test deployments with an explicit `--download-dir`: `<download-dir>/Ravyn`

The root contains:

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

When automatic organization is enabled, jobs without an explicit destination are routed by extension and job type before transfer. Direct HTTP primary files are classified again after completion using MIME information and bounded local magic-byte inspection, so a generic or misleading filename can be moved into the correct category without overriding explicit user destinations. Operator-defined extension overrides take precedence. Set `--library-auto-organize false` to keep the normal download directory behavior while retaining the persistent library index.

The library implementation includes:

- permanent searchable records for downloaded and imported files;
- SHA-256 identity, duplicate candidate lookup, and verified local cache reuse;
- filename templates with safe per-segment sanitization and preview;
- reusable presets and profile-specific settings overlays;
- a deferred download basket with stable ordering and batch start;
- bounded folder import, missing-file verification, and hash-based relocation repair;
- managed trash, restore, permanent purge, and retention policies;
- explainable source/artifact trust reports, including optional Ed25519 verification;
- user-facing storage, activity, average-speed, and saved-bandwidth statistics.

## Library API overview

The additive `/v1` API includes:

- `/v1/library`, `/v1/library/duplicates`, `/v1/library/import`, `/v1/library/verify`, and `/v1/library/relocate`;
- `/v1/templates/preview`;
- `/v1/presets` and `/v1/profiles`;
- `/v1/basket`, `/v1/basket/reorder`, and `/v1/basket/start`;
- `/v1/trust/preview` and `/v1/jobs/{id}/trust`;
- `/v1/system/cleanup-policies`, `/v1/system/cleanup`, and `/v1/statistics`.

The generated document at `/openapi.json` is the authoritative contract.

## External programs

- `yt-dlp` for supported media sites.
- `rqbit` for BitTorrent and magnet links.

On Windows, **Settings > Browser integration > Torrent default app** registers Ravyn as a candidate for `.torrent` files and `magnet:` links, then opens Windows Default Apps so the user can make the final selection.
- `ffmpeg` for audio/video conversion.
- `7z` for archive extraction.

Paths are configurable through environment variables or command-line arguments.

## Run

```bash
ravyn --data-dir ./ravyn-data --listen 127.0.0.1:47821
```

A custom library location can be selected at startup:

```bash
ravyn \
  --data-dir ./ravyn-data \
  --library-root /path/to/Ravyn
```

The API binds to loopback by default. Non-loopback binding requires explicit opt-in, a global bearer token, and `--remote-api-behind-tls-proxy`; the listener must be behind a trusted TLS reverse proxy.

## Validation gate

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

The repository also contains migration, HTTP integration, fuzz-target, and release-build workflows. See `AGENTS.md` for the full verification contract.
