# Ravyn Frontend Redesign — Phase 5 Progress

Date: 2026-07-14
Scope: Automation and Settings architecture

## Completed in this checkpoint

### Automation

- Replaced the monolithic Automation screen with focused components and a reactive page controller.
- Added separate Rules and Schedules workspaces.
- Added a visual rule editor with clear `When` and `Then` blocks.
- Limited visual rule conditions and actions to contracts already supported by the backend.
- Added readable schedule modes for one-time, interval, daily, weekly, and advanced cron execution.
- Kept raw cron expressions inside the advanced mode.
- Added a structured execution history with a shared list/details layout.
- Added before/after rule previews instead of exposing only raw backend data.
- Added presentation helpers and unit tests for rule and schedule conversion.

### Settings

- Replaced the single long Settings page with ten focused categories:
  - General
  - Downloads
  - Storage and Library
  - Appearance
  - Tools
  - Network
  - Updates
  - Privacy and Secrets
  - Troubleshooting
  - About
- Added a dedicated `SettingsController` for backend settings, validation, dirty state, profiles, presets, tags, secrets, cleanup, updates, and installation information.
- Appearance changes remain immediate and device-local.
- Backend settings use a sticky `Unsaved changes` bar with explicit Save and Discard actions.
- Changing categories with unsaved backend changes now requires confirmation.
- Leaving Settings through the main navigation or an Add shortcut also requires confirmation.
- Added a global settings navigation guard with unit tests.
- Moved component provisioning into `Settings → Tools`.
- Moved diagnostics into `Settings → Troubleshooting`.
- Removed Components and Diagnostics as top-level application routes.
- Added simple updater states and kept the technical result inside an advanced disclosure.
- Added typed secret editors for credentials and write-only secret values.
- Moved Library cleanup policies into `Settings → Storage and Library`.
- Added an About page with install mode, version, executable, data directory, backend address, and copyable system information.

### Structure

Major new or rewritten files include:

- `automation/automationController.svelte.ts`
- `automation/automationPresentation.ts`
- `automation/RulesList.svelte`
- `automation/RuleEditor.svelte`
- `automation/RulePreview.svelte`
- `automation/SchedulesList.svelte`
- `automation/ScheduleEditor.svelte`
- `automation/ExecutionHistory.svelte`
- `settings/settingsController.svelte.ts`
- `settings/SettingsNavigation.svelte`
- `settings/SettingsDialogs.svelte`
- individual Settings category components

## Verification

- `svelte-check`: 0 errors, 0 warnings
- Vitest: 89 tests passed across 16 test files
- Vite production build: completed successfully
- No backend API contracts were changed
- No React, Tailwind, or second icon library was introduced

## Remaining frontend work

The next checkpoint should focus on final product refinement:

1. Break down the remaining large Downloads, Library, Media, Torrents, Components, and Diagnostics implementations further.
2. Complete keyboard parity and focus restoration across every menu, dialog, drawer, and list.
3. Add a command palette only if it remains useful after shortcut coverage is complete.
4. Add component tests for Settings forms, Automation editors, destructive confirmations, and details-pane behavior.
5. Add Playwright flows for setup, direct download, media, torrent selection, Library trash/restore, Automation, provisioning, and updater states.
6. Add visual regression coverage for light, dark, compact, comfortable, High Contrast, Reduced Motion, and high-DPI layouts.
7. Verify the native WebView2 application on Windows at 100%, 125%, 150%, 175%, and 200% scaling.
8. Perform a final copy review and localization-readiness pass.
