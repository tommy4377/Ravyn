# Ravyn Frontend Redesign Progress — 2026-07-14

## Completed in this pass

- Reduced the main navigation to Downloads, Library, Media, Torrents, and Automation.
- Kept Settings in the navigation footer.
- Converted Basket into a transient Batch queue drawer.
- Added expanded, compact rail, and small-window overlay navigation behavior.
- Simplified the application shell so anchored content uses flat surfaces and dividers instead of floating cards.
- Unified the download details region with the main shell and preserved its persistent width.
- Added initial global keyboard commands:
  - `Ctrl+N` opens Add download.
  - `Ctrl+,` opens Settings.
  - `F5` refreshes Downloads.
  - `Escape` closes transient navigation and Batch queue layers.
- Added reusable frontend primitives:
  - `PageScaffold`
  - `CompactSummary`
  - `AdvancedDisclosure`
- Reduced decorative material intensity and removed card shadows from anchored content.
- Created a Figma reference screen for the target Downloads shell and detail layout.

## Validation

- `npm run check`: passed with 0 errors and 0 warnings.
- `npm test`: 10 test files and 67 tests passed.
- `npm run build`: production build completed successfully.

## Next implementation priorities

1. Migrate Downloads to `PageScaffold` and the shared command bar model.
2. Refactor Add Download into a source-first flow with automatic type detection.
3. Split Settings into categories and move Components and Diagnostics into Tools and Troubleshooting.
4. Apply the shared list/details layout to Library, Media, and Torrents.
5. Replace remaining metric-card grids with `CompactSummary`.
6. Add persistent table columns, sorting, density, and filter flyouts.
7. Complete keyboard navigation, focus restoration, and destructive-action consistency.
