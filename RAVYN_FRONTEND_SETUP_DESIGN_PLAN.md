# Ravyn — Frontend, Setup, and Native Windows UX Master Plan

> **Document purpose:** This is the authoritative design and implementation plan for the Ravyn frontend.
>
> **Absolute priority:** Build the custom setup experience first, then the main application.
>
> **Non-negotiable implementation rule:** Every visible feature must be connected to the real backend immediately. Ravyn must be built as working vertical slices, not as a complete visual mockup that is wired later.

---

# 1. Product vision

Ravyn must feel like a genuine Windows 11 application designed for power users and normal users at the same time.

The target experience is:

```text
native Windows behavior
+ Fluent Design 2
+ File Explorer–style information density
+ fast download management
+ modular engine provisioning
+ progressive disclosure
+ real backend integration from the first component
```

Ravyn must not look like:

- a generic web dashboard inside a desktop window;
- a Material Design application;
- a macOS-inspired application;
- a mobile interface stretched onto desktop;
- an AI-generated collection of oversized cards;
- a visual prototype with fake data;
- a traditional NSIS/MSI wizard exposed to the user.

The application may have a recognizable Ravyn identity, but Windows conventions always take priority over decorative branding.

---

# 2. Core implementation principles

## 2.1 Setup first

The first complete frontend feature must be the custom Ravyn setup.

The setup is not a disposable prototype. It is the first production use of the shared design system, native shell integration, backend API client, event system, error handling, and accessibility standards.

The implementation order is:

```text
1. Shared tokens and foundational controls
2. Native window shell required by setup
3. Custom Ravyn setup
4. Setup-to-main-app transition
5. Main application shell
6. Primary download workflows
7. Secondary and advanced features
```

Only the minimum shared infrastructure required by the setup may be built before it.

## 2.2 Backend-connected vertical slices

A component is not complete because it looks correct.

A component is complete only when it:

- calls the real backend;
- displays real backend data;
- subscribes to the required real events;
- handles loading;
- handles empty states;
- handles stable backend errors;
- provides retry or recovery;
- supports keyboard interaction;
- has tests;
- contains no production mock data.

The required workflow for every feature is:

```text
inspect backend contract
→ design the user interaction
→ implement UI
→ connect API or Tauri command
→ connect events
→ implement all states
→ test end to end
→ document coverage
```

Do not first build the entire interface and connect it later.

Do not create buttons that do nothing.

Do not create fake progress bars.

Do not create placeholder pages in the production navigation.

## 2.3 Backend as capability source, user goals as UX source

The backend is the source of truth for:

- available operations;
- data models;
- validation rules;
- error codes;
- events;
- security restrictions;
- persisted state.

The backend must not dictate the visual structure directly.

For every backend capability, decide:

```text
backend capability
+ user goal
+ usage frequency
+ risk
+ complexity
= correct UI representation
```

Not every route needs a dedicated button.
Not every database field needs to be visible.
Not every advanced option belongs in the primary flow.

## 2.4 Progressive disclosure

Common tasks must remain simple.

Advanced functionality must remain available without overwhelming the default experience.

Examples:

- paste a URL first;
- show headers, proxy, checksum, scheduling, and post-processing only in Advanced;
- show normal progress and errors in the download list;
- show HTTP ranges, process logs, and segment diagnostics only in Details or Diagnostics;
- expose presets before raw command arguments.

## 2.5 Native Windows first

Ravyn must follow Windows behavior for:

- title bars;
- caption controls;
- Snap Layouts;
- system menus;
- focus;
- keyboard navigation;
- context menus;
- file and folder pickers;
- taskbar;
- tray;
- notifications;
- theme;
- accent color;
- High Contrast;
- per-monitor DPI;
- text scaling;
- window restoration.

---

# 3. Application distribution model

Ravyn should offer two official modes.

## 3.1 Installed mode

The normal public download is a single custom executable:

```text
RavynSetup.exe
```

The user must not see a classic NSIS or MSI wizard.

The setup application:

- uses the same Fluent design system as Ravyn;
- installs per user by default;
- requires no administrator privileges for the default path;
- installs Ravyn under the user's local application directory;
- registers Ravyn in Windows Installed Apps;
- creates Start Menu integration;
- optionally creates a desktop shortcut;
- optionally enables startup with Windows;
- stores the selected feature configuration;
- launches the installed Ravyn application;
- allows optional engine provisioning to continue in the main app.

Recommended default locations:

```text
Application:
%LOCALAPPDATA%\Programs\Ravyn

Application data:
%LOCALAPPDATA%\Ravyn

Default Ravyn library:
%USERPROFILE%\Downloads\Ravyn
```

An advanced option may allow system-wide installation later, but it must not be the default.

## 3.2 Portable mode

A separate portable build may be offered:

```text
RavynPortable.exe
```

Portable mode:

- does not register an installed application;
- does not create shortcuts without explicit consent;
- does not modify PATH;
- stores portable state in a predictable adjacent data directory;
- clearly identifies itself as portable;
- may still run the same first-run feature setup.

Installed mode and portable mode must not silently share incompatible configuration paths.

## 3.3 Reliable installation internals

The user-facing setup is custom, but the installation logic must still be robust.

It must support:

- detecting an existing installation;
- upgrade;
- repair;
- atomic executable replacement;
- rollback;
- file-in-use handling;
- Start Menu shortcuts;
- optional desktop shortcut;
- Installed Apps registration;
- uninstall;
- protocol registration when introduced;
- optional file associations when introduced;
- preserving or removing user data based on explicit choice;
- signed release verification;
- cleanup after failed installation.

Do not invent an unsafe package format merely to avoid NSIS/MSI branding.

---

# 4. Custom setup — absolute first priority

## 4.1 Setup goals

The setup must:

- introduce Ravyn briefly;
- install or repair the main application;
- let the user select features rather than technical engines;
- choose the Ravyn library location;
- expose only a small number of useful preferences;
- install selected managed components;
- display real progress;
- recover gracefully from partial failure;
- transition cleanly into the main application.

The setup should feel like the first screen of Ravyn, not a separate legacy installer.

## 4.2 Setup visual direction

Use:

- Fluent Design 2;
- Segoe UI Variable;
- native Windows caption controls;
- Mica on supported Windows 11 systems;
- solid fallback;
- subtle Ravyn branding;
- Fluent native-style icons;
- compact layouts;
- clear hierarchy;
- restrained motion.

Avoid:

- giant illustration-only pages;
- purple/blue decorative gradients;
- glass effects behind every control;
- excessive rounded cards;
- carousel onboarding;
- long marketing text;
- technical engine names as the main labels;
- fake progress.

## 4.3 Setup pages

The setup should contain no more than the following primary stages.

### Stage 1 — Welcome

Purpose:

- identify Ravyn;
- explain its value in one or two lines;
- detect whether this is install, upgrade, repair, or first-run configuration.

Example:

```text
Welcome to Ravyn

A modern download manager for files, media, archives, and torrents.

[Get started]
```

For an existing installation:

```text
Ravyn is already installed

[Update]
[Repair]
[Open Ravyn]
```

### Stage 2 — Setup type

Offer simple choices:

```text
Recommended
Standard downloads, video, media processing, and archive extraction

Minimal
Standard downloads only

Everything
All available features

Custom
Choose features manually
```

These are frontend presets for feature selection. The real selected features must be persisted through the backend.

### Stage 3 — Feature selection

Display user-facing capabilities, not engine package names.

```text
[x] Standard downloads
    Core HTTP/HTTPS downloads. Always enabled.

[x] Video and playlists
    Download supported video, audio, playlists, and channels.
    Powered by yt-dlp.

[x] High-quality media processing
    Merge streams, probe media, and convert formats.
    Powered by FFmpeg.

[ ] Torrent downloads
    Download magnet links and torrent files.
    Powered by rqbit.

[x] Archive extraction
    Extract supported archive formats after download.
    Powered by 7-Zip.
```

Rules:

- Standard downloads are always enabled.
- A disabled feature must never be installed silently.
- Selecting a feature may recommend another dependent feature, but must explain it.
- Technical engine names may appear as secondary details.
- Show estimated download size and disk usage when the backend provides them.
- Show unsupported features clearly instead of hiding failures.

### Stage 4 — Library location

Default:

```text
%USERPROFILE%\Downloads\Ravyn
```

Allow:

- use recommended location;
- choose another folder;
- choose another drive;
- show required permissions;
- show available disk space;
- validate the path through the backend;
- explain the generated structure.

Default structure:

```text
Ravyn
├── Downloads
├── Videos
├── Music
├── Documents
├── Images
├── Archives
├── Torrents
├── Playlists
├── Temporary
└── Trash
```

The exact active structure must come from backend configuration and must not be hardcoded if the backend later changes it.

### Stage 5 — Preferences

Keep this stage short.

Possible settings:

- launch Ravyn after setup;
- start Ravyn with Windows;
- create desktop shortcut;
- close behavior;
- theme: System by default;
- update channel only if the backend supports it safely.

Advanced installation settings may be placed behind a single link.

### Stage 6 — Installation and provisioning

Show separate real operations:

```text
Installing Ravyn
Creating shortcuts
Preparing the Ravyn library
Saving preferences
Installing Video and Playlists
Installing Media Processing
Installing Archive Extraction
```

Component states must come from the backend:

```text
not_installed
queued
downloading
verifying
installing
installed
update_available
failed
unsupported
custom_path
cancelled
```

Each active operation may show:

- current state;
- progress percentage;
- bytes downloaded;
- speed when available;
- retry action after failure;
- expandable technical details.

The entire setup must not fail because one optional component fails.

Example:

```text
Ravyn installed successfully

Video and playlists       Installed
Media processing          Failed — Retry available
Torrent support           Not selected
Archive extraction        Installed

[Open Ravyn]
```

### Stage 7 — Completion

The final screen must state what is ready and what remains.

```text
Ravyn is ready

Your library:
C:\Users\Name\Downloads\Ravyn

3 optional components installed
1 component requires attention

[Open Ravyn]
```

Do not use a meaningless Finish button when Open Ravyn is the real next action.

## 4.4 Setup backend integration

The setup must connect immediately to the real backend or shared installation service.

Required backend capabilities include:

- query installation state;
- detect existing installation;
- detect update or repair state;
- read supported feature catalog;
- read feature dependencies;
- read component states;
- save selected feature profile;
- validate application path;
- validate library path;
- create library directories;
- install or update Ravyn;
- provision selected components;
- subscribe to provisioning events;
- cancel provisioning;
- retry provisioning;
- rollback a component;
- remove a managed component;
- save preferences;
- complete setup;
- launch the main application.

If a required route or command does not exist, implement it before claiming the setup screen is complete.

## 4.5 Setup transition

The transition between setup and the main application must be deterministic.

Required sequence:

```text
setup preferences committed
→ main application process or window launched
→ backend reports main app ready
→ main window becomes visible
→ setup window closes
```

Do not use repeated arbitrary timeouts or polling loops to guess whether the main window has opened.

## 4.6 Setup error handling

Required error classes:

- application installation failed;
- update failed;
- repair failed;
- library location denied;
- insufficient disk space;
- network unavailable;
- component download failed;
- checksum verification failed;
- signature verification failed;
- unsupported platform;
- antivirus quarantine suspected;
- custom engine path invalid;
- setup cancellation;
- rollback failed.

For each error define:

- stable code;
- user-facing explanation;
- whether Ravyn remains usable;
- retry action;
- fallback action;
- data safety statement;
- technical details view.

---

# 5. Managed feature and component UX

## 5.1 Feature model

The frontend presents capabilities:

| User-facing feature | Backend component |
|---|---|
| Standard downloads | Ravyn core |
| Video and playlists | yt-dlp |
| Media processing | FFmpeg |
| Torrent downloads | rqbit |
| Archive extraction | 7-Zip |

The frontend must not treat engine names as the primary navigation model.

## 5.2 Distinct states

The UI must distinguish:

```text
Feature disabled
The user intentionally disabled it.

Component not installed
The feature is enabled, but provisioning has not completed.

Installation failed
The component was requested but could not be installed.

Unsupported
No verified artifact exists for the current platform.

Custom path
The user supplied an external executable.

Custom path invalid
The configured executable cannot be used.

Update available
A compatible verified update exists.

Rollback available
A previous verified managed version can be restored.
```

These states require different messages and actions.

## 5.3 On-demand installation

When the user requests a feature that is not installed:

```text
Torrent support is not installed

Ravyn needs the Torrent component to open this magnet link.

[Install component]
[Cancel]
```

For media:

```text
This quality requires media processing

Install Media Processing to merge the selected video and audio streams?

[Install]
[Choose another quality]
[Cancel]
```

Never silently install a feature the user disabled.

## 5.4 Components settings page

After setup, provide:

```text
Settings
└── Components
```

Each feature row displays:

- user-facing feature name;
- secondary engine name;
- enabled or disabled preference;
- installation state;
- installed version;
- available version;
- integrity state;
- disk usage;
- source: managed or custom;
- last verified time;
- actions.

Possible actions:

- Enable;
- Disable;
- Install;
- Update;
- Retry;
- Cancel;
- Roll back;
- Remove;
- Choose custom executable;
- Restore managed version;
- Verify now;
- Open logs.

---

# 6. Native Windows visual system

## 6.1 Overall visual reference

Ravyn should take structural inspiration from modern Windows 11 File Explorer and Windows Settings:

- native title bar;
- left navigation pane;
- compact command bar;
- content list;
- optional details pane;
- restrained separators;
- high information density;
- native context menus;
- Windows-like spacing and typography.

This does not mean cloning File Explorer. It means matching the same interaction grammar.

## 6.2 Window materials

Default behavior:

```text
Windows 11
→ Mica

Windows 10 where supported
→ Acrylic or solid Fluent fallback

High Contrast
→ Solid

Transparency disabled in Windows
→ Solid

Remote Desktop or degraded composition
→ Solid or reduced transparency
```

Settings:

```text
Window material
- Automatic
- Mica
- Acrylic
- Solid

Transparency effects
- On
- Off
```

Materials enhance the app but must never be required for readability.

## 6.3 Shape tokens

| Token | Value | Usage |
|---|---:|---|
| `radius-small` | 2px | very small controls |
| `radius-control` | 4px | text fields, checkboxes |
| `radius-medium` | 6px | buttons, selectors, rows |
| `radius-layer` | 8px | flyouts, panels, dialogs |
| `radius-large` | 12px | major surfaces only |
| `radius-pill` | 999px | compact status filters only |

Do not give every object the same large radius.

## 6.4 Spacing tokens

Use a 4px system:

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

## 6.5 Control sizes

| Token | Height |
|---|---:|
| `control-compact` | 28px |
| `control-default` | 32px |
| `control-large` | 40px |
| `hit-target-minimum` | 40px |

## 6.6 Density modes

### Comfortable

For normal use and touch-friendly interaction.

### Compact

For users managing hundreds or thousands of items.

Density changes:

- row height;
- cell padding;
- command bar spacing;
- metadata visibility;
- details panel density;
- form spacing.

## 6.7 Typography

```css
font-family:
  "Segoe UI Variable",
  "Segoe UI",
  system-ui,
  sans-serif;
```

Use a consistent Fluent type ramp.

Do not invent isolated font sizes per component.

## 6.8 Color and theme

Support:

- System;
- Dark;
- Light;
- Windows High Contrast.

Default:

```text
Theme = System
```

Use Windows accent color by default.

Derived tokens:

```text
accent-default
accent-hover
accent-pressed
accent-subtle
accent-border
accent-text
accent-on-color
```

Fallback accent:

```text
#0078D4
```

Never communicate status only through color.

---

# 7. Native icon strategy

## 7.1 General rule

Ravyn should use icons that look native to Windows 11.

Do not use:

- emoji as interface icons;
- random mixed icon libraries;
- macOS symbols;
- Material icons;
- custom glyph fonts for standard Windows actions;
- inconsistent stroke widths.

## 7.2 File, folder, drive, and shell icons

For file-system objects, use icons supplied by Windows whenever practical.

Examples:

- file type icons;
- folder icons;
- drive icons;
- network location icons;
- removable storage icons;
- known-folder icons;
- application icons associated with file types.

These should be resolved through a native Windows shell icon service exposed by the Rust/Tauri layer.

The frontend must request semantic objects such as:

```text
file icon for .zip
folder icon for Downloads
drive icon for C:
known-folder icon for Music
```

It must not ship inaccurate imitations of Windows shell icons.

Cache resolved shell icons by:

- extension;
- known folder;
- shell item type;
- DPI;
- theme when relevant.

Provide fallbacks when native resolution fails.

## 7.3 Application action icons

For application actions use **Fluent UI System Icons** or native Windows-provided equivalents.

Examples:

- Add;
- Download;
- Pause;
- Resume;
- Retry;
- Cancel;
- Delete;
- Restore;
- Search;
- Filter;
- Sort;
- Settings;
- More;
- Open folder;
- Copy link;
- Properties;
- Information;
- Warning;
- Error.

Use consistent regular/filled variants:

- regular for normal actions;
- filled for selected or emphasized state only.

## 7.4 Icon accessibility

Every icon-only control must include:

- accessible name;
- tooltip;
- visible focus state;
- minimum hit target;
- pressed or selected state when applicable.

---

# 8. Native Windows shell and behavior

## 8.1 Title bar

Use native Windows caption controls:

```text
Minimize
Maximize / Restore
Close
```

Required behavior:

- Snap Layouts;
- drag from title bar;
- double-click to maximize or restore;
- `Alt+Space`;
- native resize borders;
- keyboard-accessible caption controls;
- correct taskbar previews.

## 8.2 Window persistence

Persist:

- last non-maximized size;
- last valid position;
- maximized state;
- selected monitor when available.

Validate restored bounds.

If a monitor is disconnected, restore the window to a visible area.

## 8.3 File Explorer–style shell

The main window should use a desktop-oriented layout:

```text
Native title bar
Command bar
Left navigation
Main content
Optional details pane
Status bar where useful
```

The details pane must be optional and resizable.

It may show:

- selected download summary;
- progress;
- source;
- destination;
- outputs;
- integrity;
- component requirements;
- recent errors;
- quick actions.

## 8.4 Command bar

Use a compact Windows-style command bar.

Common actions:

- Add download;
- Paste;
- Pause;
- Resume;
- Retry;
- Cancel;
- Open folder;
- Delete;
- Sort;
- View;
- Filter;
- More.

Only show actions relevant to the current selection and screen.

## 8.5 Context menus

Use native-feeling context menus with:

- icons where appropriate;
- standard keyboard navigation;
- separators based on action groups;
- clear destructive labels;
- no ambiguous generic Delete item.

Examples:

```text
Pause
Resume
Retry
Open file
Open containing folder
Copy source URL
Move to...
Remove from list
Move file to Trash
Delete file permanently
Properties
```

---

# 9. Main application information architecture

The exact navigation must be validated against the final backend capability matrix.

Recommended initial top-level navigation:

```text
Downloads
Library
Basket
Schedule
Rules
Components
Settings
Diagnostics
```

Possible secondary views:

```text
Downloads
├── Active
├── Queued
├── Completed
├── Failed
└── All

Library
├── All
├── Videos
├── Music
├── Documents
├── Images
├── Archives
├── Torrents
├── Playlists
└── Trash
```

Do not add a top-level screen merely because a backend table exists.

---

# 10. Primary screens

## 10.1 Downloads

This is the most important screen in the application.

It must support:

- large virtualized lists;
- compact and comfortable density;
- sorting;
- search;
- filtering;
- saved views;
- multi-select;
- keyboard selection;
- drag and drop;
- context menus;
- column customization;
- bulk actions;
- live progress;
- total speed;
- ETA;
- current state;
- engine or mode only when useful;
- inline recoverable errors.

The list should be closer to File Explorer than to a card dashboard.

## 10.2 Add download

Primary flow:

```text
Paste URL
→ detect content
→ show relevant options
→ apply preset or rules
→ confirm
→ create real backend job
```

Support:

- direct HTTP/HTTPS;
- media URLs;
- playlists;
- magnet links;
- torrent files;
- batch URL input.

Advanced sections may contain:

- destination;
- filename;
- checksum;
- headers;
- proxy;
- schedule;
- post-processing;
- limits;
- tags.

## 10.3 Media selection

Show real probe results.

Allow:

- video quality;
- audio quality;
- container;
- codec;
- subtitles;
- thumbnails;
- metadata;
- playlist item selection;
- estimated size when available;
- FFmpeg requirement;
- alternative selection when media processing is unavailable.

## 10.4 Torrent selection

Show:

- torrent name;
- total size;
- file tree;
- per-file selection;
- priority;
- destination;
- seed behavior;
- rqbit requirement;
- availability and errors.

Large torrent file trees must be virtualized.

## 10.5 Download details

Tabs or sections may include:

- Overview;
- Files and outputs;
- Activity;
- Integrity;
- Media or torrent details;
- Advanced diagnostics.

The default view must remain understandable.

Do not expose raw backend internals in Overview.

## 10.6 Library

The Library is a persistent searchable catalog, not only download history.

Support:

- search;
- category;
- tags;
- source;
- date;
- size;
- hash;
- media metadata;
- existing-file status;
- moved-file repair;
- duplicate relationships;
- open file;
- open folder;
- reuse local copy;
- move to Trash;
- restore.

Use Windows shell icons for file types and folders.

## 10.7 Basket

Support:

- collecting jobs before creation;
- reordering;
- editing;
- preset application;
- destination changes;
- validation;
- duplicate review;
- start selected;
- start all.

## 10.8 Scheduler

Support:

- scheduled jobs;
- recurring schedules;
- missed-run behavior;
- conflict handling;
- enabled state;
- next run;
- history;
- clear validation.

Do not expose raw cron syntax first. Offer understandable scheduling controls, with advanced syntax optional.

## 10.9 Rules

Use a visual rule builder:

```text
WHEN [condition]
AND [condition]
THEN [action]
AND [action]
```

Support:

- priority;
- enable/disable;
- preview;
- test against a URL or existing item;
- conflict explanation;
- validation;
- advanced raw representation only when needed.

## 10.10 Components

Defined in Section 5.

## 10.11 Settings

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

## 10.12 Diagnostics

Diagnostics may expose:

- backend connection;
- database state;
- engine capability details;
- component verification;
- recent structured logs;
- event connection;
- network tests;
- torrent diagnostics;
- storage permissions;
- migration status;
- export support bundle.

Diagnostics must not dominate the normal user experience.

---

# 11. Shared component system

Build only components that are used by real connected features.

Required foundations:

- Button;
- Split button;
- Icon button;
- Text field;
- Search box;
- Checkbox;
- Radio group;
- Toggle;
- Dropdown;
- Combo box;
- Progress bar;
- Indeterminate progress;
- Status badge;
- Tooltip;
- Menu;
- Context menu;
- Dialog;
- Flyout;
- Navigation view;
- Command bar;
- Virtualized data list;
- Tree view;
- Details pane;
- Inline error;
- Empty state;
- Skeleton;
- Toast or in-app notification;
- Path picker;
- Feature/component row;
- Setup step container.

Each component must support:

- keyboard;
- focus;
- accessible labels;
- light/dark;
- High Contrast;
- compact/comfortable density;
- disabled state;
- validation where relevant;
- real production usage before being considered stable.

---

# 12. Frontend architecture

## 12.1 Stack

```text
Svelte
TypeScript
Vite
Tauri
Rust native integration
```

Use strict TypeScript.

Avoid a large UI framework that imposes Material or generic SaaS styling.

## 12.2 API client

Generate or strongly type the frontend API client from the current backend OpenAPI contract where practical.

Requirements:

- stable request and response types;
- stable error model;
- cancellation support;
- timeouts;
- pagination;
- retry only where safe;
- request correlation identifiers;
- no duplicated hand-written endpoint shapes.

## 12.3 Event client

Create a central event service.

Pipeline:

```text
backend events
→ validate event
→ buffer
→ coalesce by entity
→ update normalized stores
→ render visible components only
```

Visible progress updates should normally be capped around 10 Hz per active row.

Provisioning events and download events must use the same disciplined architecture.

## 12.4 Stores

Use normalized stores for:

- connection state;
- setup state;
- component state;
- feature preferences;
- jobs;
- selected jobs;
- library entries;
- rules;
- schedules;
- settings;
- notifications.

Avoid one global store that rerenders the entire application.

## 12.5 Native platform services

Keep platform behavior behind dedicated services:

- shell icons;
- window materials;
- accent color;
- theme;
- High Contrast;
- notifications;
- file and folder pickers;
- taskbar progress;
- tray;
- startup registration;
- shortcuts;
- installation state;
- protocol registration;
- uninstall integration.

Do not scatter native calls across Svelte components.

---

# 13. Real backend integration contract

Every screen specification must include:

| Field | Required |
|---|---|
| User goal | Yes |
| Backend route or command | Yes |
| Backend event subscription | When applicable |
| Data model | Yes |
| Loading state | Yes |
| Empty state | Yes |
| Error codes | Yes |
| Retry/recovery | Yes |
| Permission/security constraints | Yes |
| Destructive behavior | When applicable |
| Keyboard behavior | Yes |
| Accessibility behavior | Yes |
| Integration tests | Yes |

A frontend issue is not complete until all applicable fields are implemented.

## 13.1 No visual-only milestones

Forbidden milestone:

```text
Build every screen visually, then connect backend later.
```

Required milestone:

```text
Complete Setup Feature Selection:
- final visual design
- real feature catalog
- real saved selection
- real validation
- real component state
- real errors
- tests
```

Then move to the next vertical slice.

## 13.2 Mock data policy

Mock data is allowed only in:

- isolated component stories;
- unit tests;
- screenshot tests;
- prototypes outside production routes.

No production screen may ship with mock data or delayed backend wiring.

---

# 14. Application states

## 14.1 Global states

```text
Installing application
Repairing installation
Updating application
First-run setup required
Setup incomplete
Starting backend
Connected
Disconnected
Reconnecting
Migration required
Database error
Read-only recovery mode
Component provisioning
Component installation partially failed
Component update available
Required component disabled
Required component unavailable
Native integration unavailable
Application update available
```

## 14.2 Job states

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

## 14.3 Component states

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
Custom path
Custom path invalid
Cancelled
Rollback available
```

Every state must have:

- text;
- icon;
- accessible description;
- permitted actions;
- backend mapping.

---

# 15. Error and recovery design

For each operation define:

- stable error code;
- concise message;
- optional technical details;
- retry;
- fallback;
- recovery steps;
- whether data remains safe;
- whether the app remains usable.

Examples:

```text
The server stopped responding after 1.4 GB.
Your partial download is safe.
Ravyn will retry in 30 seconds.

[Retry now]
[Pause]
[Details]
```

```text
The selected media quality requires FFmpeg.
Media Processing is disabled.

[Enable and install]
[Choose another quality]
```

```text
The file was moved outside the Ravyn library.
A matching file was found in D:\Videos.

[Repair location]
[Keep missing]
```

Destructive actions must be explicit:

- Remove from list;
- Move file to Trash;
- Delete permanently;
- Delete torrent data;
- Forget metadata;
- Cancel transfer.

Do not use one ambiguous Delete command.

---

# 16. Accessibility

## 16.1 Keyboard

Every action must support keyboard use.

Required conventions:

- Tab and Shift+Tab;
- arrow keys in lists, menus, and trees;
- Enter;
- Space;
- Escape;
- Ctrl+A for list selection where appropriate;
- Delete only when action is unambiguous and recoverable;
- standard Windows shortcuts where appropriate.

## 16.2 Focus

Use visible focus indicators.

Dialogs must:

- move focus inside;
- trap focus;
- restore focus after closing.

## 16.3 Screen readers

Test with Windows Narrator.

Requirements:

- meaningful names;
- list position and selection;
- accessible progress;
- clear status;
- polite announcements;
- no byte-level progress spam;
- setup steps announced correctly;
- component failures announced clearly.

## 16.4 Contrast and transparency

Meet WCAG AA.

Mica and Acrylic must never reduce readability.

Use solid or sufficiently opaque backplates where required.

High Contrast always overrides decorative material choices.

## 16.5 Scaling

Support:

- 125%;
- 150%;
- 175%;
- 200%;
- mixed-DPI monitors;
- Windows text scaling.

---

# 17. Motion

Use short Fluent-style motion:

```text
150–250 ms
cubic-bezier(0.1, 0.9, 0.2, 1)
```

Motion may reinforce:

- setup step transitions;
- selection;
- navigation;
- expansion;
- drag operations;
- state changes.

Motion must not delay installation, downloads, or primary actions.

Respect reduced motion.

---

# 18. Performance

Ravyn must remain responsive with large datasets.

## 18.1 Virtualization

Required for:

- downloads;
- library entries;
- torrent files;
- page resources;
- logs;
- scheduler history;
- rule lists when large.

## 18.2 Pagination and lazy loading

Do not load the entire database.

Load details only when selected or opened.

## 18.3 Rendering

Do not render:

- off-screen graphs;
- off-screen thumbnails;
- collapsed diagnostics;
- full segment maps before requested.

## 18.4 Targets

```text
Setup window interactive:
< 2 seconds after process launch on a normal modern PC

Main shell interactive after backend connection:
< 2 seconds

Large-list scrolling:
60 FPS target

Visible progress updates:
up to approximately 10 Hz per active row

Idle CPU:
near zero

Memory:
bounded caches with eviction
```

---

# 19. Testing strategy

## 19.1 Setup tests

Test:

- clean install;
- reinstall;
- upgrade;
- repair;
- uninstall;
- portable mode;
- no network;
- partial component failure;
- checksum mismatch;
- unsupported component;
- insufficient disk space;
- invalid library path;
- cancelled setup;
- rollback;
- launch handoff;
- retained preferences;
- Windows restart after installation.

## 19.2 Windows visual and behavior tests

Test:

- Windows 11 with Mica;
- Windows 11 transparency disabled;
- Windows 11 High Contrast;
- Windows 10 fallback;
- Remote Desktop;
- battery saver;
- multiple monitors;
- mixed DPI;
- 200% scaling;
- Snap Layouts;
- `Alt+Space`;
- taskbar behavior;
- tray behavior.

## 19.3 Data-scale tests

Test:

- 1 active job;
- 100 active jobs;
- 1,000 stored jobs;
- 10,000 history or library entries;
- thousands of torrent files;
- high-frequency events;
- rapid filtering and selection.

## 19.4 Accessibility tests

Test:

- Narrator;
- keyboard only;
- High Contrast;
- reduced motion;
- reduced transparency;
- focus restoration;
- accessible names;
- status announcements.

## 19.5 Backend coverage review

Before each release:

1. scan backend models;
2. scan routes and Tauri commands;
3. scan event types;
4. scan migrations;
5. compare with the capability matrix;
6. identify missing UI coverage;
7. identify obsolete frontend contracts;
8. identify any mock data;
9. update tests and documentation.

---

# 20. Required design and implementation artifacts

Maintain:

## 20.1 Capability matrix

| Field | Description |
|---|---|
| Backend source | Route, model, event, command, or table |
| User goal | Why it exists |
| Exposure level | Primary, secondary, advanced, diagnostics, hidden |
| UI representation | Screen, control, status, or none |
| Backend dependency | Exact route, command, or event |
| Risk | Low, medium, high |
| Status | Planned, connected, tested |

## 20.2 Screen inventory

Every screen records:

- purpose;
- entry points;
- exit points;
- APIs;
- commands;
- events;
- loading;
- empty;
- errors;
- recovery;
- permissions;
- destructive actions;
- keyboard behavior;
- accessibility;
- tests.

## 20.3 API-to-UI map

Maintain a mapping among:

```text
backend routes and commands
events
frontend services
stores
screens
components
tests
```

## 20.4 Native integration matrix

Track:

- installation;
- window shell;
- Mica/Acrylic;
- shell icons;
- taskbar;
- tray;
- notifications;
- startup;
- shortcuts;
- Installed Apps;
- uninstall;
- file pickers;
- protocol handlers;
- DPI;
- theme;
- accent;
- High Contrast.

---

# 21. Implementation phases

## Phase 0 — Backend and contract audit

Before frontend coding:

- inspect the complete backend;
- verify component provisioning APIs;
- verify setup state persistence;
- verify stable error codes;
- verify events;
- verify OpenAPI;
- identify missing backend endpoints;
- implement missing backend contracts required by setup.

Deliverables:

- capability matrix;
- setup API map;
- component state map;
- setup error matrix.

## Phase 1 — Shared foundation for setup

Build only what setup needs:

- design tokens;
- theme;
- accent;
- typography;
- buttons;
- fields;
- checkboxes;
- radio groups;
- progress;
- dialogs;
- tooltips;
- inline errors;
- setup layout;
- native title bar behavior;
- platform service boundaries;
- typed backend client;
- event client.

Every control must already meet accessibility and theme requirements.

## Phase 2 — Custom setup

Implement the entire setup as connected vertical slices:

1. Installation detection;
2. Welcome and install/repair/update mode;
3. Setup type;
4. Feature selection;
5. Library location;
6. Preferences;
7. Installation;
8. Component provisioning;
9. Failure and retry;
10. Completion;
11. Main-app handoff;
12. Uninstall and repair entry points.

This phase is complete only when a clean Windows machine can install and open a usable Ravyn build.

## Phase 3 — Main shell

Implement:

- native window;
- left navigation;
- command bar;
- content host;
- details pane;
- global connection state;
- notifications;
- shell icon service;
- theme and density;
- window persistence.

No placeholder navigation pages.

## Phase 4 — Core downloads

Implement connected flows:

- Downloads list;
- Add direct download;
- Pause;
- Resume;
- Retry;
- Cancel;
- Remove;
- Open file;
- Open folder;
- live progress;
- errors;
- details.

## Phase 5 — Media, torrent, and archive flows

Implement:

- media probing;
- media selection;
- FFmpeg requirement;
- torrent selection;
- rqbit on-demand installation;
- archive post-processing;
- component gating and recovery.

## Phase 6 — Library and organization

Implement:

- Library;
- categories;
- shell icons;
- search;
- tags;
- duplicate handling;
- moved-file repair;
- Trash;
- restore;
- import.

## Phase 7 — Basket, presets, scheduling, and rules

Implement each as a complete backend-connected vertical slice.

## Phase 8 — Settings, Components, and Diagnostics

Complete:

- all appearance settings;
- component management;
- network settings;
- automation settings;
- security;
- updates;
- diagnostics;
- support export.

## Phase 9 — Hardening

Perform:

- accessibility audit;
- performance audit;
- native Windows comparison;
- large-data tests;
- visual regression;
- backend coverage review;
- removal of mock data;
- dead component cleanup;
- documentation update.

---

# 22. Out of scope for the initial frontend

Until the main application is complete and stable:

- browser extension;
- mobile client;
- web remote client;
- decorative AI features;
- plugin marketplace UI;
- unsupported platform-specific redesigns.

The architecture may leave room for these later, but they must not distract from the Windows desktop application.

---

# 23. Definition of done

A frontend feature is done only when:

- visual design is final enough for production;
- it uses shared design tokens;
- it follows Windows behavior;
- it uses native or Fluent-consistent icons;
- it is connected to the real backend;
- it uses real data;
- it handles all expected states;
- it handles stable errors;
- it supports retry or recovery;
- it works with keyboard;
- it works with Narrator where applicable;
- it works in light, dark, and High Contrast;
- it works in compact and comfortable density;
- it has tests;
- the capability matrix is updated;
- the API-to-UI map is updated;
- no production mock remains.

---

# 24. Coding-agent execution rules

Any AI coding agent implementing this plan must:

1. Read the complete current backend and frontend before changing code.
2. Read `AGENTS.md`, the master project document, OpenAPI, migrations, event models, and component provisioning documentation.
3. Use Context7 MCP for current library documentation.
4. Follow the existing coding style.
5. Write code comments in English.
6. Implement the setup before the main application.
7. Build each feature as a complete backend-connected vertical slice.
8. Never leave visual-only production components.
9. Never use production mock data.
10. Add or update backend contracts when the frontend requires them.
11. Add tests with each component.
12. Update the capability matrix and API-to-UI map continuously.
13. Re-run backend coverage analysis before each milestone.
14. Stop and fix integration gaps immediately rather than postponing them.
15. Preserve native Windows accessibility and behavior.

---

# Final direction

Ravyn should feel like a native Windows system utility with the power of an advanced download manager.

The required result is:

```text
custom Ravyn setup first
+ no visible classic NSIS/MSI wizard
+ optional installed and portable modes
+ feature-based component selection
+ real provisioning progress
+ deterministic setup-to-app handoff
+ Windows 11 native visual language
+ File Explorer–style structure and density
+ Windows shell icons for files, folders, and drives
+ Fluent icons for application actions
+ Mica with solid accessibility fallback
+ backend-connected vertical slices
+ no production mock data
+ fast large-data handling
+ complete keyboard and screen-reader support
```

Visual polish and backend integration are not separate phases.

Every screen must become beautiful and functional at the same time.
