# Backend Capability Matrix

Last reconciled with source: 2026-07-14

**Frontend status**: `tested`, `connected`, `partial`, or `api-only`.
**Backend status**: `complete`, `partial`, or `release-blocked`.

## Primary product workflows

| Domain | Backend coverage | Frontend coverage | Frontend status | Backend status |
|---|---|---|---|---|
| Downloads | List/create/update/delete, pause/resume/cancel/retry, bulk actions, outputs, segments, actions, logs, SSE | Virtualized Downloads workspace, add flows, row/bulk actions, details tabs | tested | complete |
| Library | Browse, trash/purge, restore, import, verify, relocate, duplicates, cleanup policies/run, statistics | Library browser and maintenance flows | connected | complete |
| Media | Probe, archive records, per-item summary/list/retry | Media probe, archive, and job-oriented controls | connected | complete; requires yt-dlp/FFmpeg as appropriate |
| Torrents | Probe, list/details/stats/peers/files, seeding, remove, engine/DHT stats | Torrents workspace and add flow | connected | complete; requires rqbit |
| Basket | Add/edit/delete/reorder/clear/start | Basket workspace | connected | complete |
| Automation | Rules CRUD, schedules CRUD/run/enable, executions/cancel | Automation workspace | connected | complete |
| Components | Status, feature selection, install/update/verify/rollback/cancel/remove/cleanup | Setup and full Components screen | connected | complete for yt-dlp/FFmpeg/rqbit; 7-Zip uses custom/system path |
| Settings | Read/validate/patch/reset, presets, profiles, activation | Appearance, storage, performance, network, executable paths, updater, presets/profiles | connected | complete |
| Diagnostics | Readiness, database/backup/restore, hosts, audit, dependencies, capabilities, maintenance, statistics | Diagnostics workspace | connected | complete |
| Setup | Library preparation, installation report, completion validation | Full setup app and installed-copy handoff | tested | complete |

## Component delivery and desktop reliability

| Capability | Source implementation | UI / release integration | Status |
|---|---|---|---|
| Embedded component catalog | Validated built-in stable manifest | Setup and Components | complete |
| Remote signed component catalog | HTTPS-only refresh, ETag/Last-Modified, size bound, Ed25519, expiry, replay/downgrade rejection, transactional cache, LKG | Components status/refresh and tagged release workflow | complete in source; deployment keys required |
| Managed component transactions | Staging, checksum, archive member extraction, version/capability health, rollback, cleanup | Components actions and setup progress | complete for yt-dlp/FFmpeg/rqbit |
| 7-Zip | Functional custom-path health probe | File picker and guidance in Settings/Components | complete custom/system path; managed provisioning deferred |
| Installed-app update | Signed manifest, streamed staging, install-on-close | Passive Settings status | complete in source |
| Update readiness/rollback | Durable transaction, webview/backend marker, retained binary, timeout/crash rollback, persisted result | Result displayed in Settings | complete for main binary; full repair still partial |
| Desktop installer | NSIS/MSI/portable build and process smoke tests | Release workflow | complete in source/CI; Authenticode open |

## Secondary and advanced surfaces

| API group | Current frontend exposure | Status |
|---|---|---|
| Metalink creation and large batch creation | Core import exists; dedicated advanced UX not complete | partial |
| Tags and template preview | Some data is visible through jobs/presets; dedicated management/preview UX open | partial |
| Rule preview | Rule CRUD is connected; preview UI is not exposed | api-only |
| Browser page/sniff/import routes | Browser extension intentionally excluded from current pass | api-only / product-deferred |
| Secrets | Create/replace/delete/list UI, credential-store persistence, type validation, and rqbit binding | connected |
| Deep DHT/host diagnostic tables | Summary diagnostics connected; every low-level table is not exposed | partial |

## Release blockers outside the backend core

- Full Windows Rust/Tauri compile and native runtime validation.
- Production signing credentials and a successful tagged release run; the CI signing and verification path is implemented.
- Full installed-file repair and clean-machine N-to-N+1 updater rollback E2E.
- Per-monitor different-wallpaper selection.
