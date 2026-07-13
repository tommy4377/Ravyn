# Backend Capability Matrix

> Companion to `DESIGN_PLAN(2).md` §26.1. Generated from `src/api/routes.rs` and the per-domain route modules under `src/api/routes/`. Update this file whenever a route, event, or frontend connection changes.
>
> **Frontend status** legend: `planned` (no client code yet), `connected` (typed client/service exists and a screen calls it), `tested` (connected + covered by frontend tests).
> **Backend status** legend: `complete`, `partial`, `release-blocked` (works, but blocked by a release-engineering gap tracked in `TODO.md`).

## Jobs (Downloads)

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `GET /v1/jobs` | List/search/filter downloads | Primary | Downloads list (virtualized) | Low | tested | complete |
| `POST /v1/jobs` | Add a single download | Primary | Add Download dialog | Medium | tested | complete |
| `POST /v1/jobs/metalink` | Add a download from a Metalink document | Primary | — | Medium | planned | complete |
| `POST /v1/jobs/batch` | Bulk-create jobs from a list of specs | Secondary | — | Medium | planned | complete |
| `POST /v1/jobs/import-text` | Add multiple downloads from pasted URLs | Primary | Add Download dialog (multi-line) | Medium | tested | complete |
| `GET/PATCH/DELETE /v1/jobs/{id}` | Read, edit, remove a download | Primary | Downloads list, details pane, row/command-bar actions | Medium/High (DELETE) | tested | complete |
| `POST /v1/jobs/{id}/pause` | Pause a transfer | Primary | Row/command-bar action | Low | tested | complete |
| `POST /v1/jobs/{id}/resume` | Resume a paused/failed transfer | Primary | Row/command-bar action | Low | tested | complete |
| `POST /v1/jobs/{id}/cancel` | Stop a transfer, keep partial data | Primary | Row/command-bar action | Medium | tested | complete |
| `POST /v1/jobs/{id}/retry` | Requeue a failed/cancelled/partial job | Primary | Row/command-bar action | Low | tested | complete |
| `POST /v1/jobs/actions` | Bulk pause/resume/cancel/retry/delete | Primary | Command-bar bulk actions | High (empty `ids` = all jobs; frontend never sends empty) | tested | complete |
| `GET /v1/jobs/{id}/outputs` | List produced files for a job | Secondary | Details pane → Outputs tab | Low | connected | complete |
| `GET /v1/jobs/{id}/segments` | Inspect segmented-download internals | Diagnostics | Details pane → Advanced tab (summary only) | Low | connected | complete |
| `GET /v1/jobs/{id}/actions` | Inspect post-processing action history | Diagnostics | Details pane → Activity tab | Low | connected | complete |
| `GET /v1/jobs/{id}/logs` | Inspect job-scoped log entries | Diagnostics | Details pane → Activity tab | Low | connected | complete |
| `GET /v1/jobs/{id}/trust`, `/tags`, media-item routes | Torrent/media-specific job data | Secondary/Advanced | — | Medium | planned | complete |

## Media

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `POST /v1/media/probe` | Inspect a video/playlist URL before downloading | Primary | Media probe & format picker (Phase 4) | Medium | planned | complete (gated by yt-dlp component) |
| `GET /v1/jobs/{id}/media-items*`, `/media-summary` | Track per-item media job progress | Primary | Media job details (Phase 4) | Medium | planned | complete |
| `POST /v1/media/archive` (removal) | Manage media archive de-duplication records | Secondary | Library/Media settings (Phase 6) | Low | planned | complete |

## Torrents

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `POST /v1/torrents/probe` | Inspect a magnet/torrent before downloading | Primary | Torrent probe & file-tree picker (Phase 4) | Medium | planned | complete (gated by rqbit component) |
| `GET /v1/torrents`, `/{id}`, `/{id}/stats`, `/{id}/peers`, `/{id}/files` | Monitor an active torrent | Primary | Torrent details (Phase 4) | Medium | planned | complete |
| `POST /v1/torrents/{id}/seeding`, `/remove` | Control seeding, remove a torrent | Primary | Torrent details actions (Phase 4) | High (`remove` can delete files) | planned | complete |
| `GET /v1/torrents/engine`, `/engine/stats`, `/dht/*` | Engine/DHT diagnostics | Diagnostics | Diagnostics → Network (Phase 7) | Low | planned | complete |

## Library

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `GET /v1/library`, `/library/{id}` | Browse the persistent file catalog | Primary | Library list (Phase 3) | Low | planned | complete |
| `GET /v1/library/duplicates` | Find duplicate files | Secondary | Library duplicates view (Phase 3) | Low | planned | complete |
| `DELETE /v1/library/{id}`, `POST /v1/library/{id}/restore` | Move to Trash / restore | Primary | Library Trash flow (Phase 3) | High | planned | complete |
| `POST /v1/library/import`, `GET /v1/library/import` | Import existing files into the library | Primary | Library import flow (Phase 3) | Medium | planned | complete |
| `POST /v1/library/verify` | Verify library entries against disk | Secondary | Library verify flow (Phase 3) | Low | planned | complete |
| `POST /v1/library/relocate` | Repair moved-file references | Secondary | Library relocate flow (Phase 3) | Medium | planned | complete |
| `POST /v1/templates/preview` | Preview a naming/destination template | Secondary | Presets/Settings (Phase 5/6) | Low | planned | complete |
| `GET/POST /v1/basket`, `/basket/{id}`, `/basket/reorder` | Stage downloads before creating jobs | Primary | Basket (Phase 5) | Low | planned | complete |
| `GET/POST/PUT/DELETE /v1/presets`, `/presets/{id}` | Manage reusable job option presets | Secondary | Presets (Phase 5) | Low | planned | complete |
| `GET/POST/PUT/DELETE /v1/profiles*` | Manage broader operating profiles | Secondary | Profiles (Phase 5) | Medium | planned | complete |
| `GET/POST/DELETE /v1/tags*` | Manage reusable tags | Secondary | Tags (Phase 5); read today via `job.options_json.tags` only | Low | partial (read-only via Job) | complete |
| `GET/POST/PATCH/DELETE /v1/pages*` | Browser page-resource capture | Advanced | Browser capture (Phase 5) | Low | planned | complete |

## Automation (Rules, Schedules, Browser)

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `GET/POST/PUT/DELETE /v1/rules`, `/rules/preview` | Automate job handling with WHEN/THEN rules | Secondary | Rules builder (Phase 5) | Medium | planned | complete |
| `GET/POST/PATCH/DELETE /v1/schedules*`, executions, run-now | Schedule downloads | Secondary | Schedules (Phase 5) | Medium | planned | complete |
| `POST /v1/browser/*` (tokens, sniff, import) | Browser-extension capture workflows | Advanced | Browser capture (Phase 5) | Medium | planned | partial (product-readiness noted in plan §5) |

## Components (managed engines)

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `GET /v1/components` | View managed-engine status | Primary | Setup (connected); Components page (Phase 6) | Low | connected (setup only) | complete |
| `POST /v1/components/features` | Choose which features/engines are enabled | Primary | Setup (connected); Components page (Phase 6) | Medium | connected (setup only) | complete |
| `POST /v1/components/{id}/install`, `/update`, `/verify`, `/rollback`, `/cancel`, `DELETE` | Manage a single component's lifecycle | Primary | Setup (connected, install/cancel only); Components page (Phase 6) | Medium/High | partial (install/cancel only) | complete, except 7-Zip has no verified artifact (see `TODO.md`) |

## Setup

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `GET /v1/setup`, `POST /v1/setup/library`, `/complete` | First-run setup flow | Primary | Setup app (connected) | Medium | tested | complete |
| `POST /v1/setup/installation` | Report Windows install results | Primary | Setup app | Medium | connected | complete |

## System (Settings, Diagnostics, Maintenance)

| Backend source | User goal | Exposure | UI representation | Risk | Frontend status | Backend status |
|---|---|---|---|---|---|---|
| `GET/PATCH /v1/settings*` | Configure application behavior | Primary | Settings (Phase 6) | Medium | planned | complete |
| `GET/POST /v1/secrets*` | Manage credential references | Advanced | Settings → Network (Phase 6) | High | planned | complete |
| `GET /v1/system/hosts`, `/dependencies`, `/capabilities` | Host profiles and environment diagnostics | Secondary/Diagnostics | Settings/Diagnostics (Phase 6/7) | Low | planned | complete |
| `GET /v1/system/database*`, `/cleanup*`, `/maintenance` | Backup, restore, cleanup, maintenance | Diagnostics | Diagnostics (Phase 7) | High (restore/cleanup) | planned | complete |
| `GET /v1/statistics` | Personal usage statistics | Secondary | Library/Diagnostics (Phase 3/7) | Low | planned | complete |
| `GET /v1/audit*` | Audit log and hash-chain verification | Diagnostics | Diagnostics → Audit (Phase 7) | Low | planned | complete |
| `GET /v1/events` (SSE) | Live backend state (job/progress/component/queue/resync) | Primary | Global event client → normalized stores | Low | tested | complete |
| `GET /health`, `/health/live`, `/health/ready`, `/metrics`, `/openapi.json` | Operational/diagnostic endpoints | Diagnostics | Diagnostics → Overview (Phase 7) | Low | planned | complete |

## Notes

- "Frontend status: tested" means covered by `frontend/src/**/*.test.ts`, not end-to-end browser tests (see `DESKTOP_RELEASE_CHECKLIST.md` for the still-pending Windows E2E pass).
- Every "planned" row already has a concrete backend contract; nothing here is speculative. Phase numbers reference `DESIGN_PLAN(2).md` §24.
