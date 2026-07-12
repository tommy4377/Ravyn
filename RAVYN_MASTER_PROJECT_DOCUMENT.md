# Ravyn — Master Project Status, Architecture, Audit, and Roadmap

> **Canonical project document**
>
> This file replaces the previous phase reports, static audits, incremental progress reports, duplicated status documents, and fragmented roadmaps.
>
> Keep this document updated whenever the backend changes. Historical phase files may be removed after confirming that this document is committed to the repository.
>
> **Scope:** Ravyn backend only. The desktop frontend and browser extensions are intentionally excluded except where backend contracts exist to support them.

---

# 1. Executive summary

Ravyn is a backend-only download manager written in Rust. It combines:

- a resumable and segmented HTTP download engine;
- yt-dlp media support;
- rqbit BitTorrent support;
- persistent queue and automation;
- rules, tags, schedules, and monitored pages;
- checksums and post-processing;
- a secured local API;
- browser-scoped integration endpoints;
- operational diagnostics, backups, audit records, metrics, and event streaming.

The project is no longer a prototype. It is an **advanced-alpha backend approaching beta**, with most core download-manager capabilities already implemented.

Current estimated maturity:

| Area | Estimated status |
|---|---:|
| Core functional coverage | 88–92% |
| Public beta readiness | 75–80% |
| Production-grade readiness | 55–65% |
| Frontend readiness of API surface | High, but not final |
| Operational hardening | Partial |
| Advanced transfer optimization | Partial |

The remaining work is primarily about:

```text
production observability
+ process isolation
+ complete external-tool policies
+ time-zone correctness
+ destructive and long-running tests
+ advanced bandwidth scheduling
+ advanced multi-source performance
+ reproducible GitHub releases with checksums and attestations
```

---

# 2. Current repository snapshot

This section describes the latest consolidated backend snapshot after Increment 4.

| Item | Current value |
|---|---:|
| Rust source and Rust test lines | approximately 22,244 |
| Rust source files | 76 (largest file 910 lines after the de-monolith refactor) |
| Database migrations | 16 |
| Database tables after migrations | 23 |
| Axum/OpenAPI operations | 90 / 90 |
| Rust test attributes present | approximately 99 (95 unit/property + 4 integration) |
| Runtime `todo!`, `unimplemented!`, or application `panic!` | 0 found by static scan |
| Latest SQLite integrity check | `ok` |
| Latest foreign-key violations | 0 |

External runtime programs:

- yt-dlp;
- rqbit;
- FFmpeg;
- 7-Zip;
- optional image converter such as ImageMagick.

Primary implementation stack:

- Rust;
- Tokio;
- Axum;
- SQLx;
- SQLite;
- Reqwest;
- Serde;
- Tower and tower-http;
- keyring;
- tracing;
- rqbit HTTP API;
- yt-dlp structured CLI output.

---

# 3. Repository architecture

## 3.1 Main modules

After the 2026-07-12 de-monolith refactor, every source file is below 1,000 lines. Multi-file types (`Repository`, `JobManager`) follow the multiple-`impl`-block pattern: the struct lives in one file and each sibling module contributes a cohesive `impl` block.

```text
src/
├── adapters/
│   ├── media.rs            (types + adapter; yt-dlp plumbing in media/ytdlp.rs)
│   ├── media/ytdlp.rs
│   ├── torrent.rs          (types + adapter; payload normalization in torrent/wire.rs)
│   └── torrent/wire.rs
├── api/
│   ├── routes.rs           (state, router, audit helpers)
│   ├── routes/{system,jobs,media,torrents,automation,browser}.rs
│   ├── openapi.rs          (document assembly + parity test)
│   ├── openapi/{operations,components}.rs
│   └── pagination.rs
├── core/
│   ├── manager.rs          (JobManager state + constructor)
│   ├── lifecycle.rs        (create/pause/resume/cancel/retry/delete)
│   ├── dispatcher.rs       (workers, dispatch loop, shutdown)
│   ├── execution.rs        (engine run, checksum, post-processing phases)
│   ├── bulk.rs             (sniff, text imports, batches)
│   ├── automation.rs       (rules, tokens, schedules, validation)
│   ├── maintenance.rs      (backup/restore control)
│   ├── media_control.rs / torrent_control.rs
│   ├── models.rs / events.rs / progress.rs / rate_limit.rs / metrics.rs
├── download/
│   ├── adapter.rs / http.rs / planner.rs / probe.rs / segmented.rs
├── postprocess/
│   └── pipeline.rs
├── services/
│   ├── browser.rs / checksum.rs / cron.rs / dedup.rs / filename.rs
│   ├── imports.rs / process.rs / rules.rs / scheduler.rs / schedules.rs
│   └── secrets.rs / security.rs / sniffer.rs
└── storage/
    ├── repository.rs       (shared SQLite handle + connect)
    ├── jobs.rs / outputs.rs / schedules.rs / audit.rs / secrets.rs
    ├── settings.rs / backup.rs / automation.rs / media.rs
    ├── pagination.rs / recovery.rs / segments.rs / host_profiles.rs
    ├── torrent_policy.rs
    └── repository_tests.rs (cross-module storage tests)
```

## 3.2 Architectural responsibilities

### Core manager

The manager coordinates:

- queue dispatch;
- adapter selection;
- job lifecycle;
- pause, resume, retry, cancel, and delete;
- cancellation tokens;
- progress persistence;
- checksum verification;
- post-processing;
- output registration;
- event publication;
- reconciliation after restart.

### Storage layer

SQLite is the durable source of truth for:

- jobs;
- segments;
- outputs;
- tags;
- rules;
- schedules;
- schedule executions;
- idempotency;
- host profiles;
- torrent policy state;
- media items and media archive;
- browser tokens;
- monitored-page history;
- audit records;
- logs;
- settings;
- secret references;
- post-action journal.

### Adapter layer

Adapters isolate external engines:

- HTTP engine is implemented directly in Ravyn;
- yt-dlp is used for media extraction and downloads;
- rqbit is used for torrent lifecycle and data transfer;
- FFmpeg and image converters handle conversion;
- 7-Zip handles archive extraction.

### API layer

The Axum API provides:

- management endpoints;
- diagnostics;
- browser-scoped endpoints;
- SSE events;
- health and readiness;
- OpenMetrics;
- OpenAPI;
- structured errors;
- request IDs;
- pagination;
- rate limiting;
- concurrency limiting;
- request timeouts.

---

# 4. Completed backend capabilities

## 4.1 HTTP download engine

Implemented:

- single-stream HTTP downloads;
- segmented HTTP downloads;
- strict `Content-Range` validation;
- persisted segment state;
- resumable partial downloads;
- safe sequential fallback;
- detection of changed ETag, Last-Modified, final URL, and size;
- invalidation of stale resume data;
- bounded exponential retry;
- cancellation-aware reads and writes;
- file synchronization before durable progress updates;
- per-job bandwidth limiting;
- global bandwidth limiting;
- per-host concurrency limiting;
- persistent host performance profiles;
- host circuit breakers;
- deferred job availability after temporary circuit opening;
- dynamic distribution of persistent work units;
- HTTP mirror failover;
- DNS pinning;
- redirect-by-redirect SSRF validation;
- localhost and private-network blocking by default;
- controlled opt-in for private networks;
- remote identity and partial-file reconciliation;
- graceful pause, cancel, retry, and recovery.

### Current limitation

The current “dynamic work stealing” is dynamic work-unit distribution. It does not yet duplicate or split a range already assigned to a slow worker.

True speculative tail downloading remains future work.

## 4.2 Persistent queue and job lifecycle

Implemented:

- durable SQLite queue;
- priorities;
- queued availability time;
- pause;
- resume;
- cancel;
- retry;
- delete;
- partial completion;
- global and batch operations;
- safe state-aware job editing;
- tags;
- duplicate prevention;
- recovery on restart;
- renewable schedule leases;
- durable cancellation state;
- supervised cancellation for long-running job phases;
- idempotent creation for supported routes.

Job states include, where applicable:

```text
queued
deferred
probing
downloading
paused
verifying
post-processing
completed
partial
failed
cancelled
seeding
```

## 4.3 Output artifact model

Implemented:

- first-class `job_outputs`;
- output type and source type;
- output state;
- original path and current path;
- relative path;
- size;
- MIME field;
- checksum fields;
- parent-output lineage;
- post-action index;
- JSON metadata;
- HTTP output registration;
- media output registration;
- selected torrent-file output registration;
- derived output registration for conversion and extraction;
- path updates after move;
- deleted, moved, replaced, and ready states;
- output diagnostics API;
- auxiliary media-output links.

Current output roles include:

```text
primary
video
audio
subtitle
thumbnail
metadata
description
chapter
auxiliary
torrent_file
extracted_file
converted_file
archive
directory
other
```

## 4.4 Checksums and verification

Implemented:

- SHA-256 verification;
- expected checksum validation;
- cancellation-aware hashing;
- persisted checksum metadata on outputs;
- checksum post-action;
- final-size validation;
- safe status handling during verification.

### Partial

Incremental hashing and piece-level verification are not yet implemented.

## 4.5 Post-processing

Implemented:

- durable post-action journal;
- cancellation-aware execution;
- restart recovery;
- FFmpeg conversion;
- ten validated named FFmpeg presets for video, audio, and images;
- local-file-only FFmpeg input protocol policy by default;
- arbitrary FFmpeg arguments require per-action and process-wide unsafe opt-in;
- six-hour external-process safety timeout;
- audio/video conversion;
- AVIF fallback through a dedicated image converter;
- staged archive extraction;
- bounded extraction controls;
- move;
- cross-device move fallback;
- open-after-completion;
- keep/delete original policy;
- derived-output lineage;
- child-process termination and `kill_on_drop`;
- shared external-process supervisor;
- Windows Job Objects with kill-on-close, CPU-time, and memory quotas;
- Unix process groups with CPU-time and address-space limits;
- bounded stdout/stderr capture and draining;
- wall-clock and output-file size enforcement;
- path confinement;
- traversal protection;
- Windows filename hardening.

### Partial

Full OS sandboxing and Windows restricted process tokens are not yet complete.

## 4.6 Media and yt-dlp

Implemented:

- structured yt-dlp probing;
- structured progress parsing;
- `--ignore-config`;
- capability probing through `--version` and `--help`;
- early rejection of incompatible yt-dlp contracts;
- bounded and redacted probe errors;
- capability cache;
- FFmpeg path integration;
- persistent media items;
- persistent media archive;
- archive-based deduplication;
- partial playlist completion;
- per-item status;
- per-item error;
- selective item retry;
- retry of failed items;
- parent playlist summary;
- recovery from `partial` to `completed`;
- auxiliary outputs;
- subtitles;
- thumbnails;
- metadata files;
- descriptions;
- compact stored media metadata;
- links from retry outputs to original playlist items.

### Partial

Playlist items are durable child entities, but not every playlist item is always executed as a fully independent queue job.

A formal supported-version range and controlled yt-dlp update/rollback system remain incomplete.

## 4.7 BitTorrent and rqbit

Implemented:

- magnet and torrent submission;
- local torrent-file validation;
- torrent size limits;
- rqbit capability inspection;
- torrent lifecycle;
- pause;
- start/resume;
- delete;
- forget;
- file selection;
- file listing;
- peer information;
- global statistics;
- torrent statistics;
- selected output registration;
- managed-torrent listing;
- seeding state persistence;
- minimum seeding duration;
- maximum seeding duration;
- maximum seed ratio;
- automatic stop reasons;
- missing-engine reconciliation;
- restricted engine ID validation;
- typed normalized contracts for stable operations;
- retained raw upstream payloads for compatibility.

### Partial

Remaining raw rqbit areas include unstable DHT and diagnostic envelopes.

Per-torrent and global runtime upload caps remain incomplete until a verified rqbit contract is selected.

Full compatibility testing across rqbit versions remains incomplete.

## 4.8 Rules, tags, and organization

Implemented:

- rule CRUD;
- tag CRUD;
- job-tag management;
- priority-ordered rule evaluation;
- domain matching;
- MIME matching;
- extension matching;
- scalar first-wins behavior;
- additive tags;
- additive post-actions;
- rule validation;
- safe destination validation;
- filename sanitization;
- rule execution during job creation and probing;
- filtering and pagination for supported collections.

### Partial

A dedicated rule-preview and conflict-explanation API can still be improved.

Saved organizational views and richer rule grouping remain future enhancements.

## 4.9 Scheduler and automation

Implemented:

- one-shot schedules;
- interval schedules;
- five-field cron;
- six-field cron with seconds;
- lists, ranges, steps, month names, and weekday names;
- atomic SQLite claims;
- renewable leases;
- execution history;
- run-now;
- enable/disable;
- execution cancellation state;
- page sniffing schedules;
- “only new” page-resource imports;
- backoff;
- schedule failure state;
- fixed UTC offsets;
- temporary pause;
- overlap policies:
  - `skip`;
  - `queue`;
  - `replace`;
  - `allow_parallel`;
- missed-run policies:
  - `skip`;
  - `run_once`;
  - `catch_up`;
- bounded catch-up;
- grace window for normal scheduling delay.

### Partial

Named IANA time zones and daylight-saving transitions are not yet implemented.

Execution cancellation and replacement should receive more destructive concurrency testing.

## 4.10 Page scanner and browser backend

Implemented backend functionality:

- static HTML scanning;
- relative URL resolution;
- `<base>` handling;
- HTML entity handling;
- resource classification;
- image, media, link, script, stylesheet, object, embed, source, and track discovery;
- extension filters;
- page-size and resource-count limits;
- monitored-page history;
- “only new” resource behavior;
- page and resource APIs;
- browser-scoped tokens;
- token hashing;
- origin allow-lists;
- restricted browser import DTO;
- browser token revocation;
- scoped browser endpoints;
- CORS origin handling;
- `Vary: Origin`;
- rate-limit identities per browser token.

The actual Firefox and Chrome extensions are outside this backend document.

## 4.11 Security

Implemented:

- loopback-only default listener;
- explicit remote API opt-in;
- mandatory bearer token for remote access;
- mandatory trusted TLS reverse-proxy mode for non-loopback binding;
- constant-time token comparison;
- scoped browser tokens;
- origin validation;
- DNS pinning;
- redirect validation;
- private-address blocking;
- output path confinement;
- symlink-escape checks through canonical ancestors;
- local torrent-file checks;
- path traversal prevention;
- secret references;
- platform keyring storage;
- audited secret lifecycle;
- public error redaction;
- sensitive-option redaction in API output;
- bounded body size;
- request timeout;
- concurrency limit;
- load shedding;
- rate limiting;
- rate-limit identity by admin token, browser token, or client IP;
- secure reverse-proxy requirement;
- audit log.

### Partial

Secret references are integrated into key adapters, but all future credential-bearing options must continue to use references rather than plaintext.

Native TLS and mTLS are not implemented directly in Ravyn.

Full external-process sandboxing is incomplete.

## 4.12 API and client contract

Implemented:

- REST API;
- structured request and response DTOs;
- structured machine-readable errors;
- stable error codes;
- retryable flag;
- request IDs;
- cursor pagination;
- pagination limits;
- filtering for major collections;
- per-item batch results;
- OpenAPI 3.1 document;
- exact router/OpenAPI operation parity in the latest static check;
- capability endpoint;
- SSE;
- sequenced events;
- replay through `Last-Event-ID`;
- health;
- liveness;
- readiness;
- metrics;
- browser-scoped APIs;
- database maintenance APIs;
- backup APIs;
- staged restore APIs.

## 4.13 Database, recovery, and maintenance

Implemented:

- SQLite WAL;
- foreign keys;
- busy timeout;
- ordered migrations;
- 16 migrations;
- atomic job claims;
- schedule leases;
- online backup;
- backup listing;
- backup verification;
- staged restart-time restore;
- restore state machine;
- WAL/SHM preservation;
- restore rollback;
- crash-resumable restore markers;
- integrity checks;
- retention for selected logs, executions, audit records, and idempotency records;
- runtime settings;
- secret-reference metadata;
- audit records;
- job logs.

### Partial

More schema `CHECK` constraints and broader automated maintenance policies can still be added.

Full restore fault-injection coverage remains incomplete.

## 4.14 Observability

Implemented:

- tracing;
- request IDs;
- structured API errors;
- job logs;
- audit records;
- liveness;
- readiness;
- OpenMetrics endpoint;
- job counts by state;
- active jobs;
- queue depth;
- transferred bytes;
- output counts;
- failed job counts;
- capability reporting;
- dependency health reporting.

### Status

The Section 7.6 metrics backlog is complete as of 2026-07-12: work-unit duration, SQLite query latency, SQLite busy errors, seeding-policy outcomes, DNS duration, free disk space, and temporary disk usage are implemented with bounded label cardinality. Deeper per-mirror and per-piece telemetry is deferred to the advanced transfer priorities.

---

# 5. Development history consolidated

## Phase 1 — HTTP foundation

Delivered:

- SQLite queue;
- HTTP downloads;
- segmentation;
- pause/resume/cancel/retry;
- rules;
- schedules;
- checksums;
- post-processing foundation.

## Phase 2 — Media

Delivered:

- yt-dlp adapter;
- FFmpeg;
- structured media progress;
- media probing;
- format selection and post-processing integration.

## Phase 3 — Torrent

Delivered:

- rqbit adapter;
- magnet and torrent lifecycle;
- file selection;
- torrent statistics;
- managed torrent monitoring.

## Phase 4 — Automation and browser backend

Delivered:

- batch imports;
- text imports;
- HTML scanning;
- browser token bridge;
- page-resource history;
- rules, tags, and schedule CRUD;
- scheduled page monitoring;
- cron support.

## Phase 5 — Adaptive HTTP performance

Delivered:

- persistent host profiles;
- circuit breaker;
- delayed job availability;
- dynamic work-unit distribution;
- range-specific penalties;
- host diagnostics;
- AVIF fallback.

## Stabilization and Increment 1

Delivered:

- correction of main job cancellation lifecycle;
- durable progress path improvements;
- output lineage;
- output MIME/checksum fields;
- derived outputs;
- actual secret retrieval;
- secret binding for HTTP, media, and rqbit;
- Rust 1.85 compatibility correction for keyring;
- compiler and Clippy checks at that point.

## Increment 2

Delivered:

- reusable pagination;
- route-complete OpenAPI;
- staged database restore;
- crash-safe restore state machine;
- WAL/SHM bundle preservation;
- broader audit coverage;
- pagination for more diagnostics.

## Increment 3

Delivered:

- media playlist items;
- media archive;
- partial media jobs;
- selective retry;
- retry-failed batch;
- torrent seeding policies;
- typed rqbit foundations;
- API timeout;
- concurrency protection;
- rate limiting;
- public error redaction.

## Increment 4

Delivered:

- yt-dlp capability enforcement;
- first-class auxiliary media outputs;
- normalized rqbit contracts;
- per-token and per-client rate limiting;
- broader persistent settings;
- correct settings reset baseline;
- live global bandwidth limit update;
- scheduler overlap and missed-run policies;
- fixed UTC offsets;
- temporary schedule pause;
- updated OpenAPI parity.

---

# 6. Validation history and confidence

This section must be preserved so future contributors do not confuse static verification with compiler verification.

## 6.1 Earlier verified snapshot

At earlier stabilization stages, the project successfully passed combinations of:

```text
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

One hardened snapshot passed 22 tests.

Increment 1 later passed:

```text
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo metadata --locked --no-deps
```

## 6.2 Latest snapshot

The latest Increment 4 snapshot was **not compiled with Cargo**, following an explicit instruction to avoid long Cargo runs in the available runtime.

The latest snapshot received fast checks only:

- Rust syntax parsing;
- direct rustfmt;
- migration application;
- SQLite integrity check;
- foreign-key check;
- router/OpenAPI parity;
- route duplication scan;
- placeholder scan;
- ZIP integrity verification.

Latest static results:

```text
Rust files analyzed:                 49
Rust files with syntax errors:        0
Migrations applied:                  16
SQLite integrity_check:              ok
Foreign-key violations:               0
Axum/OpenAPI operations:           90/90
Missing or duplicate routes:           0
Runtime todo/unimplemented/panic:      0
```

## 6.3 Important confidence statement

The latest source is:

```text
statically inspected
+ migration-tested
+ formatted
+ packaged
```

but it must **not** be described as compiler-tested until the current Increment 4 code passes Cargo checks in CI or a suitable local environment.

Before a beta release, run:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

## 6.4 Latest compiler-verified snapshot (2026-07-12, Priority 1 completion)

- `Cargo.lock` was missing from the repository and was generated and committed on this pass; every later command used `--locked`;
- `cargo fmt --all -- --check` passed;
- `cargo check --all-targets` passed;
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
- `cargo test --locked --all-targets` passed: 87 unit/property tests and 3 HTTP integration tests, 0 failures.

## 6.5 Latest compiler-verified snapshot (2026-07-12, Priority 5 second pass)

- new fault-injection tests: SQLite busy counting, scheduler lease loss and reclaim, 8-way single-owner claims, overlap replacement, idempotency replay/conflict, an end-to-end redirect-loop failure, and three restore fault fixtures (staged corruption, post-apply open failure, orphaned staged database);
- one real restore-recovery bug was found by the corruption fixture and fixed (see Priority 5 notes);
- `cargo fmt --all -- --check` passed;
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
- `cargo test --locked --all-targets` passed: 95 unit/property tests and 4 HTTP integration tests, 0 failures.

## 6.6 Latest compiler-verified snapshot (2026-07-12, de-monolith refactor)

- all six 1,000+-line files were split into cohesive modules (see Section 12); the largest source file is now 910 lines;
- moves were mechanical with no behavior changes; 76 Rust files, ~22,244 lines;
- `cargo fmt --all -- --check` passed;
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
- `cargo test --locked --all-targets` passed: 95 unit/property tests and 4 HTTP integration tests, 0 failures.

---

# 7. Known partial areas

These features exist but are not considered fully complete.

## 7.1 Time zones

Current:

- fixed UTC offset;
- validated range from UTC-14 to UTC+14;
- optional persisted IANA zone identifiers backed by `chrono-tz`;
- existing fixed-offset rows remain unchanged after migration;
- ambiguous fall-back wall times execute once at the earliest occurrence;
- nonexistent spring-forward wall times are skipped;
- calculated UTC `next_run_at` and the selected zone are exposed in schedule responses;
- DST forward/backward transition tests for `Europe/Rome`.

Missing:

- none. Time-zone data is supplied by the locked `chrono-tz` dependency; routine
  dependency-update pull requests must update `Cargo.lock` and pass the named-zone,
  DST-transition, schedule, and full backend gates before adoption. Ravyn does not
  fetch mutable time-zone data at runtime.

## 7.2 rqbit typing and compatibility

Current:

- typed normalized responses for stable operations;
- raw payload retained for forward compatibility;
- root capability inventory.

Supported-version policy (documented 2026-07-12): support is capability-based, not version-number-based. At startup Ravyn inspects the rqbit HTTP root document; the server must identify as `rqbit` and expose every required API:

```text
GET  /torrents
POST /torrents
GET  /torrents/{id}/stats/v1
POST /torrents/{id}/add_peers
POST /torrents/{id}/pause
POST /torrents/{id}/start
POST /torrents/{id}/delete
POST /torrents/{id}/forget
POST /torrents/{id}/update_only_files
```

A missing API marks the engine incompatible and torrent operations fail with a clear capability error instead of undefined behavior.

Missing:

- stable typed contracts for all DHT and diagnostic responses;
- tested compatibility matrix across rqbit versions (requires CI images with real rqbit binaries);
- runtime upload-control contract;
- explicit minimum and maximum tested versions (blocked on the same CI matrix).

## 7.3 yt-dlp support policy

Current:

- capability-based compatibility;
- early rejection;
- structured output contract;
- archive deduplication;
- persistent media state.

Supported-version policy (documented 2026-07-12): support is capability-based. Before any probe or download, Ravyn runs `yt-dlp --version` and `--help` and requires all of the following flags; an installation missing any of them is rejected early with a structured error:

```text
--ignore-config
--dump-single-json
--print
--progress-template
--download-archive
--ffmpeg-location
```

Missing:

- formal tested version range (requires CI images with real yt-dlp binaries; the capability contract above is the enforced floor);
- controlled updater;
- rollback;
- independent queue execution for every playlist item;
- more selective retry categories.

## 7.4 Runtime settings

Current:

- persistent settings;
- broad configuration coverage;
- base/effective configuration distinction;
- live global bandwidth update;
- restart classification.

Missing:

- live reload for more settings;
- `new_jobs_only` classification where appropriate;
- atomic runtime reconfiguration of concurrency and API middleware;
- richer settings validation reports.

## 7.5 Audit coverage

Current:

- broad administrative audit coverage;
- secret, settings, backup, restore, maintenance, job, import, rule, schedule, tag, page, token, and torrent operations;
- request-level success/failure audit for every `/v1` POST, PUT, PATCH, and DELETE;
- standardized hashed-token or local-client actor identity and request-ID correlation;
- bounded searchable JSON export through the paginated audit API and retention through maintenance;
- a transactionally serialized SHA-256 hash chain with retention anchors and a verification endpoint.

Missing or requiring review:

- none locally. An external audit sink remains an optional deployment integration,
  not a prerequisite now that local records are tamper-evident and verifiable.

## 7.6 Metrics

Current:

- essential process and persisted job metrics;
- bounded per-engine starts, outcomes, retries, failure classes, transferred bytes, and current throughput;
- end-to-end job duration histograms;
- progress-writer backlog;
- HTTP open-circuit rejections, validated redirects, and segmented range fallbacks;
- aggregate torrent download/upload rates and connected peer count;
- yt-dlp, FFmpeg, and 7-Zip duration/outcome histograms;
- post-action duration/outcome histograms;
- schedule delay and execution duration/outcome histograms;
- SSE receiver, replay-buffer, replay-count, resync, and sequence-span telemetry;
- OpenMetrics label-cardinality tests that exclude URLs, paths, tokens, job IDs, and arbitrary error text;
- segmented work-unit duration histograms by bounded outcome (`success`, `failure`, `cancelled`);
- hot-path SQLite statement latency histograms by named operation (`claim_next_queued`, `update_progress_batch`, `insert_job`, `claim_due_schedule`);
- process-wide SQLite busy/locked error counter recorded in the central `sqlx::Error` conversion;
- torrent seeding-stop counters by bounded policy reason (`ratio_limit`, `time_limit`, `engine_missing`, `removed`, `cancelled`, `other`);
- pre-connection DNS resolution duration histograms;
- free disk space on the download filesystem (Windows `GetDiskFreeSpaceExW`, Unix `statvfs`);
- temporary disk usage from `*.ravyn.part` files and `.ravyn-extract-*` staging directories through a depth- and entry-bounded scan.

Missing:

- none; the Section 7.6 backlog is complete. Additional per-engine depth (for example per-mirror or per-piece telemetry) belongs to the advanced transfer priorities.

## 7.7 Process hardening

Current:

- cancellation;
- timeout in relevant paths;
- bounded output;
- `kill_on_drop`;
- child termination;
- temporary directories;
- path confinement.

Missing:

- restricted tokens;
- filesystem capability restrictions;
- network restrictions;
- Linux/macOS sandbox equivalents;
- signed executable allow-list.

## 7.8 Advanced HTTP performance

Current:

- segmentation;
- dynamic work-unit distribution;
- persistent host learning;
- circuit breaker;
- mirror failover;
- Metalink v4 parsing with whole-file and ordered piece SHA-256 metadata;
- piece-verified sequential mirror failover with corruption quarantine;
- fair HTTP bandwidth scheduling;
- deterministic loopback network-comparative Criterion fixtures;
- validated concurrent mirror scheduling at non-overlapping range boundaries;
- checksum-before-commit Metalink piece transfers with per-mirror corruption quarantine;
- one delayed, bounded, piece-verified hedge with explicit loser-task shutdown;
- incremental single-stream SHA-256; resume rebuilds only the persisted prefix.

Missing:

- speculative duplication for unverified or non-Metalink ranges;
- splitting of an already active slow range;
- incremental whole-file SHA-256 for non-piece segmented transfers is implemented by an ordered hasher that waits for durable contiguous range completion and hashes each range exactly once;
- explicit Happy Eyeballs;
- HTTP/3 experiments;
- benchmark-based socket tuning.

The 2026-07-12 loopback fixture compares a 12 ms primary with a guarded
duplicate admitted after 4 ms against a 2 ms mirror, using identical verified
256 KiB payloads. Criterion measured the single source at
13.754–14.060 ms and the guarded first-valid result at 9.023–9.184 ms. This
justifies continued design work on bounded speculation, but it does not by
itself establish a safe production policy: connection admission, destination
isolation, loser cleanup, cancellation, and per-host thresholds remain open.

Implementation reconciliation on 2026-07-12: production HTTP workers now admit
at most the already bounded 16 mirrors only after redirect-by-redirect network
validation, exact object length, byte-range support, and either a matching
validator or checksum-backed identity. Distinct non-overlapping work units are
scheduled across admitted sources under the existing per-host connection
semaphores and fair per-job bandwidth limiter. Metalink layouts are scheduled
at their actual piece boundaries; each piece is buffered with a 64 MiB cap,
SHA-256 verified, and only then written and durably marked complete. A corrupt
source is quarantined for the remainder of the transfer. For these verified
pieces only, a single alternate source may start after 250 ms; the first valid
piece wins and `JoinSet::shutdown` aborts and drains the loser before commit.
Focused integration coverage proves that two validated mirrors both supply
work units, corrupt-piece isolation still completes from a good mirror, and a
slow verified piece is completed by the bounded hedge. General unverified
range duplication and active-range splitting remain unfinished.

## 7.9 Release engineering

Current:

- GitHub CI definitions;
- stable and MSRV checks in planned workflow;
- cargo-audit and cargo-deny policy files;
- source manifests;
- GitHub-hosted Windows, Linux, and macOS release archives;
- SHA-256 checksums, CycloneDX SBOMs, and GitHub provenance/SBOM attestations;
- tag-driven stable channel policy with least-privilege GitHub workflow permissions;
- external tools are not silently bundled: verified managed-engine manifests provide
  opt-in activation and rollback for yt-dlp/FFmpeg/rqbit-compatible executables.

Missing:

- in-place Ravyn binary self-update/rollback. This is intentionally deferred until
  a separately installed client can coordinate process replacement safely; the
  repository currently ships archives rather than an installer or privileged agent;
- protected GitHub release environment/branch rules, which are repository-host
  settings and cannot be enforced by committed workflow YAML alone.

---

# 8. Remaining work

## Priority 0 — Verify the current snapshot

Verified on 2026-07-12 (Windows, Rust 1.85-compatible manifest):

- repaired Increment 4 media retry reconciliation, checked SQLite count conversions, and typed rqbit file handling;
- `cargo fmt --all -- --check` passed;
- `cargo check --locked --all-targets` passed;
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
- `cargo test --locked --all-targets` passed: 73 unit tests and 3 HTTP integration tests;
- `cargo build --release --locked` passed.

Priority 0 compiler and test verification is complete. Populated-database migration and destructive restore fault fixtures remain tracked under Priority 5 because the repository does not yet contain those fixtures.

Before adding another large feature tranche:

1. run full formatting;
2. run Cargo check;
3. run strict Clippy;
4. run all tests;
5. fix any Increment 2–4 compiler errors;
6. run migrations on a populated database fixture;
7. run HTTP integration tests;
8. update this document with the exact results.

This is the highest-value next action because the latest increments were not compiler-verified.

## Priority 1 — Production metrics

**Completed and verified on 2026-07-12 (second pass):**

- the bounded runtime registry and core engine/process/scheduler instrumentation listed in Section 7.6 are implemented;
- the previously missing metrics are now implemented: work-unit duration, SQLite query latency, SQLite busy errors, seeding-policy outcomes, DNS duration, free disk space, and temporary disk usage;
- SQLite busy/locked detection covers every query path through the central `From<sqlx::Error>` conversion;
- disk gauges are computed off the async runtime through `spawn_blocking` with a bounded filesystem scan;
- the repository generated and now tracks `Cargo.lock` (it was missing despite the documented `--locked` workflow);
- `cargo fmt --all -- --check` passed;
- `cargo check --all-targets` passed (lock file generated on this run; subsequent commands used `--locked`);
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
- `cargo test --locked --all-targets` passed: 87 unit/property tests and 3 HTTP integration tests.

Priority 1 is complete. The requirements below remain as regression criteria.

Implement the metrics listed in Section 7.6.

Requirements:

- bounded label cardinality;
- no raw URL labels;
- no tokens or paths in labels;
- stable metric names;
- per-engine aggregation;
- histogram buckets chosen through measurement;
- metrics documentation.

## Priority 2 — IANA time zones and DST

Completed and verified on 2026-07-12:

- migration `0017_schedule_timezones.sql` adds an optional constrained `timezone_name` without modifying existing fixed-offset schedules;
- schedule create, update, pagination, claim, skip, and recurrence queries preserve the named zone;
- invalid or oversized zone names are rejected;
- ambiguous times use the earliest occurrence and nonexistent times are skipped;
- `cargo check --locked --all-targets` and strict Clippy passed;
- `cargo test --locked --all-targets` passed: 76 unit tests and 3 HTTP integration tests.

Implement named time zones using a maintained Rust time-zone library.

Required behavior:

- store IANA zone names;
- preserve existing fixed-offset schedules;
- define ambiguous local-time behavior;
- define nonexistent local-time behavior;
- test DST forward and backward transitions;
- expose calculated next-run details;
- migrate existing schedules safely.

## Priority 3 — Safe FFmpeg presets

Completed and verified on 2026-07-12:

- all ten named presets are serialized with the documented public names and expand to bounded static arguments;
- preset/output-extension combinations are validated;
- FFmpeg is restricted to local-file input protocols and confined output paths;
- arbitrary arguments require `unsafe_arguments=true` plus `--allow-unsafe-ffmpeg`/`RAVYN_ALLOW_UNSAFE_FFMPEG`;
- browser imports remain capability-reduced and cannot supply post-processing actions;
- OpenAPI components document the preset enum and conversion action;
- external processes have a six-hour safety timeout in addition to cancellation and `kill_on_drop`;
- strict Clippy passed and `cargo test --locked --all-targets` passed: 78 unit tests and 3 HTTP integration tests.

Introduce named presets:

```text
video-copy
video-h264
video-h265
video-av1
audio-mp3
audio-aac
audio-opus
audio-flac
image-avif
image-webp
```

Requirements:

- validated preset options;
- no arbitrary input protocols by default;
- no arbitrary output paths;
- bounded process runtime;
- explicit advanced unsafe mode;
- restricted browser clients;
- OpenAPI schemas;
- tests.

## Priority 4 — External-process resource limits

Completed and verified on 2026-07-12:

- shared supervisor enforces wall time, bounded stdout/stderr, optional output-file size, cancellation, and process-tree cleanup;
- Windows children use Job Objects with kill-on-close, per-process CPU time, and memory limits;
- Unix children use isolated process groups, group termination, `RLIMIT_CPU`, and `RLIMIT_AS`;
- yt-dlp downloads/probes/dependency checks, FFmpeg, 7-Zip, and image conversion use the hardened execution path;
- live tests verify bounded pipe draining and deadline-driven tree termination;
- strict Clippy passed and `cargo test --locked --all-targets` passed: 80 unit tests and 3 HTTP integration tests.

Add:

- process supervisor;
- CPU time limit;
- memory limit;
- wall-clock timeout;
- output file-size limit;
- stdout/stderr bounds;
- entire process-tree termination;
- platform-specific isolation where available.

## Priority 5 — Fault injection and property tests

Implementation status verified on 2026-07-12:

- added `proptest` infrastructure with 1,000-case segment coverage/non-overlap properties and 500-case output-root confinement properties;
- added live process fault tests for bounded-output overflow, wall timeout, and cancellation/tree termination;
- added private/special-address regression coverage for IPv4 and IPv6 literals;
- the new IPv6 property test found and fixed bracketed IPv6 loopback bypass in both lexical validation and DNS resolution;
- strict Clippy passed and `cargo test --locked --all-targets` passed: 84 unit/property tests and 3 HTTP integration tests;
- remaining filesystem, SQLite, recovery, archive, scheduler, idempotency, and protocol fault cases below remain active Priority 5 work.

Second pass verified on 2026-07-12 (`fault_injection_tests` in `src/storage/repository.rs` plus a new HTTP integration test):

- SQLite busy/locked: a zero-busy-timeout writer contending with an active write transaction fails with SQLITE_BUSY and the process-wide `ravyn_sqlite_busy_total` counter increments through the central error conversion;
- scheduler lease loss: an expired lease is reclaimable by a new claimant, and the stale owner receives a conflict from both renewal and completion;
- concurrent claims: eight simultaneous `claim_due_schedule` calls produce exactly one owner;
- overlap replacement: a `replace` claim marks the still-running execution as replaced and cancellation-requested;
- idempotency conflict: replays with an equal payload return the original resource, and a changed payload for the same key cannot overwrite the stored record;
- redirect loop: a self-redirecting server fails the job with the bounded "redirect limit" protocol error through the full manager lifecycle;
- restore faults: a staged backup corrupted on disk is abandoned gracefully at startup with a recorded failure while the active database survives; an open/migration failure after apply rolls back to the previous database; an orphaned staged database without its request marker is rejected;
- **bug found and fixed by the corruption fixture:** a staged database that could not even be opened as SQLite made `verify_database_file` return a raw database error that bypassed the graceful-abandon path, leaving the restore marker in place and making every startup fail; `apply_pending` now treats verification errors the same as failed integrity checks (`staged_database_is_valid` in `src/storage/recovery.rs`);
- strict Clippy passed and `cargo test --locked --all-targets` passed: 95 unit/property tests and 4 HTTP integration tests.

Add tests for (remaining cases; struck items are covered):

- disk full;
- permission denied;
- file locked;
- ~~SQLite busy~~ — covered 2026-07-12;
- ~~SQLite locked~~ — covered by the same contention fixture;
- crash between file sync and DB checkpoint;
- ~~crash during restore state transitions~~ — covered 2026-07-12 (marker restart, staged corruption, post-apply open failure, orphaned staged database);
- shutdown during checksum;
- shutdown during yt-dlp;
- shutdown during FFmpeg;
- shutdown during 7-Zip;
- rqbit restart;
- ~~changing ETag~~ — covered by the existing remote-identity integration test;
- ~~invalid range~~ — covered by the existing range-liar integration test;
- ~~redirect loop~~ — covered 2026-07-12;
- DNS rebinding (lexical and literal coverage exists; live rebinding fixture still open);
- archive traversal (validator unit coverage exists; live 7-Zip fixture still open);
- archive bomb;
- playlist partial failure;
- ~~scheduler lease loss~~ — covered 2026-07-12;
- ~~overlap replacement~~ — covered 2026-07-12;
- ~~idempotency conflict~~ — covered 2026-07-12.

Property tests:

- ~~segment coverage has no gaps~~ — covered;
- ~~segments do not overlap~~ — covered;
- ~~path confinement always holds~~ — covered;
- pagination cursors are stable (deterministic test exists; property-based version open);
- ~~idempotency replays equal payloads~~ — covered 2026-07-12;
- ~~idempotency rejects changed payloads~~ — covered 2026-07-12;
- ~~scheduler claims have one owner~~ — covered 2026-07-12 (8-way concurrency test);
- rule priority is deterministic (first-wins unit coverage exists; property-based version open).

## Priority 6 — Torrent upload controls

Implement only after verifying rqbit support:

- per-torrent upload limit;
- global upload limit;
- scheduled upload profile;
- upload metrics;
- capability error when unsupported.

## Priority 7 — Fair bandwidth scheduler

Replace simple global limiting with a policy capable of:

- weighted priority;
- minimum and maximum share per job;
- foreground/background classes;
- starvation prevention;
- work-conserving idle-flow redistribution;
- persistent IANA-time-zone scheduled profiles with live boundary application;
- separate torrent upload/download pools (blocked on upstream rqbit runtime rate APIs);
- live reconfiguration.

## Priority 8 — Speculative HTTP completion

Add benchmark-driven:

- slow-tail detection;
- bounded duplicate request;
- first-valid-response wins;
- loser cancellation;
- per-host speculation policy;
- corruption-safe verification;
- connection-count guardrails.

## Priority 9 — True multi-source and Metalink

Implement:

- same-file mirror validation;
- piece scheduling across mirrors;
- mirror performance ranking;
- per-mirror failure profile;
- mirror corruption quarantine;
- Metalink parsing;
- piece checksums;
- fallback behavior.

## Priority 10 — Release hardening

Complete:

- SBOM;
- GitHub release archives;
- GitHub provenance and SBOM attestations;
- reproducible build process;
- updater metadata;
- rollback;
- dependency packaging;
- release checklist;
- security response policy;
- compatibility policy.

---

# 9. Testing roadmap

## 9.1 Fast checks for every change

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
```

## 9.2 Full suite before merging milestones

```bash
cargo test --locked --all-targets
```

## 9.3 Required CI platforms

- Windows;
- Linux;
- macOS;
- current stable Rust;
- minimum supported Rust.

## 9.4 Fuzz targets

- URL parsing;
- `Content-Disposition`;
- `Content-Range`;
- cron parsing;
- rule parsing;
- yt-dlp progress;
- rqbit payload normalization;
- archive output paths;
- browser import validation;
- restore markers.

## 9.5 Soak scenarios

- thousands of queued jobs;
- hundreds of active/retrying jobs;
- repeated pause/resume;
- repeated backend restarts;
- rqbit restart loops;
- long playlists;
- large torrent file lists;
- scheduler catch-up;
- SSE reconnect storms;
- continuous page monitoring;
- repeated backup/restore staging.

## 9.6 Benchmarks

Compare with:

- curl;
- aria2;
- wget;
- direct yt-dlp;
- direct rqbit where meaningful.

Measure:

- throughput;
- CPU;
- memory;
- disk writes;
- time to resume;
- queue latency;
- checksum overhead;
- segment-count impact;
- mirror benefit;
- tail latency.

---

# 10. Beta-ready definition

Ravyn backend may be called beta-ready when:

- the latest source passes Cargo check, Clippy, and tests;
- all migrations work on fresh and upgraded databases;
- database backup and restore are verified through fault tests;
- output lineage is stable;
- media and torrent multi-file outputs are recoverable;
- external-tool capability errors are clear;
- API errors are stable and documented;
- OpenAPI remains route-complete;
- all large collections are paginated;
- secrets are not stored in plaintext;
- remote access cannot be enabled insecurely;
- essential production metrics exist;
- destructive integration tests pass;
- supported external-tool versions are documented.

---

# 11. Production-ready definition

Ravyn backend may be called production-ready when:

- all beta requirements are met;
- latest releases have checksums and GitHub attestations;
- releases are reproducible;
- SBOM and license reports are published;
- process isolation is implemented or clearly bounded;
- time zones and DST are correct;
- fault injection and soak tests pass;
- migration and restore rollback are verified;
- observability covers every engine and major failure mode;
- support and deprecation policies are documented;
- release rollback works;
- no critical plaintext secrets remain;
- remote transport security is production-safe.

---

# 12. Technical debt and recommended refactors

**Completed on 2026-07-12.** Every file above 1,000 lines was split along the layout recommended below (see Section 3.1 for the resulting tree):

- `storage/repository.rs` (2,579 lines → 60) into `jobs`, `outputs`, `settings`, `audit`, `secrets`, `backup`, `schedules`, plus torrent records into `torrent_policy.rs`, rules into `automation.rs`, and cross-module tests into `repository_tests.rs`;
- `core/manager.rs` (1,990 → ~280) into `lifecycle`, `dispatcher`, `execution`, `bulk`, `automation`, `maintenance`, `media_control`, `torrent_control`;
- `api/routes.rs` (1,986 → ~240) into `routes/{system,jobs,media,torrents,automation,browser}.rs` with the router unchanged;
- `adapters/torrent.rs` into `torrent/wire.rs` (payload normalization and its tests);
- `adapters/media.rs` into `media/ytdlp.rs` (command building, output parsing, capability probing);
- `api/openapi.rs` into `openapi/{operations,components}.rs`.

The refactor was purely mechanical (verbatim moves; only visibility markers and pool-accessor rewrites), and API and database behavior are unchanged: the full suite (95 unit/property + 4 integration tests), strict Clippy, and rustfmt all pass on the split tree.

Refactoring rules (still in force for future work):

- do not mix large refactors with feature changes;
- preserve API and database behavior;
- add tests before moving lifecycle code;
- use typed domain errors;
- keep transactions close to repository operations;
- avoid detached tasks;
- keep event delivery separate from durable persistence.

---

# 13. Documentation policy

After adopting this document, the following historical documents may be deleted or archived:

```text
AUDIT.md
FIXES_COMPLETED.md
PHASE1.md
PHASE2_STATIC_AUDIT.md
PHASE3.md
PHASE3_STATIC_AUDIT.md
PHASE4.md
PHASE4_STATIC_RESULTS.json
PHASE5.md
PHASE5_STATIC_RESULTS.json
STATIC_AUDIT_PHASE3.md
STATIC_AUDIT_PHASE4.md
STATIC_AUDIT_PHASE5.md
BACKEND_PROGRESS_*.md
INCREMENT_*_STATIC_RESULTS.json
PROJECT_STATUS_AND_ROADMAP.md
Ravyn-Project-Status-and-Roadmap.md
Ravyn-Backend-Remaining-Work-Roadmap.md
SOURCE_MANIFEST_INCREMENT_*.txt
```

Recommended documents to keep:

```text
README.md
AGENTS.md
RAVYN_MASTER_PROJECT_DOCUMENT.md
Cargo.toml
Cargo.lock
deny.toml
migrations/
.github/
src/
tests/
```

The source manifest does not need to live permanently in the repository. Generate it as a release artifact.

Update this document after every milestone with:

- date;
- completed items;
- partial items;
- deferred items;
- migrations added;
- API changes;
- validation commands and results;
- known risks;
- next priorities.

---

# 14. Immediate next milestone

The next milestone should be called:

```text
Increment 5 — Operational Hardening
```

Progress within this milestone as of 2026-07-12:

1. ~~compile and test the complete Increment 4 source~~ — done;
2. ~~implement deep operational metrics~~ — done, including the former Section 7.6 “Missing” list;
3. ~~add IANA time zones and DST~~ — done;
4. ~~add safe named FFmpeg presets~~ — done;
5. ~~add external-process resource limits~~ — done;
6. ~~add restore fault tests~~ — done (staged corruption, post-apply open failure, orphaned staged database; one recovery bug found and fixed);
7. ~~add scheduler overlap and DST destructive-concurrency tests~~ — done (lease loss, single-owner concurrency, replace-policy cancellation, DST transitions);
8. ~~add initial property-testing infrastructure~~ — done; remaining fault cases tracked under Priority 5;
9. ~~document supported yt-dlp and rqbit versions~~ — done as an enforced capability contract (Sections 7.2 and 7.3); a numeric tested matrix remains blocked on CI images with real binaries;
10. update this master document with verified results — ongoing after every pass.

**Increment 5 status (2026-07-12): complete.** Every scoped item is either done or explicitly reduced to work that requires infrastructure this repository does not yet have (CI images with real yt-dlp/rqbit binaries for a numeric version matrix, and OS-level fixtures for disk-full/permission/file-locked faults). Those remain tracked under Priorities 5 and 7.2/7.3.

The next milestone should be called:

```text
Increment 6 — Transfer Scheduling
```

Recommended scope:

1. Priority 7 fair bandwidth scheduler (weighted priority, per-job min/max share, starvation prevention, live reconfiguration);
2. the remaining portable Priority 5 property tests (pagination-cursor and rule-priority property versions);
3. CI workflow activation — `.github/workflows/backend-ci.yml` already defines the full gate (fmt/check/clippy/test/build across Windows/Linux/macOS on stable and 1.85, plus cargo-audit/cargo-deny); with `Cargo.lock` now tracked it runs on the next push to GitHub;
4. after CI exists with real external binaries: the yt-dlp/rqbit tested version matrix and the remaining tool-dependent fault fixtures.

Do not begin speculative HTTP multi-source work until benchmark fixtures exist.

Implementation reconciliation on 2026-07-12:

- the complete locked baseline is green again after repairing a strict-Clippy
  regression in the scheduler water-filling loop;
- the portable Priority 5 pagination-cursor and rule-priority property tests
  were already present in the source, so the earlier open-status text was
  stale;
- a populated version-9 SQLite fixture now applies the remaining embedded
  migrations, proves legacy job preservation, checks the final migration
  ledger, verifies the named-time-zone schema, and runs integrity checking;
- the fair scheduler is no longer an isolated prototype: HTTP transfers now
  register scoped flows, derive weights/classes from job priority, treat the
  existing per-job speed limit as a hard cap, rebalance active jobs when the
  global limit changes, and automatically release allocations on every exit;
- authenticated mutating API requests now add a request-level audit record
  containing the bounded actor identity, request ID, method, status, route,
  and success/failure outcome, including handler failures;
- the managed-engine foundation now validates signed manifest entries, rejects
  unsafe URLs and filenames, streams bounded downloads, verifies exact size
  and SHA-256, installs into versioned directories, atomically replaces active
  metadata on Windows/Unix, selects verified managed defaults at startup,
  cleans failed staging files, and performs checksum-verified rollback;
- Metalink v4 parsing and job creation now enforce bounded XML, HTTPS mirror
  identity, exact size, whole-file SHA-256, and ordered piece SHA-256 ledgers;
  corrupt piece data is discarded before failover to the next mirror;
- cargo-fuzz now builds 11 parser/protocol targets, the nightly workflow runs
  each target with a bounded budget on Linux and runs the release/test matrix
  on Windows, Linux, and macOS;
- a Criterion baseline covers transfer planning and fair-scheduler rebalance;
  measured scheduler setup/rebalance ranges from about 0.48 microseconds for
  one flow to 1.39 milliseconds for 128 flows on the local Windows fixture;
- a deterministic loopback Criterion fixture now provides the required
  network comparison before speculative transfer work: a 12 ms primary alone
  measured 13.754–14.060 ms, while a verified duplicate delayed by 4 ms and
  racing a 2 ms mirror measured 9.023–9.184 ms for the same 256 KiB body;
  this supports guarded experimentation but production speculation remains
  unfinished pending isolated outputs, admission limits, and loser cleanup;
- the GitHub-only release workflow builds Windows/Linux/macOS archives, emits
  CycloneDX SBOM and SHA-256 files, creates GitHub provenance/SBOM
  attestations, and publishes tagged GitHub Releases without an external
  signing service;
- checksum failures can no longer leave a corrupt final-looking output: failed
  artifacts are quarantined (or removed if quarantine is unavailable), and
  restore/engine metadata reads are bounded against oversized state files;
- the post-fixture 2026-07-12 gate is fully green: `cargo fmt --all -- --check`,
  `cargo check --locked --all-targets`, strict all-feature/all-target Clippy,
  and `cargo test --locked --all-targets` pass; the latter runs 127 unit and
  property tests, 7 HTTP integration tests, and all Criterion targets in test
  mode. The explicit locked HTTP integration run passes the same 7 tests,
  `cargo check --manifest-path fuzz/Cargo.toml --bins` passes for all 11 fuzz
  binaries, and `cargo build --locked --release` succeeds.

The post-multi-source 2026-07-12 gate is also fully green: formatting, locked
all-target check, strict all-feature/all-target Clippy, 127 unit/property tests,
7 HTTP integration tests, all 11 fuzz binaries, and the locked optimized build
pass. The release build completed in 95.1 seconds after the first 60-second
invocation timed out without a compiler error. The affected loopback benchmark
measured the single 12 ms source at 13.671–13.843 ms and the guarded delayed
first-valid result at 8.984–9.226 ms, with no statistically significant
regression against the prior fixture.

Increment 6 remains partial. Separate torrent pools and upstream rqbit
upload-control APIs, destructive OS fixtures, general unverified-range speculation, and
active-range splitting remain open. Validated concurrent piece-level
multi-source transfer and one bounded verified-piece hedge are now implemented
and tested. The remaining items are not
represented as complete merely because
their foundations or CI wiring now exist.

The scheduler now detects demand through limiter callbacks: flows idle for 500
ms yield their allocation, while a returning flow synchronously rejoins the
weighted water-fill before consuming, so aggregate contention remains bounded.
Persistent settings also accept at most 32 non-overlapping recurring bandwidth
windows using an IANA time zone, ISO weekdays, and minute bounds (including
overnight wrap). Invalid zones and overlaps are rejected before persistence;
patches apply immediately and a supervised 15-second task reapplies the
effective limit across time-window and DST boundaries. Unit tests cover idle
yield/rejoin, named-zone conversion, overnight behavior, overlap rejection, and
invalid zones. Separate torrent upload/download pools cannot be enforced by
Ravyn's process because torrent payload bytes remain inside rqbit and its
currently required HTTP capability contract exposes no runtime upload/download
rate endpoint; a real pool therefore remains an upstream capability blocker,
not a local token bucket that would fail to govern traffic.

The post-scheduler 2026-07-12 gate passes `cargo fmt --all -- --check`,
`cargo check --locked --all-targets`, strict all-feature/all-target Clippy,
`cargo test --locked --all-targets` (130 unit/property tests, 7 HTTP integration
tests, and Criterion targets in test mode), the explicit 7-test HTTP integration
run, all 11 fuzz binaries, and the locked release build (147.7 seconds). After
removing an avoidable per-rebalance allocation discovered by Criterion, fair
scheduler setup/rebalance measures 0.674–0.717 microseconds for one flow,
12.085–12.730 microseconds for 8, 150.26–156.26 microseconds for 32, and
1.676–1.735 milliseconds for 128. The demand-aware policy adds bounded
registration bookkeeping while retaining millisecond-scale setup at the
configured high end.

---

# 15. Final status

Ravyn already has a broad and technically sophisticated backend:

```text
HTTP engine
+ media engine
+ torrent engine
+ durable queue
+ automation
+ browser bridge backend
+ rules and tags
+ scheduler
+ output lineage
+ post-processing
+ security
+ diagnostics
+ backup and restore
```

The core product is built.

The remaining path is:

```text
verify latest source
→ deepen observability
→ complete time and process safety
→ test destructive scenarios
→ benchmark advanced transfer algorithms
→ harden releases
```

This document is the canonical reference for project status and future backend work.
