# API-to-UI Map

Last reconciled with source: 2026-07-14

## Setup and components

- `/v1/setup*` → setup controller and installation handoff.
- `/v1/components*` → setup component selection and the Components screen.
- `GET/POST /v1/components/manifest` → signed catalog status and manual refresh
  on the Components screen.

## Downloads

- `/v1/jobs*`, bulk actions, outputs, segments, logs, and SSE `/v1/events` →
  Downloads workspace, virtualized rows, command bar, details pane, and stores.
- Direct, media, and torrent creation are exposed through the typed Add dialog.
- Metalink and some advanced batch/template surfaces remain secondary API-only
  capabilities.

## Library and basket

- `/v1/library*`, duplicate discovery, import, verify, relocate, cleanup
  policies, cleanup, and statistics → Library screen.
- `/v1/basket*`, reorder, clear, and start → Basket screen.

## Media and torrents

- `/v1/media/probe`, media archive, media item summary/list/retry → Media screen
  and media creation flow.
- `/v1/torrents*`, engine/DHT stats, details, peers, files, seeding, and remove →
  Torrents screen and torrent creation flow.

## Automation

- `/v1/rules*`, `/v1/schedules*`, and schedule executions → Automation screen.
- Rule preview and some advanced diagnostic tables remain API-only.

## Presets, profiles, and settings

- `/v1/presets*`, `/v1/profiles*`, and profile activation → Settings screen.
- `/v1/settings`, validation, patch, and reset → Settings screen.
- Executable overrides for yt-dlp, FFmpeg, rqbit, and 7-Zip plus the rqbit API
  URL are editable through file-aware controls and validated before save.

## Diagnostics

- `/health/ready`, `/v1/system/database*`, dependencies, capabilities,
  maintenance, audit, backups, restore state, host profiles, and statistics →
  Diagnostics screen.

## Frontend-only shell preferences

Theme, density, material mode/intensity, optional custom backdrop override,
navigation collapse state, and details width are local shell preferences stored
in `localStorage`. Wallpaper geometry and DWM accent are supplied by the
restricted `desktop_appearance` Tauri command.

## External/native boundaries

- Windows wallpaper/accent sampling and Explorer integration use main-window-only
  Tauri commands.
- `app_update_status` and `check_app_update` feed the Settings update section.
- `main_window_ready` confirms backend/webview readiness to the detached updater
  transaction before the previous binary is discarded.
- Secret values are submitted once from the Settings editor to the authenticated
  loopback backend, stored in the operating-system credential manager, and never
  returned by the API. Create, replace, delete, and rqbit binding are connected.
- Browser-extension capture is intentionally outside the current pass.
