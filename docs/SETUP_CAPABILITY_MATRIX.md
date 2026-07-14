# Ravyn setup — capability matrix and API-to-UI map

> Phase 0 deliverable required by `RAVYN_FRONTEND_SETUP_DESIGN_PLAN.md` §20–21.
> Status values: `planned` → `connected` → `tested`.
> This file must be updated with every completed setup slice.

## 1. Backend capability matrix (setup scope)

| Backend source | User goal | Exposure | UI representation | Backend dependency | Risk | Status |
|---|---|---|---|---|---|---|
| `GET /v1/components` | See feature catalog, component states, platform support | Primary | Setup feature selection + provisioning screens | `ComponentOverviewResponse` | Low | tested |
| `GET /v1/components/manifest` | Inspect signed catalog source, revision, freshness, and fallback state | Advanced | Components catalog status row | `ManifestRefreshStatus` | Low | tested |
| `POST /v1/components/manifest` | Force an HTTPS conditional refresh and signature/replay validation | Advanced | Components “Refresh catalog” action | `ManifestRefreshStatus` | Medium | tested |
| `POST /v1/components/features` | Persist selected setup profile + features | Primary | Setup type + feature selection stages | `SaveFeatureSelections` | Low | connected |
| `POST /v1/components/{id}/install` | Install a managed component | Primary | Provisioning stage rows, retry action | `InstallComponentRequest` | Medium | connected |
| `POST /v1/components/{id}/cancel` | Cancel a running installation | Primary | Provisioning cancel action | — | Medium | connected |
| `POST /v1/components/{id}/rollback` | Restore previous managed version | Advanced | Components settings (post-setup); setup failure recovery | — | Medium | connected |
| `DELETE /v1/components/{id}` | Remove a managed component | Advanced | Components settings (post-setup) | — | Medium | connected |
| `GET /v1/setup` | Detect first-run / setup-complete / installed state | Primary | Installation detection + welcome stage | `SetupStateResponse` | Low | connected |
| `POST /v1/setup/library` | Validate + create the Ravyn library layout | Primary | Library location stage | `PrepareLibraryRequest` | Medium | connected |
| `POST /v1/setup/complete` | Commit setup completion deterministically | Primary | Completion stage → handoff | `CompleteSetupRequest` | Low | connected |
| `GET /v1/events` (SSE) | Live provisioning progress | Primary | Provisioning stage progress bars | `Event::Component` | Low | connected |
| `GET /v1/settings`, `PATCH /v1/settings` | Persist runtime preferences | Secondary | Preferences stage (subset) | `SettingsResponse` | Low | planned |
| `GET /v1/system/capabilities` | Show engine capability details | Diagnostics | Expandable technical details | — | Low | planned |
| `GET /v1/system/dependencies` | Verify effective engine paths | Diagnostics | Component technical details | — | Low | planned |
| Tauri `setup_installation_info` | Detect install dir, portable vs installed, version | Primary | Welcome stage mode (install/update/repair/first-run) | src-tauri command | Medium | connected |
| Tauri `apply_windows_integration` | Shortcuts, startup, Installed Apps registration | Primary | Preferences stage + install stage | src-tauri command | High | connected |
| Tauri `pick_folder` | Native folder picker | Primary | Library location stage | tauri-plugin-dialog | Low | connected |
| Tauri `finish_setup_handoff` | Deterministic setup → main window transition | Primary | Completion stage | src-tauri command | Medium | connected |

## 2. Component/feature model

Features (`FeatureId`, snake_case on the wire): `standard_downloads` (always on, Ravyn core), `video_extraction` (yt-dlp), `media_merging` (FFmpeg), `torrent_support` (rqbit), `archive_extraction` (7-Zip).

Setup profiles (`SetupProfile`): `minimal`, `recommended`, `full`, `custom`.

Component ids on the wire (`ComponentId`): `ytdlp`, `ffmpeg`, `rqbit`, `seven_zip` (route path accepts `yt-dlp`, `ffmpeg`, `rqbit`, `7zip`).

Component states (`ComponentState`, snake_case): `not_installed`, `queued`, `downloading`, `verifying`, `installing`, `installed`, `update_available`, `failed`, `unsupported`, `cancelled`, `custom_path`, `custom_path_invalid`.

Every state maps to UI text, Fluent icon, accessible description, and permitted actions in
`frontend/src/lib/setup/componentStates.ts`.

## 3. Event contract

SSE `GET /v1/events`, events are JSON `{sequence, type, ...}` with `Last-Event-ID` replay and
`resync_required` on gap. Setup consumes:

| Event `type` | Payload | UI consumer |
|---|---|---|
| `component` | `{component, state, progress_pct?, message?, bytes_downloaded?, bytes_total?}` | Provisioning rows (≤10 Hz coalesced) |
| `resync_required` | `{oldest_available, newest_available}` | Full `GET /v1/components` refetch |

## 4. Setup error matrix

HTTP error body: `{code, message, request_id, retryable, details}`.

| Stable code | Meaning | Ravyn usable? | Retry | Fallback |
|---|---|---|---|---|
| `INVALID` (400) | Bad path / unknown feature | yes | after correction | edit input |
| `NOT_FOUND` (404) | Unknown component | yes | no | refresh overview |
| `CONFLICT` (409) | Custom path configured; cancelled | yes | no | remove custom path |
| `UNAVAILABLE` (503) | Backend not ready / network | yes | yes | wait + retry |
| `INTERNAL_ERROR` (500) | Unexpected | maybe | yes | details + support |
| component `failed` state + `error_message` | download/verify/install failure | yes (optional component) | per-row Retry | continue without feature |
| component `unsupported` state | no verified artifact for platform | yes | no | shown, never hidden |

## 5. API-to-UI map (setup slice)

| Screen / stage | Routes & commands | Events | Service | Store | Components |
|---|---|---|---|---|---|
| Installation detection | Tauri `setup_installation_info`, `GET /v1/setup` | — | `setupService` | `setupStore` | `DetectGate` |
| Welcome | `GET /v1/setup` | — | `setupService` | `setupStore` | `WelcomeStage` |
| Setup type | `GET /v1/components` | — | `componentsService` | `componentsStore` | `SetupTypeStage`, `RadioGroup` |
| Feature selection | `GET /v1/components`, `POST /v1/components/features` | — | `componentsService` | `componentsStore` | `FeatureSelectionStage`, `FeatureRow`, `Checkbox` |
| Library location | `POST /v1/setup/library`, Tauri `pick_folder` | — | `setupService` | `setupStore` | `LibraryStage`, `PathPicker` |
| Preferences | Tauri `apply_windows_integration` (deferred to install), `PATCH /v1/settings` | — | `preferencesService` | `setupStore` | `PreferencesStage`, `Toggle` |
| Installation & provisioning | `POST /v1/components/{id}/install`, `/cancel`, Tauri `apply_windows_integration` | `component` | `provisioningService` | `provisioningStore` | `ProvisioningStage`, `ComponentProgressRow`, `ProgressBar` |
| Completion & handoff | `POST /v1/setup/complete`, Tauri `finish_setup_handoff` | — | `setupService` | `setupStore` | `CompletionStage` |

## 6. Smoke-test status (2026-07-13)

Verified end-to-end on Windows via the real desktop app (`ravyn-desktop.exe`,
debug build, scratch `RAVYN_DATA_DIR`): welcome detection (portable/dev),
setup type, feature selection persisted through `POST /v1/components/features`,
library preparation through `POST /v1/setup/library` (layout created on disk),
Windows integration (Installed Apps registration applied; app copy honestly
skipped in dev), component provisioning with real failure states from the
empty built-in manifest (per-row Retry offered), setup completion via
`POST /v1/setup/complete`, and the setup → main window handoff.

## 7. Known gaps / follow-ups

- `GET /v1/settings` preference subset not yet surfaced in Preferences stage (theme is frontend + Windows; close behavior belongs to main app).
- Development builds can use the embedded catalogue or a signed operator override. Tagged release builds additionally publish and consume the signed remote catalogue described in `docs/COMPONENT_MANIFESTS.md`.
- Uninstall entry point (plan §21 Phase 2 item 12) not yet implemented.
