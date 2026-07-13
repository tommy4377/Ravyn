# Screen Inventory

> Companion to `DESIGN_PLAN(2).md` §26.3. One entry per real screen. Screens without a real, connected implementation are not listed as "built" — see `BACKEND_CAPABILITY_MATRIX.md` for what's still planned.

## Main shell (`frontend/src/lib/shell/AppShell.svelte`)

- **Purpose:** Application chrome — navigation, command bar, content host, details pane, connection/notification plumbing. Boots the main window's backend connection.
- **Entry points:** Mounted by `App.svelte` when the Tauri window label is `"main"`.
- **Routes/commands:** Tauri `backend_info`, `main_window_ready`; `GET /v1/setup` (via `connection.svelte.ts`); `GET /v1/events` (SSE).
- **Events:** Subscribes once to the event stream for the app's lifetime; forwards to `jobsStore.applyEvent`.
- **Loading:** `ConnectionBoot.svelte` (indeterminate progress) while `connection.status === "connecting"`.
- **Empty state:** N/A (chrome, not a data screen).
- **Errors:** `ConnectionBoot.svelte` shows `InlineError` with retry when the backend is unreachable; the main window is still shown (per the deterministic setup→main handoff) so the user never sees a blank window.
- **Recovery:** Retry re-runs `connection.connect()`.
- **Destructive actions:** None directly.
- **Keyboard:** Standard Tab order through nav → command bar → list → details pane.
- **Accessibility:** Light/Dark/system theme and Comfortable/Compact density are togglable from the nav-pane footer (no Settings screen exists yet to host them properly — tracked as a gap, not hidden).
- **Tests:** None at the shell-composition level yet (each piece it composes has its own unit tests). A component-level smoke test is a good addition once a `svelte-testing-library` harness exists in this project (none does today, and the plan says vertical slices should add tests progressively, not introduce a new test framework as a side effect of one screen).

## Downloads (`frontend/src/lib/downloads/DownloadsView.svelte`)

- **Purpose:** The primary product surface — list, filter, search, sort, add, and act on downloads. The first complete vertical slice per the design plan's Phase 2.
- **Entry points:** Only nav section currently registered in `NavigationView.svelte`.
- **Routes/commands:** `GET /v1/jobs`, `POST /v1/jobs`, `POST /v1/jobs/import-text`, `POST /v1/jobs/actions`, `DELETE` (via bulk action), pause/resume/cancel/retry.
- **Events:** `job_status`, `progress` (coalesced ~10 Hz), `queue_changed`, `resync_required` — all via the shared `jobsStore`.
- **Loading:** Skeleton rows while the first page loads (`jobsStore.status === "loading" && !hasLoadedOnce`).
- **Empty state:** `EmptyState` with distinct copy for "no downloads yet" vs. "no results for this search", plus an "Add download" call to action in the former case.
- **Errors:** `InlineError` with retry on list-load failure; per-item bulk-action failures surface as a toast naming the failure count; remove failures surface inside the confirm dialog itself.
- **Recovery:** Retry button on list error; "Refresh" in the overflow menu; automatic refetch on `resync_required`.
- **Destructive actions:** "Remove from list" (bulk and per-row) — explicitly labeled, with a confirm dialog stating that downloaded files are not deleted (verified against `src/core/lifecycle.rs::delete`, which cancels + deletes the job row but does not remove already-downloaded output files for HTTP/media jobs).
- **Keyboard:** Arrow Up/Down move the roving selection (with scroll-into-view), Shift+Arrow extends a range, Shift/Ctrl+click for range/toggle selection, Ctrl+A selects all loaded+visible items, Enter opens details, Delete opens the remove-confirmation flow. The list is a real `role="listbox"` composite widget (`tabindex="0"`, `aria-activedescendant`), not a click-only surface.
- **Accessibility:** Status pills carry both an icon and text (not color-only); progress bars are real `role="progressbar"` elements; Light/Dark/High-Contrast handled via the existing token system (`forced-colors` media query already present in `tokens.css`).
- **Tests:** `jobPresentation.test.ts` (status→action mapping against the exact backend transition guards), `jobActions.test.ts` (destructive-action labeling), `jobs.test.ts` (store normalization, event application, coalescing, view filters), `selection.test.ts`.

## Download details pane (`frontend/src/lib/downloads/JobDetailsPane.svelte`)

- **Purpose:** Optional, resizable-in-spirit (currently fixed-width; true resize is a follow-up) side pane for a selected download — Overview / Outputs / Activity / Advanced tabs.
- **Entry points:** Selecting a row (click, Enter, or double-click) in Downloads.
- **Routes/commands:** `GET /v1/jobs/{id}/outputs`, `/actions`, `/logs`, `/segments` — each fetched lazily, only when its tab is first opened.
- **Events:** Reads live data from the shared `jobsStore` (no separate subscription).
- **Loading:** `Skeleton` while the job record itself isn't yet in the store; per-tab `Skeleton`/`InlineError` for each lazily-fetched tab.
- **Empty state:** "No output files yet.", "No log entries." per tab.
- **Errors:** `InlineError` per tab on fetch failure (no blanket pane-level error).
- **Recovery:** N/A beyond re-opening the tab (no explicit retry button yet — a small gap; the tab-switch itself re-triggers the fetch only if the cache is empty, so switching away and back does not naturally retry a failed fetch without a page reload. Tracked as a follow-up.)
- **Destructive actions:** None (read-only pane).
- **Keyboard:** `Tabs.svelte` supports Arrow Left/Right/Home/End between tabs; Escape is not wired to close the pane (only the explicit close button) — intentional, since Escape is reserved for dialogs.
- **Accessibility:** Tabs use real `role="tablist"/"tab"` semantics with `aria-selected` and roving `tabindex`.
- **Tests:** Covered indirectly through `jobPresentation.test.ts` (status presentation shared with the Overview tab); no dedicated component test.

## Add Download dialog (`frontend/src/lib/downloads/AddDownloadDialog.svelte`)

- **Purpose:** Create one or many downloads from pasted/typed URLs, with destination, checksum, duplicate policy, tags, and network overrides behind a progressive-disclosure "Advanced options" section.
- **Entry points:** Downloads command bar ("Add", "Paste"), and the Downloads empty state's "Add download" button.
- **Routes/commands:** `POST /v1/jobs` (single line) or `POST /v1/jobs/import-text` (multiple lines), chosen automatically by `JobsService.addFromInput`.
- **Events:** None directly; created jobs are pushed into `jobsStore` immediately via `upsert` so they appear before the next SSE event arrives.
- **Loading:** Submit button shows "Adding…" and disables while in flight.
- **Empty state:** N/A.
- **Errors:** `InlineError` inside the dialog on failure; partial success (some lines rejected) is reported as a toast rather than blocking the dialog.
- **Recovery:** The dialog stays open with the entered text intact on failure, so the user can fix and resubmit without retyping.
- **Destructive actions:** None (creation only).
- **Keyboard:** Standard Dialog focus trap; Escape cancels (disabled while submitting); Enter in a text field does not accidentally submit (only the explicit button does).
- **Accessibility:** Every field has a real `<label>`; the destination field's Browse button uses the already-capability-granted Tauri dialog plugin (`pickFolder`), not a new command.
- **Tests:** Not yet covered by an automated test (would need a mocked `RavynClient`/`fetch`); see `API_UI_MAP.md` "Known gaps".
