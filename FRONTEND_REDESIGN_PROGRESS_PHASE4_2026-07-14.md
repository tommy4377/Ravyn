# Ravyn Frontend Redesign — Phase 4 Progress

Date: 2026-07-14

## Scope completed

This checkpoint implements the Library, Media, and Torrents portion of the frontend redesign plan while preserving the existing backend contracts.

## Shared architecture

- Added `ListDetailsLayout.svelte` for a single docked/overlay list-and-details model.
- Added `DetailsPane.svelte` for consistent headers, tabs, close behavior, and scroll handling.
- Library, Media, and Torrents now use `PageScaffold`, `PageCommandBar`, `CompactSummary`, `ListDetailsLayout`, and `DetailsPane`.
- Removed metric-card dashboards from Media and Torrents.
- Main content uses flat surfaces and dividers; floating depth is reserved for dialogs and narrow-window detail overlays.

## Library

- Added Files, Trash, and Duplicates modes.
- Files view now resembles File Explorer with Name, Type, Size, Modified, and Source columns.
- Added persistent sorting for all visible columns.
- Active and missing records are loaded together in Files and Duplicates.
- Added grouped duplicate detection using checksum first and name plus size as a fallback.
- Added Open and Show in Explorer actions.
- Added Undo after moving a file to trash.
- Destructive copy now states whether a file is moved to trash or deleted permanently.
- Removed cleanup policy editing from the main Library page. It belongs in Settings → Storage and Library.
- Added a dedicated `Find moved files` flow with current root, scan root, entry/depth limits, result summary, and final reload.

### Important backend limitation

The existing `/v1/library/relocate` endpoint does not physically move the library root. It scans a selected folder and repairs missing records by checksum. The UI therefore calls this operation `Find moved files` instead of presenting a misleading relocation workflow.

A complete physical library move with disk-space preflight, copy progress, cancellation, resumability, conflict handling, and final source cleanup still requires new backend APIs.

## Media

- Replaced metric cards with a compact status strip.
- Added Downloads and Archive modes.
- Added the shared list/details layout.
- Added detail tabs:
  - Overview
  - Items
  - Produced files
  - Activity
- Produced files are collected from media item output records in bounded batches and deduplicated by path.
- Added Open and Show in Explorer actions for produced files.
- Added a chronological media-item activity timeline with surfaced errors.
- Kept retry-one and retry-all-failed operations.
- Archive removal clearly states that downloaded files are not deleted.

## Torrents

- Replaced metric cards with a compact engine/activity summary.
- Main page now shows managed torrents only.
- Removed raw Engine and DHT views from the normal workspace.
- Added a separate Torrent engine details dialog for engine-only torrents and advanced DHT data.
- Main table now shows Name, Progress, Down, Up, ETA/Ratio, and State.
- Added detail tabs:
  - Overview
  - Files
  - Peers
  - Trackers
  - Advanced
- Added a nested torrent file tree with:
  - folder selection;
  - per-file selection;
  - file search;
  - selected file count;
  - selected byte total.
- Added tracker extraction from available engine raw data.
- Added explicit removal choices:
  - Remove and keep files
  - Remove and delete files
- Raw engine payloads are available only under Advanced.

## API typing

- Added `LibraryRelocationRequest` to the frontend API types.
- Updated `relocateLibrary` to expose backend scan limits without changing the endpoint contract.

## Tests and validation

- `svelte-check`: 0 errors, 0 warnings.
- Vitest: 82 tests passed across 14 files.
- Vite production build: completed successfully.
- Added unit tests for:
  - library duplicate grouping and sorting;
  - media progress and produced-file deduplication;
  - torrent progress, ETA, file-tree construction, and tracker extraction.

## Remaining planned work

The next redesign phase should focus on:

1. Automation visual rule builder and readable schedule editor.
2. Full Settings category navigation.
3. Moving cleanup policies into Settings → Storage and Library.
4. Moving Components into Settings → Tools.
5. Moving Diagnostics into Settings → Troubleshooting.
6. Specific secret editors and unsaved-change handling.
7. Accessibility, DPI, E2E, and visual-regression passes.
8. Backend work required for a true physical library-root relocation workflow.
