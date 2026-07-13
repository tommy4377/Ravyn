# Ravyn — Frontend Design and Implementation Plan

> **Status:** Authoritative frontend plan derived from the current Ravyn codebase.
>
> **Project snapshot reviewed:** July 13, 2026.
>
> **Primary decision:** The main frontend may begin now. The backend application core is sufficiently broad and mature for frontend integration, but the desktop distribution, application updater, remote component-manifest pipeline, portable mode, privileged Tauri command restrictions, and Windows end-to-end release validation are not complete.

---

# 1. Purpose

This document defines how the Ravyn frontend must be designed and implemented against the backend that exists today.

It replaces plans that treated the frontend as a speculative interface. Ravyn already exposes a large operational backend including:

- HTTP and segmented downloads;
- media probing and media-item workflows;
- torrent management through rqbit;
- library indexing, import, verification, relocation, Trash, and duplicate discovery;
- basket, presets, profiles, rules, tags, and schedules;
- component provisioning and setup persistence;
- settings, secrets, host profiles, audit records, metrics, backup, restore, and maintenance;
- browser-token and page-resource workflows;
- Server-Sent Events for live updates;
- a Tauri setup shell and Windows integration layer.

The frontend must expose those capabilities as coherent user workflows rather than mirroring backend tables or routes one-for-one.

---

# 2. Backend readiness decision

## 2.1 Decision

The backend is **ready enough to begin the complete frontend**, with defined exceptions.

Frontend development should start immediately on:

1. the application shell;
2. the Downloads list and job actions;
3. Add Download;
4. job details and logs;
5. Library;
6. media probing and selection;
7. torrent probing and file selection;
8. Basket and presets;
9. Rules, tags, and schedules;
10. Components, Settings, and Diagnostics.

These areas already have concrete storage models, routes, validation, and operational services.

## 2.2 What “ready” means

For this project, backend readiness does not mean that every release-engineering feature is finished.

It means:

- the primary domain models exist;
- the main user operations exist;
- persistence exists;
- stable route families exist;
- live events exist;
- error responses are structured;
- migrations cover the current product model;
- the core has meaningful automated test coverage;
- frontend work can proceed without inventing fake production behavior.

## 2.3 Remaining backend and desktop blockers

The following work is still required before Ravyn can be called a reliable public desktop beta.

### Release-critical

- Build and publish the Tauri desktop application instead of only the CLI backend.
- Build the frontend in CI before packaging.
- Produce the real Windows desktop artifact and custom setup artifact.
- Add binary signing and release smoke tests.
- Test install, relaunch, update, rollback, and uninstall on clean Windows machines.

### Application update and repair

- Consume remote release metadata.
- Verify signed update metadata.
- Download and transactionally replace the Ravyn desktop executable.
- Verify readiness after restart.
- Roll back a failed application update.
- Repair missing or corrupted application files.

### Managed component delivery

- Add the missing verified 7-Zip artifact or revise the extraction strategy.
- Embed the production manifest public key.
- Implement a signed remote manifest provider.
- Add ETag or Last-Modified caching.
- Add expiration and replay/downgrade protection.
- Preserve and fall back to the last verified manifest.
- Make stable/beta component channels operational rather than nominal.

### Desktop security

- Restrict privileged Tauri commands by calling window.
- Restrict commands by setup state and installation mode.
- Require persisted user consent where appropriate.
- Prevent repeated or conflicting execution.
- Ensure the main window cannot freely invoke setup-only commands.

### Portable mode

- Expose an explicit portable-mode choice.
- Define portable data-directory behavior.
- Prevent unintended migration to LocalAppData.
- Support updates without breaking portability.

### Provisioning hardening

- Add proactive free-space checks.
- Add best-effort antivirus or quarantine diagnostics.
- Convert desktop installation and custom-path failures to the same structured provisioning error model.
- Add real HTTPS provisioning integration tests with trusted test certificates.

These items must be tracked as parallel release work. They must not block implementation of frontend workflows whose backend contracts already exist.

---

# 3. Product direction

Ravyn is a Windows-first download manager and local download library.

The experience should combine:

```text
Windows 11 interaction conventions
+ File Explorer information density
+ Fluent visual language
+ powerful download and automation workflows
+ progressive disclosure
+ real-time backend state
```

Ravyn must not resemble:

- a generic browser dashboard;
- a mobile layout stretched across a desktop window;
- a card-heavy SaaS admin panel;
- a Material Design application;
- a decorative prototype using fake data;
- a technical backend console presented directly to normal users.

The application should feel efficient with one download and remain usable with thousands of records.

---

# 4. Non-negotiable implementation rules

## 4.1 Build vertical slices

Every production feature must be implemented in this order:

```text
inspect the real route and model
→ define the user workflow
→ implement typed client methods
→ implement normalized state
→ implement the visible interface
→ subscribe to relevant events
→ handle loading, empty, error, and recovery states
→ add tests
→ update the API-to-UI map
```

A screen is not complete if it only looks finished.

## 4.2 No production mock data

Mock data is allowed only in:

- unit tests;
- isolated component stories;
- screenshot fixtures;
- experiments outside production routes.

Production screens must use the embedded backend.

## 4.3 Do not mirror the API literally

The backend defines capabilities, validation, and truth.

The frontend defines user-centered workflows.

Not every route requires a page. Not every field belongs in the default view. Diagnostic data must remain available without overwhelming primary workflows.

## 4.4 Keep backend contracts centralized

Do not call `fetch` or Tauri `invoke` directly from arbitrary Svelte components.

Use dedicated services for:

- API transport;
- events;
- jobs;
- library;
- media;
- torrents;
- automation;
- components;
- setup;
- settings;
- diagnostics;
- Windows integration.

## 4.5 Treat destructive operations explicitly

Never use one ambiguous “Delete” action.

Use labels such as:

- Remove from list;
- Move file to Trash;
- Delete file permanently;
- Remove torrent and keep data;
- Remove torrent and delete data;
- Forget library entry;
- Clear history.

---

# 5. Current frontend baseline

The current frontend already includes:

- Svelte 5;
- TypeScript;
- Vite;
- Tauri client integration;
- foundational design tokens;
- basic controls;
- a backend client;
- an SSE event client;
- a connected setup flow;
- real setup and component provisioning state;
- a deterministic main-window readiness handoff.

The current main application is intentionally minimal. It only validates backend connectivity and displays setup information.

Therefore, the next milestone is not more setup prototyping. It is the real main shell and the first complete download-management slice.

---

# 6. Information architecture

## 6.1 Primary navigation

Use this initial navigation:

```text
Downloads
Library
Basket
Automation
Components
Settings
Diagnostics
```

`Automation` contains:

```text
Schedules
Rules
Presets
Profiles
Tags
Browser capture
```

This avoids overcrowding the top-level navigation while retaining all backend capabilities.

## 6.2 Downloads views

```text
Downloads
├── Active
├── Queued
├── Completed
├── Failed
├── Scheduled
└── All
```

These are filtered views over the same normalized job collection, not separate data silos.

## 6.3 Library views

```text
Library
├── All files
├── Videos
├── Music
├── Documents
├── Images
├── Archives
├── Torrents
├── Playlists
└── Trash
```

Categories should be derived from backend library metadata and category services.

## 6.4 Diagnostics hierarchy

```text
Diagnostics
├── Overview
├── Backend
├── Database
├── Components
├── Network and hosts
├── Events
├── Logs
├── Audit
├── Backups
└── Maintenance
```

Diagnostics are for investigation and support. They must not dominate the normal application experience.

---

# 7. Main window structure

Use a desktop-oriented layout:

```text
Native title bar
Compact command bar
Left navigation pane
Main content region
Optional resizable details pane
Context-sensitive status bar
```

## 7.1 Title bar

Required behavior:

- native minimize, maximize/restore, and close controls;
- Snap Layouts;
- `Alt+Space`;
- double-click maximize/restore;
- correct drag region;
- native resize borders;
- correct taskbar preview;
- correct behavior across DPI changes.

## 7.2 Command bar

The command bar changes by view and selection.

Downloads example:

```text
Add
Paste
Start
Pause
Resume
Retry
Cancel
Open
Delete
Sort
Filter
View
More
```

Library example:

```text
Import
Open
Open folder
Verify
Relocate
Move to Trash
Restore
Sort
Filter
View
More
```

## 7.3 Details pane

The details pane must be optional and resizable.

For a selected download it may show:

- status and progress;
- source;
- destination;
- current speed and ETA;
- outputs;
- checksum or integrity state;
- media or torrent summary;
- tags;
- schedule;
- recent actions;
- recent error;
- quick actions.

Deep logs and segment diagnostics belong in a dedicated details screen or expandable advanced section.

---

# 8. Visual system

## 8.1 Design language

Use Fluent Design principles with Windows 11 density and behavior.

Structural references:

- modern File Explorer;
- Windows Settings;
- Microsoft Store download management;
- Windows security and system dialogs.

Do not clone those applications visually. Reuse their interaction grammar.

## 8.2 Typography

```css
font-family:
  "Segoe UI Variable",
  "Segoe UI",
  system-ui,
  sans-serif;
```

Define a shared type ramp. Do not assign arbitrary sizes per component.

## 8.3 Spacing

Use a 4-pixel scale:

| Token | Value |
|---|---:|
| `space-1` | 4px |
| `space-2` | 8px |
| `space-3` | 12px |
| `space-4` | 16px |
| `space-5` | 20px |
| `space-6` | 24px |
| `space-8` | 32px |
| `space-10` | 40px |
| `space-12` | 48px |

## 8.4 Shape

| Token | Value | Usage |
|---|---:|---|
| `radius-small` | 2px | tiny indicators |
| `radius-control` | 4px | inputs and checks |
| `radius-medium` | 6px | buttons and rows |
| `radius-layer` | 8px | flyouts and dialogs |
| `radius-large` | 12px | major surfaces only |
| `radius-pill` | 999px | compact filters only |

Avoid giving every surface a large rounded-card appearance.

## 8.5 Density

Support:

- Comfortable;
- Compact.

Density affects row height, padding, command spacing, metadata visibility, and form layout.

The Downloads and Library views must be genuinely useful in Compact mode.

## 8.6 Materials

Default behavior:

```text
Windows 11 with transparency enabled → Mica
Windows 10 or unsupported composition → solid Fluent fallback
High Contrast → solid
Remote Desktop or degraded composition → solid
```

Materials must never reduce readability.

## 8.7 Icons

Use:

- Fluent UI System Icons for application actions;
- Windows shell icons for files, folders, drives, and known folders.

Do not use emoji, Material icons, or mixed libraries.

---

# 9. Frontend architecture

## 9.1 Directory structure

Recommended structure:

```text
frontend/src/
├── lib/
│   ├── api/
│   │   ├── client.ts
│   │   ├── transport.ts
│   │   ├── errors.ts
│   │   ├── events.ts
│   │   └── generated-or-shared-types.ts
│   ├── services/
│   │   ├── jobs.ts
│   │   ├── library.ts
│   │   ├── media.ts
│   │   ├── torrents.ts
│   │   ├── automation.ts
│   │   ├── components.ts
│   │   ├── settings.ts
│   │   └── diagnostics.ts
│   ├── stores/
│   │   ├── connection.svelte.ts
│   │   ├── jobs.svelte.ts
│   │   ├── library.svelte.ts
│   │   ├── selection.svelte.ts
│   │   ├── components.svelte.ts
│   │   ├── settings.svelte.ts
│   │   └── notifications.svelte.ts
│   ├── components/
│   ├── shell/
│   ├── downloads/
│   ├── library/
│   ├── media/
│   ├── torrents/
│   ├── automation/
│   ├── components-page/
│   ├── settings/
│   ├── diagnostics/
│   ├── setup/
│   └── native/
├── styles/
├── App.svelte
└── main.ts
```

## 9.2 API client

The client must support:

- the embedded API token;
- request identifiers;
- typed request and response models;
- structured API errors;
- abort signals;
- pagination;
- bounded timeouts;
- safe retries only for idempotent operations;
- consistent JSON validation at the transport boundary.

Where practical, generate types from `/openapi.json` or maintain a checked shared contract. Do not duplicate route shapes across components.

## 9.3 Event client

The backend event stream supports sequence IDs, replay, and resynchronization.

Use this pipeline:

```text
SSE connection
→ parse and validate
→ track sequence
→ coalesce high-frequency entity updates
→ update normalized stores
→ rerender affected visible rows only
```

Rules:

- use `Last-Event-ID` when reconnecting;
- refetch authoritative collections on `resync_required`;
- cap visible progress rendering to approximately 10 Hz per entity;
- do not announce every byte-level update to assistive technology;
- expose connection state globally.

## 9.4 Stores

Use normalized stores keyed by entity ID.

Required stores:

- connection;
- jobs;
- selected jobs;
- job details cache;
- library entries;
- selected library entries;
- basket;
- media probes;
- torrents;
- schedules;
- rules;
- presets;
- profiles;
- tags;
- components;
- settings;
- diagnostics;
- notifications.

Avoid one monolithic global object that forces broad rerenders.

## 9.5 Native services

Keep Windows-specific behavior behind dedicated adapters:

- window state;
- shell icons;
- file and folder pickers;
- theme and accent;
- materials;
- taskbar progress;
- tray;
- notifications;
- startup integration;
- shortcuts;
- installation state;
- update state;
- uninstall entry point.

---

# 10. Downloads

Downloads are the primary product surface.

## 10.1 Backend contracts

Primary route families:

- `GET /v1/jobs`;
- `POST /v1/jobs`;
- `POST /v1/jobs/batch`;
- `POST /v1/jobs/metalink`;
- `GET/PATCH/DELETE /v1/jobs/{id}`;
- `POST /v1/jobs/{id}/pause`;
- `POST /v1/jobs/{id}/resume`;
- `POST /v1/jobs/{id}/cancel`;
- `POST /v1/jobs/{id}/retry`;
- `GET /v1/jobs/{id}/outputs`;
- `GET /v1/jobs/{id}/segments`;
- `GET /v1/jobs/{id}/actions`;
- `GET /v1/jobs/{id}/logs`;
- `POST /v1/jobs/actions` for bulk operations.

## 10.2 Downloads list

The list must support:

- virtualized rows;
- sorting;
- filtering;
- search;
- pagination or incremental loading;
- multi-selection;
- keyboard range selection;
- bulk actions;
- context menus;
- compact and comfortable density;
- configurable columns;
- saved view preferences;
- live progress updates;
- inline recoverable errors.

Recommended default columns:

```text
Name
Status
Progress
Speed
ETA
Size
Destination
Added
```

Secondary columns:

```text
Source host
Mode
Priority
Schedule
Tags
Created
Completed
Error code
```

## 10.3 Job states

Map backend states to stable UI states:

```text
Queued
Deferred
Probing
Downloading
Paused
Verifying
Post-processing
Completed
Partially completed
Failed
Cancelled
Seeding
```

Each state requires:

- text;
- icon;
- accessible description;
- permitted actions;
- visual severity;
- empty and transitional behavior.

## 10.4 Add Download

Primary flow:

```text
Paste or enter URL
→ classify the source
→ probe when needed
→ apply rules or preset
→ show only relevant options
→ confirm
→ create the real backend job
```

Supported input modes:

- direct HTTP/HTTPS URL;
- multiple URLs;
- Metalink;
- media URL;
- playlist or channel;
- magnet link;
- torrent file;
- imported text.

Advanced options:

- destination;
- filename template;
- checksum;
- headers;
- proxy or host profile;
- schedule;
- tags;
- priority;
- bandwidth policy;
- post-processing;
- duplicate policy.

## 10.5 Job details

Use sections or tabs:

```text
Overview
Files and outputs
Activity
Integrity
Media or torrent
Advanced diagnostics
```

Overview must remain understandable without knowledge of internal segment or adapter models.

---

# 11. Media workflows

## 11.1 Backend contracts

- `POST /v1/media/probe`;
- `GET /v1/jobs/{id}/media-items`;
- `GET /v1/jobs/{id}/media-summary`;
- `GET /v1/jobs/{id}/media-items/{item_id}/outputs`;
- retry routes for failed media items;
- media archive listing and removal.

## 11.2 Probe and selection UI

Show real probe results:

- title;
- uploader or channel;
- duration;
- thumbnail when available;
- playlist item count;
- available video formats;
- available audio formats;
- subtitles;
- estimated sizes;
- merge requirement;
- output container;
- metadata and thumbnail options.

Use presets for common selections:

```text
Best quality
1080p
720p
Audio only
Small file
Custom
```

Do not expose raw format IDs as the default interaction.

## 11.3 Component gating

When FFmpeg or yt-dlp is unavailable, explain the exact limitation.

Offer:

- install or enable the required component;
- choose an alternative that does not require it;
- cancel.

Never silently enable a feature the user disabled.

---

# 12. Torrent workflows

## 12.1 Backend contracts

- `POST /v1/torrents/probe`;
- `GET /v1/torrents`;
- engine status and DHT routes;
- torrent details, stats, peers, files, seeding, and removal routes.

## 12.2 Torrent probe

Show:

- torrent name;
- total size;
- source type;
- file tree;
- per-file selection;
- priority;
- destination;
- duplicate conflicts;
- seed policy;
- rqbit availability.

Virtualize large file trees.

## 12.3 Torrent details

Default view:

- progress;
- downloaded/uploaded;
- current rates;
- peers;
- seed state;
- selected files;
- destination;
- errors.

Advanced diagnostics:

- DHT state;
- engine identifiers;
- peer details;
- raw engine statistics.

---

# 13. Library

## 13.1 Backend contracts

- list and retrieve library entries;
- duplicate discovery;
- template preview;
- delete and restore;
- import and import status;
- verify;
- relocate;
- cleanup policies and cleanup execution;
- personal statistics.

## 13.2 Library experience

The Library is a persistent local catalog, not a renamed job history.

Support:

- virtualized list and optional thumbnail view;
- search;
- category filters;
- tags;
- source host;
- date and size filters;
- existing/missing status;
- duplicate relationships;
- moved-file repair;
- verification state;
- open file;
- open containing folder;
- move to Trash;
- restore;
- permanent deletion through an explicit flow.

Use native Windows shell icons for file-system objects.

## 13.3 Import and relocation

Long-running import, verification, relocation, and cleanup operations need:

- visible operation state;
- progress when available;
- cancellation when supported;
- partial-result reporting;
- clear conflict handling;
- a final summary;
- recoverable errors.

---

# 14. Basket, presets, profiles, rules, tags, and schedules

## 14.1 Basket

The Basket is a staging area before jobs are created.

Support:

- add;
- edit;
- reorder;
- validate;
- assign destination;
- assign preset/profile;
- apply tags;
- review duplicates;
- start selected;
- start all;
- clear.

## 14.2 Presets

Presets store reusable job options.

The UI should expose user-facing groups rather than raw JSON:

```text
Destination
Naming
Quality
Network
Scheduling
Post-processing
Tags
```

## 14.3 Profiles

Profiles represent broader operating configurations.

Show:

- active profile;
- purpose;
- included preferences;
- activation impact;
- validation errors;
- create, edit, duplicate, activate, and delete actions.

## 14.4 Rules

Use a visual builder:

```text
WHEN [condition]
AND [condition]
THEN [action]
AND [action]
```

Support:

- priority;
- enable/disable;
- preview against a URL or existing item;
- conflict explanation;
- validation;
- advanced raw representation only in an expandable section.

## 14.5 Tags

Tags should be lightweight and reusable across jobs and library views.

Support:

- assignment from lists and detail views;
- bulk replacement;
- filtering;
- deletion with impact explanation.

## 14.6 Schedules

The scheduler UI must support:

- one-time schedules;
- recurring schedules;
- timezone selection;
- enable/disable;
- next run;
- run now;
- execution history;
- cancellation of active executions;
- missed-run behavior;
- conflict and validation messages.

Do not lead with raw cron syntax.

---

# 15. Components

## 15.1 Feature model

| User-facing feature | Managed component |
|---|---|
| Standard downloads | Ravyn core |
| Video and playlists | yt-dlp |
| Media processing | FFmpeg |
| Torrent downloads | rqbit |
| Archive extraction | 7-Zip |

## 15.2 Component states

The UI must distinguish:

```text
Not installed
Queued
Downloading
Verifying
Installing
Installed
Update available
Failed
Unsupported
Cancelled
Custom path
Custom path invalid
Rollback available
```

## 15.3 Components page

Each row shows:

- feature name;
- engine name as secondary information;
- enabled preference;
- installation state;
- installed version;
- available version;
- source: managed or custom;
- integrity/health state;
- last verification;
- disk usage when available;
- current progress;
- last error.

Actions:

- Enable;
- Disable;
- Install;
- Update;
- Verify;
- Retry;
- Cancel;
- Roll back;
- Remove;
- Cleanup old versions;
- Choose custom executable;
- Restore managed version;
- Open logs.

## 15.4 Known current limitation

The production UI must honestly show 7-Zip as unsupported until the backend ships a verified artifact or changes its extraction approach.

Do not hide the state and do not fake successful provisioning.

---

# 16. Settings

Suggested categories:

```text
General
Appearance
Downloads
Library
Network
Components
Automation
Notifications
Privacy and Security
Updates
Advanced
```

Use:

- `GET /v1/settings`;
- `PATCH /v1/settings`;
- validation and reset routes;
- host-profile routes;
- secret-reference routes where appropriate.

Never expose secret values after storage. Display references, labels, scope, and replacement/removal actions.

Changes that require restart must be clearly marked.

---

# 17. Diagnostics and maintenance

## 17.1 Overview

Show a concise health summary:

- backend connected;
- database healthy;
- migration state;
- event stream connected;
- component health;
- storage access;
- active warnings;
- pending restore or maintenance work.

## 17.2 Database

Expose:

- status;
- backup creation;
- backup list;
- verification;
- restore scheduling;
- restore progress and cancellation.

Restore operations require strong confirmation and a clear data-safety explanation.

## 17.3 Audit

Expose audit history and hash-chain verification in a technical but readable form.

## 17.4 Metrics

Use metrics for diagnostics, not as a decorative dashboard.

Graphs should be lazy-loaded and limited to data that helps investigate performance or failures.

## 17.5 Maintenance

Expose maintenance and cleanup actions with:

- scope;
- expected effect;
- destructive impact;
- progress;
- final report;
- logs or technical details.

---

# 18. Setup

The current connected setup flow should be retained and hardened rather than redesigned from scratch.

Required follow-up work:

- persist the full supported preference subset;
- report Windows installation results back through `POST /v1/setup/installation`;
- update the capability matrix to match the current code;
- remove stale statements that uninstallation is missing;
- add setup controller tests;
- add stage-level interaction tests;
- add failure-path tests;
- constrain setup-only Tauri commands;
- add real update and repair behavior when backend/desktop support exists;
- add explicit portable-mode choice only after portable persistence is defined.

The setup UI must continue to display real component failure and unsupported states.

---

# 19. Error and recovery model

The frontend must render structured API errors consistently.

Every error presentation should include, when available:

- stable code;
- concise user-facing message;
- retryability;
- affected entity;
- operation stage;
- expected and detected versions;
- relevant path or target;
- technical details;
- request identifier.

## 19.1 Severity levels

```text
Information
Warning
Recoverable error
Blocking error
Data-safety critical
```

## 19.2 Recovery patterns

Examples:

```text
The server stopped responding after 1.4 GB.
Your partial download is safe.

[Retry now] [Pause] [Details]
```

```text
This media selection requires FFmpeg.
Media Processing is disabled.

[Enable and install] [Choose another quality] [Cancel]
```

```text
The file is no longer at its recorded location.

[Locate file] [Keep marked as missing] [Move entry to Trash]
```

Never display raw Rust or database errors as the primary message.

---

# 20. Accessibility

## 20.1 Keyboard

Required behavior:

- Tab and Shift+Tab;
- arrow navigation in menus, lists, trees, and grids;
- Enter and Space activation;
- Escape to close transient layers;
- Ctrl+A for applicable lists;
- Shift and Ctrl multi-selection;
- standard Windows shortcuts where appropriate;
- visible access to all context-menu actions.

## 20.2 Focus

- Always show a visible focus indicator.
- Move focus into dialogs.
- Trap focus while modal.
- Restore focus after close.
- Preserve sensible focus after list mutations.

## 20.3 Screen readers

Test with Windows Narrator.

Requirements:

- meaningful control names;
- selection and position announcements;
- accessible progress values;
- polite status updates;
- no high-frequency progress spam;
- understandable error recovery;
- correct table/list semantics.

## 20.4 Contrast and scaling

Support:

- Light;
- Dark;
- Windows High Contrast;
- reduced transparency;
- reduced motion;
- 125%, 150%, 175%, and 200% scaling;
- mixed-DPI monitors;
- Windows text scaling.

Meet WCAG AA for application content.

---

# 21. Performance requirements

## 21.1 Virtualization

Required for:

- Downloads;
- Library;
- torrent file trees;
- media playlist items;
- page resources;
- logs;
- audit records;
- schedule execution history.

## 21.2 Loading strategy

- Do not load the entire database.
- Use backend pagination.
- Fetch details only when selected or opened.
- Cache with bounded eviction.
- Cancel stale requests.
- Avoid rendering hidden graphs and diagnostics.
- Load thumbnails lazily.

## 21.3 Targets

```text
Main shell interactive after backend readiness: < 2 seconds
Large-list scrolling: 60 FPS target
Visible progress rendering: approximately 10 Hz per active entity
Idle CPU: near zero
Caches: explicitly bounded
```

---

# 22. Shared components

Build shared components only when they are used by connected features.

Required set:

- Button;
- SplitButton;
- IconButton;
- TextField;
- SearchBox;
- TextArea;
- Checkbox;
- RadioGroup;
- Toggle;
- Dropdown;
- ComboBox;
- NumberField;
- PathPicker;
- ProgressBar;
- StatusBadge;
- Tooltip;
- Menu;
- ContextMenu;
- Dialog;
- Flyout;
- NavigationView;
- CommandBar;
- DataGrid or virtualized list;
- TreeView;
- DetailsPane;
- Tabs;
- InlineError;
- EmptyState;
- Skeleton;
- Toast/notification;
- Confirmation flow;
- ComponentStatusRow.

Every component must support:

- keyboard interaction;
- visible focus;
- accessible labeling;
- Light, Dark, and High Contrast;
- Compact and Comfortable density;
- disabled and busy states;
- validation where applicable.

---

# 23. Testing strategy

## 23.1 Current baseline

The backend contains substantial Rust test coverage, while the frontend currently has only a small component-state test suite.

Frontend test coverage must increase with every vertical slice.

## 23.2 Unit tests

Test:

- API error parsing;
- route serialization;
- pagination state;
- event ordering and replay;
- `resync_required` handling;
- progress coalescing;
- state-to-action mapping;
- filters and sorting;
- selection logic;
- destructive-action labeling;
- settings validation mapping.

## 23.3 Component tests

Test:

- keyboard behavior;
- focus restoration;
- loading, empty, and error states;
- compact and comfortable density;
- High Contrast semantics;
- progress and status announcements;
- dialogs and context menus.

## 23.4 Integration tests

Test real frontend/backend flows:

- list jobs;
- create direct job;
- pause/resume/cancel/retry;
- bulk job actions;
- event-driven progress;
- library import and restore;
- media probe and selection;
- torrent probe and file selection;
- basket start;
- rule preview;
- schedule creation and run-now;
- component install failure and retry;
- setup completion.

## 23.5 Windows end-to-end tests

Before public beta, test on clean Windows environments:

```text
install
→ first-run setup
→ launch installed copy
→ create a real download
→ restart Windows/app
→ resume or restore state
→ update application
→ verify readiness
→ force failed update
→ rollback
→ uninstall
```

Also test:

- no network;
- insufficient disk space;
- component checksum mismatch;
- antivirus quarantine symptoms;
- multiple monitors;
- mixed DPI;
- High Contrast;
- transparency disabled;
- Remote Desktop;
- non-ASCII user paths.

---

# 24. Implementation phases

## Phase 0 — Contract freeze and documentation correction

Before broad UI work:

- generate an inventory of current routes, events, settings, and models;
- update the setup capability matrix;
- remove stale documentation contradictions;
- define the frontend error mapping;
- define pagination and event-store conventions;
- mark release-only blockers separately from UI blockers.

**Exit criteria:** the frontend team can identify the exact route, model, events, and recovery behavior for every Phase 1–3 feature.

## Phase 1 — Main shell and infrastructure

Implement:

- native window shell;
- navigation;
- command bar;
- routed content host;
- details pane;
- connection state;
- global notifications;
- typed service boundaries;
- normalized stores;
- event reconnection and resync;
- theme and density;
- window persistence.

Do not add placeholder pages to navigation.

## Phase 2 — Core Downloads vertical slice

Implement fully:

- paginated/virtualized Downloads list;
- real filters and sorting;
- Add direct download;
- pause;
- resume;
- cancel;
- retry;
- remove;
- bulk actions;
- live events;
- details overview;
- outputs;
- actions;
- logs;
- complete error and recovery states.

**Exit criteria:** Ravyn is already useful as a direct download manager.

## Phase 3 — Library vertical slice

Implement:

- Library list;
- native shell icons;
- search and filters;
- import;
- verify;
- duplicates;
- Trash;
- restore;
- relocation;
- cleanup policies;
- personal statistics where useful.

## Phase 4 — Media and torrent workflows

Implement:

- media probing;
- format selection;
- playlist item selection;
- component gating;
- media-item retry;
- torrent probing;
- file-tree selection;
- torrent details;
- peers and seeding;
- rqbit failure recovery.

## Phase 5 — Basket and automation

Implement:

- Basket;
- presets;
- profiles;
- tags;
- Rules builder and preview;
- schedules and executions;
- browser token/capture workflows where product-ready.

## Phase 6 — Components and Settings

Implement:

- full Components page;
- settings categories;
- host profiles;
- secret references;
- appearance and density;
- update settings only when real updater support exists.

Do not expose a nonfunctional update-channel selector.

## Phase 7 — Diagnostics and maintenance

Implement:

- health overview;
- database status;
- backup and restore;
- audit verification;
- metrics;
- component diagnostics;
- event diagnostics;
- host/network diagnostics;
- maintenance and support export.

## Phase 8 — Desktop release completion

In parallel with frontend work, complete:

- Tauri release build;
- setup artifact;
- signing;
- remote app updater;
- repair and rollback;
- remote signed component manifests;
- 7-Zip provisioning strategy;
- portable mode;
- Tauri command restrictions;
- desktop CI;
- clean-machine E2E tests.

## Phase 9 — Hardening

Perform:

- accessibility audit;
- keyboard-only audit;
- High Contrast audit;
- performance audit;
- large-data testing;
- event-storm testing;
- visual regression testing;
- backend coverage review;
- removal of dead code and production mocks;
- documentation synchronization.

---

# 25. Definition of done

A frontend feature is complete only when:

- it is connected to real backend routes or Tauri commands;
- it uses real data;
- its event behavior is connected when applicable;
- loading state exists;
- empty state exists;
- structured errors are handled;
- retry or recovery is implemented;
- destructive behavior is explicit;
- keyboard use works;
- accessible labels and status exist;
- Light, Dark, and High Contrast work;
- Compact and Comfortable density work;
- large collections are virtualized or paginated;
- tests cover the important states;
- the API-to-UI map is updated;
- no production mock remains.

---

# 26. Required living documents

Maintain these files beside this plan.

## 26.1 `BACKEND_CAPABILITY_MATRIX.md`

For every major backend capability record:

| Field | Meaning |
|---|---|
| Backend source | Route, event, command, service, or table |
| User goal | Why the capability exists |
| Exposure | Primary, secondary, advanced, diagnostics, hidden |
| UI representation | Screen, control, status, or none |
| Risk | Low, medium, high |
| Frontend status | Planned, connected, tested |
| Backend status | Complete, partial, release-blocked |

## 26.2 `API_UI_MAP.md`

Map:

```text
route or command
→ frontend service
→ store
→ screen/component
→ events
→ tests
```

## 26.3 `SCREEN_INVENTORY.md`

Every screen records:

- purpose;
- entry points;
- routes and commands;
- events;
- loading;
- empty state;
- errors;
- recovery;
- destructive actions;
- keyboard behavior;
- accessibility;
- tests.

## 26.4 `DESKTOP_RELEASE_CHECKLIST.md`

Track:

- frontend CI;
- Tauri build;
- setup artifact;
- signing;
- updater;
- rollback;
- portable mode;
- command restrictions;
- engine manifest delivery;
- Windows E2E tests;
- uninstall.

---

# 27. Coding-agent rules

Any coding agent working on Ravyn must:

1. Read `AGENTS.md`, this plan, current migrations, routes, OpenAPI, event models, and relevant services before editing.
2. Inspect the current implementation instead of trusting stale roadmap text.
3. Use current library documentation when changing Tauri, Svelte, Axum, SQLx, or other dependencies.
4. Write code comments in English.
5. Build complete backend-connected vertical slices.
6. Never add production mock data.
7. Never add visible controls without implemented behavior.
8. Preserve structured errors and request identifiers.
9. Preserve API-token authentication for the embedded backend.
10. Keep privileged native behavior behind constrained Tauri commands.
11. Add tests with each feature.
12. Update the capability matrix and API-to-UI map with every completed slice.
13. Avoid long, unrelated refactors while implementing a feature.
14. Treat release engineering gaps separately from stable application-domain contracts.
15. Stop and fix contract mismatches when discovered rather than hiding them in the UI.

---

# 28. Final direction

Ravyn should now move from backend-heavy development into frontend vertical slices.

The correct strategy is:

```text
begin the real main frontend now
+ keep the existing connected setup
+ treat Downloads as the first complete product slice
+ build Library next
+ integrate media and torrent workflows
+ expose automation progressively
+ keep advanced backend power in details and diagnostics
+ finish desktop release/update/security work in parallel
```

The backend core is sufficiently complete for this transition.

The project is **not yet ready for a public desktop release**, because distribution, application update/repair, remote signed component delivery, portable mode, Tauri privilege restrictions, and Windows end-to-end validation remain incomplete.

Those remaining blocks should shape the release roadmap, not prevent the frontend from starting.
