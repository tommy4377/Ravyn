# Ravyn Frontend Redesign — Phase 2 Progress

## Completed in this checkpoint

### Source-first Add Download flow

- The source field is now the first and dominant control.
- Ravyn automatically detects direct downloads, media pages, magnets/torrent files, Metalink documents, structured JSON batches, and multiple direct URLs.
- The detected type is shown with a compact explanation.
- Manual type selection remains available as a secondary override and can be reset to automatic detection.
- Media and torrent probes start automatically after a short debounce.
- The explicit Analyze action is now a Retry action.
- Multiple direct URLs are recognized as a simple batch without requiring JSON.
- Source trust copy no longer exposes a numeric score in the normal flow. It uses Secure source, Verification recommended, or Source requires attention.
- The dialog title and primary structure are consistent across source types.

### Shared component improvement

- Dropdown now supports an optional typed change callback while preserving bindable value behavior.

### Tests

- Added unit coverage for source detection.
- `svelte-check`: 0 errors, 0 warnings.
- Vitest: 72 tests passed.
- Vite production build completed successfully.

## Next implementation targets

1. Replace permanent Downloads filter and sort dropdowns with Filter and Sort flyouts.
2. Move Refresh into the Downloads overflow menu.
3. Standardize the details tabs as Overview, Files, Activity, and Advanced.
4. Add drag-and-drop for URLs, torrent files, Metalink files, and local paths.
5. Add an undo notification for non-destructive removal from the list.
6. Split Metalink import and structured batch import into focused dialogs.
7. Begin the shared list/details migration for Library, Media, and Torrents.
