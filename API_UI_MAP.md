# API-to-UI Map

Last reconciled with source: 2026-07-14

The static audit currently verifies 149 Axum/OpenAPI method-path pairs in exact
parity and 131 typed frontend client operations backed by real router entries.

## Downloads and import

- `/v1/jobs*`, bulk actions, outputs, segments, logs, and `/v1/events` →
  Downloads workspace, command bar, virtualized rows, selection actions, and
  details pane.
- `POST /v1/jobs/metalink` → dedicated Metalink import dialog.
- `POST /v1/jobs/batch` and `POST /v1/jobs/import-text` → dedicated batch import
  dialog and Batch queue.
- `/v1/trust/preview` and job trust → source guidance and Advanced details.

## Library

- `/v1/library*` → Files, Trash, Duplicates, details, restore and destructive
  actions.
- `GET/POST/DELETE /v1/library/import` → resumable status display, bounded import,
  and cooperative cancellation.
- `/v1/library/verify` and `/v1/library/relocate` → verification and Find moved
  files for checksum-based record repair.
- `POST /v1/library/move/preflight` and `GET/POST/DELETE /v1/library/move` →
  Settings → Storage and Library physical move dialog, progress, cancellation,
  restart activation, recovery, and rollback state.
- `/v1/system/cleanup-policies`, `/v1/system/cleanup`, and `/v1/statistics` →
  Storage and Library settings plus Library statistics.
- `/v1/tags*`, job tags, presets, profiles, and template preview → General
  settings, job details, and preset editor.

## Media and torrents

- `/v1/media/probe`, archive, item summary/list/outputs/retry → Media workspace and
  Add Download.
- `/v1/torrents*`, engine/DHT, details, peers, files, seeding, and remove →
  Torrents workspace, file tree, engine dialog, and Add Download.

## Automation

- `/v1/rules*` including preview → visual rule builder and before/after preview.
- `/v1/schedules*` and `/v1/schedule-executions*` → schedule editor, execution
  history, run-now, enable/disable, details, and cancellation.

## Settings, tools, diagnostics, and setup

- `/v1/settings*`, presets and profiles → categorized Settings.
- `/v1/components*` → setup and Settings → Tools.
- readiness, database, backups, restore, hosts, audit, dependencies,
  capabilities, maintenance, and statistics → Troubleshooting/advanced panels.
- `/v1/secrets*` → type-specific secure-secret editors; secret values are never
  returned by the API.
- `/v1/setup*` → setup controller, installation handoff, and completion guard.

## Native boundaries

- Wallpaper/accent sampling, file/open-folder/Explorer actions, installation
  reporting, updater status/check/repair, and main-window readiness use
  restricted Tauri commands.
- Browser token/page/sniff/import routes remain intentionally unexposed until
  the extension phase.
