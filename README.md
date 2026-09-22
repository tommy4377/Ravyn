<div align="center">

# Ravyn

**A fast, native download manager for Windows with browser integration, media tools and a persistent library.**

[![CI](https://github.com/tommy4377/Ravyn/actions/workflows/backend-ci.yml/badge.svg)](https://github.com/tommy4377/Ravyn/actions/workflows/backend-ci.yml)
[![Release](https://img.shields.io/github/v/release/tommy4377/Ravyn?display_name=tag&sort=semver)](https://github.com/tommy4377/Ravyn/releases/latest)
[![License](https://img.shields.io/github/license/tommy4377/Ravyn)](LICENSE)
![Windows](https://img.shields.io/badge/desktop-Windows%2010%20%7C%2011-0078D4?logo=windows11&logoColor=white)

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/Tauri%202-24C8DB?logo=tauri&logoColor=white)
![Svelte](https://img.shields.io/badge/Svelte%205-FF3E00?logo=svelte&logoColor=white)
![Firefox](https://img.shields.io/badge/Firefox-extension-FF7139?logo=firefoxbrowser&logoColor=white)

</div>

Ravyn is a download manager built around a Rust backend and a Tauri 2 desktop application. It combines direct HTTP downloads, segmented transfers, media downloads through yt-dlp, BitTorrent through rqbit, post-processing, scheduling, browser integration and a persistent organized library in one native-first application.

## Highlights

- **Direct and segmented HTTP downloads** with resume, retries, checksums and bounded concurrency.
- **Media downloads** through yt-dlp with playlist support and selective retry.
- **BitTorrent and magnet support** through rqbit.
- **Post-processing** with FFmpeg and 7-Zip, including conversion and extraction workflows.
- **Persistent library** with categories, search, duplicate detection, trash, relocation repair and statistics.
- **Automation** with priorities, tags, rules, schedules and batch operations.
- **Firefox integration** through restricted Native Messaging.
- **Compact desktop experience** with tray controls, notifications and Windows integration.
- **Portable-first distribution** as a single `Ravyn.exe`.

## Download

Download the latest Windows build from [GitHub Releases](https://github.com/tommy4377/Ravyn/releases/latest).

```text
Ravyn.exe
```

Requirements:

- Windows 10 or Windows 11, 64-bit
- Microsoft Edge WebView2 Runtime
- No installer is required for portable use

Running `Ravyn.exe` can either start Ravyn portably or install it for the current user under `%LOCALAPPDATA%\Ravyn`.

## Desktop application

The desktop application embeds the backend in-process and exposes the Svelte frontend through Tauri.

```text
Ravyn/
├─ src/          Rust backend, download engines and API
├─ src-tauri/    Tauri desktop shell and Windows integration
├─ frontend/     Svelte 5 desktop frontend
├─ extension/    Firefox Manifest V3 extension
├─ migrations/   SQLite schema migrations
├─ assets/       Managed component metadata and product assets
├─ tests/        Integration tests
└─ tools/        Validation and release tooling
```

On supported Windows 11 versions Ravyn can use the compositor-backed acrylic material. Older Windows versions fall back to an opaque or application-rendered material.

## Download engines

Ravyn supports:

- direct HTTP/HTTPS transfers;
- segmented downloads with dynamic work distribution;
- yt-dlp media probing and downloading;
- rqbit torrents and magnet links;
- FFmpeg conversion;
- 7-Zip extraction;
- checksums and output lineage;
- pause, resume, cancel, retry and recovery.

External engines are managed as components and validated before use.

## Organized library

By default Ravyn creates an organized library under:

```text
%USERPROFILE%\Downloads\Ravyn
```

Typical categories include Downloads, Videos, Music, Documents, Images, Archives, Torrents, Playlists, Temporary and Trash.

The library supports persistent records, duplicate detection, local cache reuse, filename templates, import, relocation repair, managed trash and usage statistics.

## Firefox integration

The Firefox extension communicates with the installed desktop application through Native Messaging. It can intercept compatible downloads, send links and media to Ravyn, scan page resources and expose configurable browser rules.

Extension development:

```bash
cd extension
npm ci
npm run check
npm run package:verify
```

## Backend API

The backend exposes a local `/v1` HTTP API and replayable server-sent events. The generated `/openapi.json` document is the authoritative API contract.

The API binds to loopback by default. Non-loopback binding requires explicit opt-in, authentication and deployment behind a trusted TLS reverse proxy.

## Build from source

Prerequisites:

- Rust 1.85 or newer
- Node.js 22 or newer
- Tauri 2 Windows prerequisites for the desktop build
- Microsoft Edge WebView2 Runtime

Backend:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo build --locked --release -p ravyn
```

Frontend:

```bash
npm ci --prefix frontend
npm run check --prefix frontend
npm test --prefix frontend
npm run build --prefix frontend
```

Desktop:

```bash
cargo build --release -p ravyn-desktop
```

The Windows desktop executable is produced as:

```text
target/release/Ravyn.exe
```

## Repository health

- CI validates Rust on Windows, Linux and macOS.
- Frontend and Firefox extension checks run independently.
- Supply-chain checks use cargo-audit and cargo-deny.
- Nightly jobs exercise fuzz targets, migrations and managed component validation.
- Tagged releases are version-synchronized across the backend, desktop, frontend and extension.

See [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), [SUPPORT.md](SUPPORT.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Maintainer

Maintained by [@tommy4377](https://github.com/tommy4377).

## License

Released under the [MIT License](LICENSE).
