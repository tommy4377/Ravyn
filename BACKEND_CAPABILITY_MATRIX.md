# Backend Capability Matrix

Last reconciled with source: 2026-07-14

**Frontend status**: `tested`, `connected`, `partial`, or `deferred`.
**Backend status**: `complete`, `partial`, or `release-blocked`.

## Primary product workflows

| Domain | Backend coverage | Frontend coverage | Frontend status | Backend status |
|---|---|---|---|---|
| Downloads | CRUD, pause/resume/cancel/retry, bulk actions, outputs, segments, actions, logs, SSE | Virtualized workspace, command bar, drag/drop, keyboard, selection, standardized details | tested | complete |
| Add/import | Direct, media, torrent, Metalink, bounded batch JSON/text, duplicate handling | Source-first Add plus dedicated Metalink and batch dialogs | tested | complete |
| Library | Browse, trash/purge, restore, duplicates, tags, import/status/cancel, verify, moved-file repair, transactional root move, cleanup, statistics | Files/Trash/Duplicates, import controls, cancellation, moved-file repair, root-move preflight/progress/restart, statistics | tested | complete in source; native relocation E2E required |
| Media | Probe, archive, item summary/list/outputs/retry | Downloads/Archive and shared details | connected | complete; requires yt-dlp/FFmpeg as appropriate |
| Torrents | Probe, managed/engine lists, details/stats/peers/files, seeding, remove, DHT | Table, file tree, peers, trackers, engine details, explicit removal | connected | complete; requires rqbit |
| Basket | Add/edit/delete/reorder/clear/start | Batch queue drawer | connected | complete |
| Automation | Rules CRUD/preview, schedules CRUD/run/enable, executions/cancel | Visual rules, readable schedules, preview, structured history | tested | complete |
| Components | Status, selection, install/update/verify/rollback/cancel/remove/cleanup | Settings → Tools and setup | connected | complete for yt-dlp/FFmpeg/rqbit; 7-Zip custom/system only |
| Settings | Read/validate/patch/reset, presets, profiles, activation | Categorized Settings with dirty/restart state | connected | complete |
| Diagnostics | Readiness, database/backup/restore, hosts, audit, dependencies, capabilities, maintenance | Troubleshooting plus advanced diagnostics | connected | complete |
| Setup | Library preparation, integration consent, installation report, completion | Full setup and installed-copy handoff | tested | complete |

## Reliability and release integration

| Capability | Source implementation | Status |
|---|---|---|
| Remote component catalog | HTTPS conditional refresh, Ed25519, bounded reads, expiry/replay protection, transactional cache and LKG | complete in source; deployment keys required |
| Component transactions | Staging, checksums, extraction bounds, health checks, cancellation, rollback, cleanup | complete in source |
| Library folder import | Bounded, symlink-safe, warning-tolerant, truncation-aware, cooperatively cancellable, audited | complete in source |
| Physical Library-root move | Durable SQLite journal, disk-space preflight, conflict policy, verified copy, cooperative cancellation, blocked job dispatch, restart recovery/finalization, Trash-path preservation, and rollback | complete in source; native filesystem/power-loss E2E required |
| Installed-app update | Signed streaming stage, install-on-close, transaction journal, readiness marker, binary/registry/shortcut rollback, repair, startup recovery | complete in source; clean-machine E2E required |
| Installer/release | NSIS/MSI/portable workflow, smoke tests, signing verification, checksums, SBOM and attestations | complete in source/CI; credentials and successful production run required |

## Deferred product scope

| API group | Status |
|---|---|
| Browser token/page/sniff/import routes | backend available; browser extension deliberately deferred |
| Per-monitor wallpaper selection | native bridge enhancement pending |
| Managed 7-Zip provisioning | deliberately deferred; custom/system executable supported |
