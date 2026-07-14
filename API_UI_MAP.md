# API-to-UI Map

## Setup and components

- `/v1/setup*`, `/v1/components*` → setup controller and Components screen.

## Downloads

- `/v1/jobs*`, job actions, outputs, segments, logs, and SSE `/v1/events` → Downloads screen, details pane, jobs service/store.

## Library

- `/v1/library*`, duplicate discovery, import, verify, relocate, cleanup policies, and cleanup → Library screen.

## Media

- `/v1/media/probe`, `/v1/media/archive`, `/v1/jobs/{id}/media-*` → Add dialog and Media screen.

## Torrents

- `/v1/torrents*`, engine stats, DHT stats, files, peers, seeding, and remove → Add dialog and Torrents screen.

## Basket

- `/v1/basket*`, reorder, and start → Basket screen.

## Automation

- `/v1/rules*`, `/v1/schedules*`, and `/v1/schedule-executions*` → Automation screen.

## Presets and profiles

- `/v1/presets*`, `/v1/profiles*`, profile activation → Settings screen.

## Settings

- `/v1/settings`, validation, patch, and reset → Settings screen.

## Diagnostics

- `/health/ready`, `/v1/system/database*`, dependencies, capabilities, maintenance, audit, backups, restore state, and host profiles → Diagnostics screen.

## Frontend-only shell preferences

Theme, density, material mode/intensity, optional custom backdrop override, navigation collapse state, and details width are local shell preferences persisted in `localStorage`. The system wallpaper geometry and DWM accent are supplied by the restricted `desktop_appearance` Tauri command.

## External/native boundaries

Windows wallpaper/accent sampling and Explorer integration are connected through main-window-only Tauri commands. Browser-extension capture and secure secret workflows remain external boundaries; per-monitor different-wallpaper selection remains an appearance edge case.

## Native application updates

`app_update_status` and `check_app_update` → Settings application-update section. The desktop shell owns background download, signature/hash verification, staging, and install-on-exit.
