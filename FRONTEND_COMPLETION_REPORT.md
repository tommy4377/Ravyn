# Ravyn Frontend Completion Report

## Scope

This pass rebuilds the main Ravyn frontend around a consistent Windows-inspired shell while leaving backend implementation unchanged. All new data screens use the existing HTTP API; no mock records or placeholder dashboards were introduced.

## Visual system

- Synthetic Fluent material that works consistently on Windows 10 and Windows 11.
- Much brighter light palette and substantially darker dark palette.
- System, light, and dark theme modes with live `prefers-color-scheme` tracking.
- Synthetic and solid material modes, adjustable material intensity, automatic Windows wallpaper alignment, optional custom backdrop override, subtle noise, tint, glow, and Acrylic-like flyouts.
- Adaptive navigation that can collapse to an icon rail.
- Responsive page headers, surfaces, command bars, lists, dialogs, menus, details panes, and high-contrast fallbacks.
- Windows DWM accent integration with separate contrast-safe light/dark palettes, Fluent-style local SVG icons, compact/comfortable density, reduced-motion support, and persistent shell preferences.

## Connected screens

### Downloads

- Responsive download list with simplified filters and contextual bulk actions.
- Direct, media, and torrent creation flows in one typed dialog.
- Media probing, format selection, playlist options, subtitles, thumbnail, and metadata options.
- Torrent probing, file selection, and seeding configuration.
- Resizable details pane with overview, output, activity, segment, retry behavior, native file opening, and Explorer reveal actions.

### Library

- Search, category/state filters, import progress, verification, trash/restore/purge, details, checksum display, duplicate discovery, and configurable cleanup policies.

### Media

- Media download overview, playlist item state, retry of individual or failed items, and anti-duplicate archive management.

### Torrents

- Managed torrent list, engine and DHT metrics, detail snapshots, files, peers, manual peer addition, selected-file updates, seeding state, and safe removal options.

### Basket

- Add multiple sources, persistent ordering, item removal, clearing, and batch start.

### Automation

- Rules, schedules, enable/disable, run-now, deletion, and execution history with cancellation for active runs.

### Components

- Install, cancel, update, verify, rollback, cleanup, remove, version visibility, health feedback, and managed-path safeguards.

### Settings

- Appearance, synthetic material, density, paths, library behavior, provisioning, performance, bandwidth, retries, and timeouts.
- Reusable download presets and activatable settings profiles.
- Backend validation before persistence and reset flow.

### Application updates

- Passive signed-update status in Settings, including checking, download progress, readiness, and install-on-exit behavior.
- Browser-only development hides the native update section instead of showing a broken control.

### Diagnostics

- Readiness, database integrity, dependencies, capabilities, audit chain, maintenance, backups, backup verification, staged restore/cancellation, audit activity, and host reliability history.

## Transport and state corrections

- JSON bodies returned with HTTP `202 Accepted` are retained instead of being discarded.
- `204 No Content` remains bodyless.
- Readiness `503` responses can be consumed as structured status data.
- New frontend contracts mirror existing backend records instead of using untyped objects.

## Validation

- `npm run check`: 0 errors, 0 warnings.
- `npm run test`: 67 tests passed.
- `npm run build`: production build completed successfully.

## Native integration completed in this pass

The synthetic material remains frontend-rendered, but it is now driven by a restricted Tauri bridge that caches the current Windows wallpaper, aligns it to monitor/window coordinates, follows window movement and DPI changes, reads the DWM accent color, and exposes safe open/reveal actions for existing local output paths. See `docs/SYNTHETIC_BACKDROP.md`.

## Remaining external boundaries

Browser-extension capture and secure secret entry still depend on external/native workflows. Per-monitor *different* wallpaper selection is the remaining appearance edge case; the current bridge handles the current wallpaper, all standard positioning modes, and virtual-desktop Span geometry. Setup installation reporting and passive signed application-update status are connected to the desktop shell.
