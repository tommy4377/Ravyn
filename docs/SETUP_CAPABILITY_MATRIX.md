# Ravyn setup capability matrix

Last verified: 2026-07-16.

Status values: `connected` means the real backend operation is wired to the UI;
`tested` means the repository contains automated coverage or the release smoke
suite exercises the path.

## Backend-to-UI coverage

| Capability | Backend exposure | UI | Status |
|---|---|---|---|
| Setup state and mode detection | `GET /v1/setup`, `setup_installation_info` | Detect gate and welcome | tested |
| Setup profiles and feature selection | `GET /v1/components`, `POST /v1/components/features` | Profile and feature stages | tested |
| Library validation and creation | `POST /v1/setup/library`, `pick_folder` | Library stage | tested |
| Runtime preferences | `GET /v1/settings`, `PATCH /v1/settings` | Setup preferences and full Settings sections | tested |
| Managed component install/update | component install/update routes | Provisioning stage and Components screen | connected |
| Managed component cancellation | component cancel route | Provisioning cancel action | connected |
| Managed component verification | component verify route | Components verify action | connected |
| Managed rollback and cleanup | rollback and cleanup routes | Components actions | tested |
| Managed component removal | component delete route | Components remove confirmation | connected |
| Signed catalog status/refresh | component manifest routes | Catalog status and refresh action | tested |
| Live provisioning progress | replayable `/v1/events` SSE | Per-component progress | tested |
| Windows integration | `apply_windows_integration` | Setup preferences/install phase | tested |
| Setup completion and handoff | `POST /v1/setup/complete`, `finish_setup_handoff` | Completion stage | tested |
| System capabilities/dependencies | system diagnostics routes | Diagnostics and Troubleshooting | connected |
| Uninstall | `Ravyn.exe --uninstall` | Installed Apps registration | tested in release smoke flow |

## Component model

Features:

- `standard_downloads` — Ravyn core;
- `video_extraction` — yt-dlp;
- `media_merging` — FFmpeg;
- `torrent_support` — rqbit;
- `archive_extraction` — 7-Zip.

Managed component identifiers are `ytdlp`, `ffmpeg`, `rqbit`, and `seven_zip`.
The built-in Windows x64 catalogue contains all four components. 7-Zip uses a
verified MSI administrative extraction into Ravyn's private engine directory;
it does not register a machine-wide 7-Zip installation.

Component states are `not_installed`, `queued`, `downloading`, `verifying`,
`installing`, `installed`, `update_available`, `failed`, `unsupported`,
`cancelled`, `custom_path`, and `custom_path_invalid`. Every state has a UI
label, accessible status, allowed action set, and recovery path.

## Setup preference contract

The setup preference stage persists its values before provisioning begins. It
currently writes automatic organization, automatic component provisioning,
maximum active downloads, and the optional global speed limit. The main
Settings UI exposes the remaining persisted runtime controls, including
bandwidth windows, circuit breakers, extraction limits, media/torrent/API
bounds, image conversion, cookie directory, and library category overrides.

## Validation evidence

The repository gates the setup surface with:

- strict Svelte/TypeScript checking;
- Vitest controller and state tests;
- exact frontend API-to-Axum and Tauri command/capability parity checks;
- in-memory application of every SQLite migration;
- Windows release installation, readiness, and uninstall smoke tests;
- signed component catalogue validation and Windows provisioning checks.

The browser extension is intentionally outside the desktop completion scope.
The backend browser-token/import routes remain available for a future extension.
