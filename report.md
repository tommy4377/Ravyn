# Ravyn backend completion report — 2026-07-13

This report summarizes the state of the backend after the roadmap-completion
pass, and — most importantly — lists the items that **cannot be finished from
this repository alone** because they need an action from you or an upstream
project.

## What was completed in this pass

1. **Priority 8 — speculative HTTP completion and active-range splitting**
   (the last open implementation items of Increment 6/7.8):
   - idle workers now split the un-downloaded tail of the largest active
     range (≥ 16 MiB remaining) into new durable work units, so the end of a
     transfer is parallelized instead of waiting on one connection; split
     layouts resume correctly after a restart;
   - one bounded speculative duplicate ("hedge") per work unit, admitted only
     with an identity anchor (whole-file SHA-256 or a source validator),
     buffered into isolated memory (≤ 64 MiB), connection-guarded by
     `try_acquire` (speculation never queues), with slow-tail detection
     (stalled, or projected > 5 s **and** under half the job's aggregate
     pace), first-valid-winner commit, and loser cancellation/cleanup;
   - the incremental whole-file hasher was generalized from fixed per-plan
     flags to a byte-range durable-coverage structure, so hashing stays
     exactly-once under splits and hedges;
   - new metrics: `ravyn_http_range_splits_total`,
     `ravyn_http_speculation_wins_total`, `ravyn_http_speculation_losses_total`;
   - deterministic tests: a stalled tail completed by one hedge; a wide
     stalled unit split into new work units; coverage-merge, retiled-layout
     resume, and crash-window resume unit tests.

2. **Priority 5 — remaining portable fault fixtures**:
   - permission denied (read-only partial file) fails with a clear error;
   - Windows share-mode file lock fails without hanging (cfg(windows));
   - crash between file sync and DB checkpoint recovers by re-downloading the
     unpersisted half and passing whole-file verification;
   - shutdown during checksum returns `Cancelled` promptly and leaves the
     file alone;
   - an rqbit engine restart mid-transfer (three dropped stats polls against
     a mock engine) is tolerated by the poll-failure budget and the job
     completes.

3. **Priority 10 — release policy artifacts**:
   - `SECURITY.md` (vulnerability reporting, scope, supported versions);
   - `COMPATIBILITY.md` (API/database/engine/OS/settings guarantees);
   - expanded `RELEASE.md` (checklist, rollback, reproducibility notes);
   - the release workflow now emits `ravyn-release.json` updater metadata
     (schema, version, per-artifact SHA-256) and pins `--remap-path-prefix`
     for binary reproducibility.

4. **Runtime settings (7.4)**:
   - new `POST /v1/settings/validate` reports **every** failing field (with
     isolated per-field blame) instead of only the first error, without
     persisting anything; registered in OpenAPI;
   - the capabilities endpoint no longer misreports `speculative_http` and
     `concurrent_multi_source` as disabled.

5. **Review pass**: the full diff was re-reviewed; two real defects found and
   fixed (a hedge heuristic that could duplicate a healthy large tail, and a
   crash-window resume state that would have discarded all progress), plus
   the stale capabilities list above.

Final gate (2026-07-13): `cargo fmt --all -- --check`, `cargo check --locked
--all-targets`, strict all-feature Clippy, `cargo test --locked --all-targets`
(152 unit/property + 9 HTTP integration tests), fuzz-target check (11
binaries), and the locked release build all pass.

## Blocked items that need YOU

1. **Push to GitHub / activate CI.** `.github/workflows/backend-ci.yml`,
   `nightly.yml`, and `release.yml` are complete but dormant until the
   repository is pushed to GitHub. Everything in the next two points also
   waits on this.
2. **GitHub repository settings** (cannot be enforced from committed YAML):
   branch protection for `master`, tag protection for `v*`, and a `release`
   environment with required reviewers. Configure these in the repository's
   Settings once pushed.
3. **CI images with real yt-dlp / rqbit / 7-Zip binaries.** The numeric
   tested-version matrices (7.2/7.3), tool-dependent fault fixtures
   (archive-bomb and traversal with real 7-Zip, playlist partial failure with
   real yt-dlp, shutdown-during-tool for each real tool), and soak scenarios
   need runners with the actual executables installed. The capability
   contracts are enforced in code in the meantime.
4. **Decide on release cadence**: pushing an annotated `v*` tag is the only
   remaining step to produce a signed-attestation GitHub release.

## Blocked upstream (not actionable locally)

- **Separate torrent upload/download bandwidth pools (Priority 6)**: torrent
  payload bytes live inside rqbit, and its required HTTP API exposes no
  runtime upload/download rate endpoint. A local token bucket would not
  govern real traffic, so this remains an upstream rqbit capability blocker.

## Intentionally deferred (documented design decisions)

- **In-place Ravyn self-update/rollback**: deferred until a separately
  installed client can coordinate process replacement; releases ship archives
  and `ravyn-release.json` gives future clients verified update discovery.
  Managed external engines already have verified install + rollback.
- **OS-destructive fault fixtures** (true disk-full, volume-level failures):
  these mutate machine state and belong in disposable CI/VM images, not on a
  developer machine.
- **Process sandboxing depth (7.7)**: restricted tokens, filesystem/network
  capability restrictions, and signed-executable allow-lists remain open
  hardening work beyond the current supervisor limits (job objects, rlimits,
  process-tree termination).
- **`new_jobs_only` settings classification (7.4)**: making engine settings
  (retries, timeouts, per-host connections, tool paths…) apply to newly
  started jobs without a restart requires adapters to snapshot a swappable
  config handle per job instead of the bootstrap `Arc<Config>` (54 access
  sites across 7 structs). It is a mechanical but wide refactor; today those
  settings are honestly classified `backend_restart`, and the new validate
  endpoint reports their constraints. Recommended as the first item of the
  next increment.
- **Happy Eyeballs, HTTP/3, benchmark-based socket tuning (7.8)**: explicitly
  future experiments; `http3`/`native_tls` are reported as disabled features.

## Suggested next increment

1. `new_jobs_only` config-snapshot refactor (above).
2. Push to GitHub, confirm CI matrix is green on all three platforms, then
   add the real-binary version matrices and remaining tool-dependent
   fixtures.
3. Criterion soak fixtures for the new split/hedge paths (the deterministic
   loopback fixtures exist; a long-running soak belongs in nightly CI).
