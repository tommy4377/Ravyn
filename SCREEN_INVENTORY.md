# Screen Inventory

The main window is a connected multi-page shell. All entries below are reachable from `NavigationView.svelte` and hosted by `AppShell.svelte`.

## Shell

- Adaptive expanded/compact navigation.
- Synthetic Fluent backdrop and solid fallback.
- Persistent theme, density, material, navigation, and details-pane preferences.
- Backend boot/retry state, application-wide SSE subscription, notifications, and status bar.

## Downloads

- Search, simplified status views, sorting/filtering, contextual selection actions, responsive rows, and resizable details.
- Add dialog supports direct downloads, media analysis, and torrent analysis.

## Library

- Search and filtering, import, verification, trash, restore, purge, duplicate search, cleanup policies, and details.

## Media

- Media jobs, playlist/item state, retry operations, and media archive management.

## Torrents

- Torrent list, engine/DHT status, detail snapshots, files, peers, seeding, and removal.

## Basket

- Deferred item creation, ordering, removal, clear, and start-all flow.

## Automation

- Rule and schedule CRUD, enable/disable, run-now, and execution history/cancellation.

## Components

- Component status, installation, cancellation, update, verification, rollback, cleanup, and removal.

## Settings

- Appearance, synthetic material, storage, download engine, network/recovery, presets, profiles, validation, persistence, and reset.

## Diagnostics

- Runtime health, database, audit, dependencies, capabilities, maintenance, backups, restore status, and host reliability.

## Setup

- Existing staged setup remains the entry path before the main shell and continues to use real setup/component APIs.

For implementation detail and validation status, see `FRONTEND_COMPLETION_REPORT.md`.
