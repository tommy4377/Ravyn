# API-to-UI Map

> Companion to `DESIGN_PLAN(2).md` §26.2. Traces each connected route/event to its frontend service, store, screen, and tests. Update with every completed vertical slice. Only *connected* work is listed in detail here; see `BACKEND_CAPABILITY_MATRIX.md` for the full backend surface including what's still planned.

## Setup (pre-existing, retained from the setup milestone)

```
GET /v1/setup, POST /v1/setup/library, POST /v1/setup/complete,
GET /v1/components, POST /v1/components/features,
POST /v1/components/{id}/install, POST /v1/components/{id}/cancel
→ frontend/src/lib/api/client.ts (RavynClient — Setup/Components sections)
→ frontend/src/lib/setup/controller.svelte.ts (SetupController)
→ frontend/src/lib/setup/SetupApp.svelte + stages/*.svelte
→ events: `component`, `resync_required` (via RavynEventClient)
→ tests: frontend/src/lib/setup/componentStates.test.ts
```

## Downloads (this milestone)

```
GET /v1/jobs
→ frontend/src/lib/api/client.ts: RavynClient.listJobs
→ frontend/src/lib/services/jobs.ts: JobsService.list
→ frontend/src/lib/stores/jobs.svelte.ts: JobsStore.loadInitial / loadMore
→ frontend/src/lib/downloads/DownloadsView.svelte (list, filters, search, view tabs)
→ events: job_status, progress, queue_changed, resync_required
→ tests: frontend/src/lib/stores/jobs.test.ts

POST /v1/jobs, POST /v1/jobs/import-text
→ RavynClient.createJob / importJobsText
→ JobsService.addFromInput (chooses single vs. batch by line count)
→ frontend/src/lib/downloads/AddDownloadDialog.svelte
→ tests: (covered indirectly via jobActions.test.ts patterns; addFromInput itself
  is integration-shaped and not yet unit-tested — see "Known gaps" below)

GET/PATCH/DELETE /v1/jobs/{id}
→ RavynClient.getJob / updateJob / deleteJob
→ JobsService.get / update / remove
→ frontend/src/lib/downloads/JobDetailsPane.svelte (read); DownloadsView remove flow (delete)
→ tests: frontend/src/lib/stores/jobs.test.ts (upsert/removeLocal)

POST /v1/jobs/{id}/pause|resume|cancel|retry, POST /v1/jobs/actions (bulk)
→ RavynClient.pauseJob/resumeJob/cancelJob/retryJob/applyJobAction
→ JobsService.pause/resume/cancel/retry/bulkAction
→ frontend/src/lib/downloads/jobPresentation.ts (permittedActions — mirrors
  src/core/lifecycle.rs transition guards exactly)
→ frontend/src/lib/downloads/jobActions.ts (buildJobMenuItems — row/context menu)
→ frontend/src/lib/downloads/DownloadsView.svelte (command-bar bulk actions)
→ frontend/src/lib/downloads/JobRow.svelte (per-row context menu)
→ tests: frontend/src/lib/downloads/jobPresentation.test.ts,
  frontend/src/lib/downloads/jobActions.test.ts

GET /v1/jobs/{id}/outputs|/actions|/logs|/segments
→ RavynClient.listJobOutputs/listJobActions/listJobLogs/listJobSegments
→ JobsService.outputs/actions/logs/segments
→ frontend/src/lib/downloads/JobDetailsPane.svelte (Outputs/Activity/Advanced tabs,
  fetched lazily per tab)
→ tests: none yet (network-shaped; would need a fetch/client mock — gap below)

GET /v1/events (SSE: job_status, progress, component, queue_changed, resync_required)
→ frontend/src/lib/api/events.svelte.ts: RavynEventClient
→ frontend/src/lib/stores/connection.svelte.ts: connection.events
→ frontend/src/lib/stores/jobs.svelte.ts: JobsStore.applyEvent
  (progress events are coalesced to one shared ~10 Hz flush timer, not per-entity)
→ frontend/src/lib/shell/AppShell.svelte (subscribes once, for the app's lifetime)
→ tests: frontend/src/lib/stores/jobs.test.ts (job_status patch, unknown-job refetch,
  progress coalescing via fake timers, queue_changed/resync_required → refreshAll)
```

## Shell infrastructure (not route-backed)

```
frontend/src/lib/stores/connection.svelte.ts — backend_info (Tauri) + GET /v1/setup
  → drives the connecting/ready/error boot sequence for the main window
frontend/src/lib/stores/navigation.svelte.ts — client-only chrome state
  (active section, Downloads view tab, details-pane selection, density, theme)
  persisted to localStorage (ravyn.density, ravyn.theme)
frontend/src/lib/stores/notifications.svelte.ts — client-only toast queue
frontend/src/lib/stores/selection.svelte.ts — generic multi-selection with
  keyboard range support, used by DownloadsView
  → tests: frontend/src/lib/stores/selection.test.ts
frontend/src/lib/shell/AppShell.svelte, NavigationView.svelte, CommandBar.svelte,
  StatusBar.svelte, NotificationHost.svelte, ConnectionBoot.svelte
  → frontend/src/App.svelte (main-window branch)
```

## Known gaps (tracked, not silently skipped)

- `JobsService.addFromInput` and the `JobDetailsPane` lazy tab fetches are not unit-tested; they're thin, mostly network-shaped compositions over already-tested primitives (`RavynClient`, `describeError`). A future pass should add a mocked-`fetch` integration test per §23.4 of the design plan.
- `Tags` are currently read-only in the Downloads slice (`job.options_json.tags`, shown in the details Overview tab). The dedicated `/v1/tags*` routes are not wired — that's Phase 5 (Automation) work.
- No "open containing folder" action exists anywhere: Ravyn's Tauri command surface has exactly 5 setup-only commands (see `src-tauri/src/lib.rs`), none of which reveal a path in Explorer. Adding one is release/desktop-shell work, not a frontend-only change, and is out of scope for this pass. Output paths are shown as copyable text instead (`JobDetailsPane` → Outputs tab).
