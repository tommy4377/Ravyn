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
+ signed reproducible releases
```

---

# 2. Current repository snapshot

This section describes the latest consolidated backend snapshot after Increment 4.

| Item | Current value |
|---|---:|
| Rust source and Rust test lines | approximately 19,928 |
| Rust source files | 49 |
| Database migrations | 16 |
| Database tables after migrations | 23 |
| Axum/OpenAPI operations | 90 / 90 |
| Rust test attributes present | approximately 73 |
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

```text
src/
├── adapters/
│   ├── media.rs
│   └── torrent.rs
├── api/
│   ├── routes.rs
│   ├── openapi.rs
│   └── pagination.rs
├── core/
│   ├── manager.rs
│   ├── models.rs
│   ├── events.rs
│   ├── progress.rs
│   └── rate_limit.rs
├── download/
│   ├── adapter.rs
│   ├── http.rs
│   ├── planner.rs
│   ├── probe.rs
│   └── segmented.rs
├── postprocess/
│   └── pipeline.rs
├── services/
│   ├── browser.rs
│   ├── checksum.rs
│   ├── cron.rs
│   ├── dedup.rs
│   ├── filename.rs
│   ├── imports.rs
│   ├── rules.rs
│   ├── scheduler.rs
│   ├── schedules.rs
│   ├── secrets.rs
│   ├── security.rs
│   └── sniffer.rs
└── storage/
    ├── automation.rs
    ├── host_profiles.rs
    ├── media.rs
    ├── pagination.rs
    ├── recovery.rs
    ├── repository.rs
    ├── segments.rs
    └── torrent_policy.rs
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

### Partial

Production metrics are not yet deep enough for every engine and failure mode.

See the remaining-work section for the required metrics.

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

- time-zone database update policy.

## 7.2 rqbit typing and compatibility

Current:

- typed normalized responses for stable operations;
- raw payload retained for forward compatibility;
- root capability inventory.

Missing:

- stable typed contracts for all DHT and diagnostic responses;
- tested compatibility matrix across rqbit versions;
- runtime upload-control contract;
- explicit minimum and maximum tested versions.

## 7.3 yt-dlp support policy

Current:

- capability-based compatibility;
- early rejection;
- structured output contract;
- archive deduplication;
- persistent media state.

Missing:

- formal tested version range;
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
- secret, settings, backup, restore, maintenance, job, import, rule, schedule, tag, page, token, and torrent operations.

Missing or requiring review:

- consistent failure audit for every mutating route;
- standardized actor identity;
- audit correlation with request ID;
- retention and export policy;
- tamper-evidence or external audit sink.

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
- OpenMetrics label-cardinality tests that exclude URLs, paths, tokens, job IDs, and arbitrary error text.

Missing:

- work-unit duration;
- SQLite query latency;
- SQLite busy errors;
- seeding-policy outcomes;
- DNS duration;
- free disk space;
- temporary disk usage.

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
- mirror failover.

Missing:

- true speculative tail duplication;
- splitting of an already active slow range;
- piece-level multi-source downloading;
- Metalink;
- per-mirror concurrent scheduling;
- corruption isolation by mirror;
- fair bandwidth scheduler;
- incremental hashing;
- explicit Happy Eyeballs;
- HTTP/3 experiments;
- benchmark-based socket tuning.

## 7.9 Release engineering

Current:

- GitHub CI definitions;
- stable and MSRV checks in planned workflow;
- cargo-audit and cargo-deny policy files;
- source manifests;
- checksums.

Missing:

- signed binaries;
- signed installers;
- SBOM;
- reproducibility attestations;
- automatic updater;
- rollback;
- release-channel policy;
- external dependency bundling policy;
- signing credentials and secure signing infrastructure.

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

Implementation status verified on 2026-07-12:

- the bounded runtime registry and core engine/process/scheduler instrumentation listed in Section 7.6 are implemented;
- `cargo check --locked --all-targets` passed;
- `cargo clippy --locked --all-targets --all-features -- -D warnings` passed;
- `cargo test --locked --all-targets` passed: 74 unit tests and 3 HTTP integration tests;
- remaining Priority 1 scope is limited to the explicitly retained “Missing” list in Section 7.6.

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

Add tests for:

- disk full;
- permission denied;
- file locked;
- SQLite busy;
- SQLite locked;
- crash between file sync and DB checkpoint;
- crash during restore state transitions;
- shutdown during checksum;
- shutdown during yt-dlp;
- shutdown during FFmpeg;
- shutdown during 7-Zip;
- rqbit restart;
- changing ETag;
- invalid range;
- redirect loop;
- DNS rebinding;
- archive traversal;
- archive bomb;
- playlist partial failure;
- scheduler lease loss;
- overlap replacement;
- idempotency conflict.

Property tests:

- segment coverage has no gaps;
- segments do not overlap;
- path confinement always holds;
- pagination cursors are stable;
- idempotency replays equal payloads;
- idempotency rejects changed payloads;
- scheduler claims have one owner;
- rule priority is deterministic.

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
- separate torrent upload/download pools;
- scheduled profiles;
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
- signed builds;
- signed installer;
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
- latest releases are signed;
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

Some files have become large and should be split before substantial further growth.

Recommended direction:

```text
storage/
├── jobs.rs
├── outputs.rs
├── settings.rs
├── audit.rs
├── secrets.rs
├── backup.rs
├── schedules.rs
└── media.rs

api/
├── jobs.rs
├── settings.rs
├── schedules.rs
├── torrents.rs
├── media.rs
├── browser.rs
└── system.rs

core/
├── dispatcher.rs
├── execution.rs
├── lifecycle.rs
├── recovery.rs
└── bulk.rs
```

Refactoring rules:

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

Recommended scope:

1. compile and test the complete Increment 4 source;
2. implement deep operational metrics;
3. add IANA time zones and DST;
4. add safe named FFmpeg presets;
5. add external-process resource limits;
6. add restore fault tests;
7. add scheduler overlap and DST tests;
8. add initial property-testing infrastructure;
9. document supported yt-dlp and rqbit versions;
10. update this master document with verified results.

Do not begin speculative HTTP multi-source work until the current source is compiler-verified and benchmark fixtures exist.

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
