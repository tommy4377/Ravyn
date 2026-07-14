# Ravyn Frontend Redesign — Phase 3 Progress

## Completed in this checkpoint

### Downloads workspace

- Replaced the page-specific toolbar with the shared `PageCommandBar` structure.
- The normal command bar now exposes only Search, Filter, Sort, and More.
- Refresh and Batch queue access were moved into the overflow menu.
- Added a compact live summary for active, queued, aggregate download speed, and items requiring attention.
- Converted status navigation into compact segments with live counts.
- Added a shared filter flyout with an active-filter badge and clear action.
- Added sortable column headers and persisted sort key and direction in local storage.
- Reduced row decoration and removed the framed icon tile treatment.
- Added a single More action on row hover while preserving the context menu.
- Source host and destination are now presented as secondary row metadata.
- Selection mode now replaces the normal command bar instead of adding controls to every row.

### Download details

- Standardized the detail tabs as Overview, Files, Activity, and Advanced.
- Renamed Outputs to Files and made produced files the focus of the second tab.
- Removed Security as a primary tab.
- Source verification now uses Secure source, Verification recommended, or Source requires attention instead of a numeric trust score.
- Technical transfer state, segment data, checksum information, and raw options are inside Advanced disclosures.
- Reworked the overview into status, contextual actions, a compact transfer summary, and essential metadata.
- Replaced uppercase technical subheadings with the normal product typography hierarchy.

### Input and keyboard behavior

- Added drag-and-drop for links, magnets, torrent files, Metalink documents, and local files.
- Multiple dropped sources open the source-first batch flow automatically.
- Added `Ctrl+F` to focus Downloads search.
- Added `Ctrl+V` to open Add Download with clipboard content when focus is not inside an editor.
- Added Space to pause or resume the current selection.
- Existing Enter, Delete, arrows, Ctrl+A, Ctrl+N, Ctrl+, and Escape behavior remains available.

### Shared components

- Added `PageCommandBar.svelte`.
- Added `FilterFlyout.svelte`.
- Extended `SearchBox` with a stable input id for keyboard focus.
- Extended `Dropdown` with an optional id for accessible labels.
- Added shared plain-language trust presentation with unit tests.

## Verification

- `svelte-check`: 0 errors, 0 warnings.
- Vitest: 73 tests passed.
- Vite production build completed successfully.

## Next implementation targets

1. Split Metalink and structured JSON batch import into focused dialogs.
2. Add undo for non-destructive removal from the Downloads list.
3. Apply the shared command bar and list/details model to Library.
4. Implement the complete Library relocation workflow.
5. Remove metric-card dashboards from Media and Torrents.
6. Introduce the shared list/details layout for Media and Torrents.
7. Begin the Settings category split and move Tools and Troubleshooting into Settings.
