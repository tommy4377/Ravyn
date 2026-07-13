# Ravyn Library Features — Implementation Report

Date: 2026-07-13

## Scope

This implementation completes the fifteen library-oriented features defined in
`LIBRARY_FEATURES_PLAN.md`. The plan was also compared against the fifteen
feature ideas discussed before the plan was written; no idea from that list was
left outside the implementation plan.

| Feature | Status | Primary implementation |
|---|---|---|
| Automatic Ravyn library | Implemented | `config.rs`, `services/library/root.rs` |
| Automatic file categorization | Implemented | `services/library/category.rs` |
| Download presets | Implemented | `storage/presets.rs`, `services/presets.rs` |
| Advanced filename templates | Implemented | `services/library/template.rs` |
| Persistent download library | Implemented | migration 0019, `storage/library.rs` |
| Duplicate detection | Implemented | library duplicate API and `services/dedup.rs` |
| Library import | Implemented | `services/library/scan.rs` |
| Download trash | Implemented | `services/library/trash.rs` |
| Download basket | Implemented | migration 0020, `storage/basket.rs` |
| User profiles | Implemented | migration 0020, `storage/profiles.rs` |
| Download trust score | Implemented | `services/trust.rs` |
| File relocation detection | Implemented | `services/library/scan.rs` |
| Local cache reuse | Implemented | `core/lifecycle.rs` |
| Automatic cleanup policies | Implemented | `services/library/cleanup.rs`, `core/dispatcher.rs` |
| Personal statistics | Implemented | `storage/library.rs` |

## Automatic library behavior

Ravyn now creates an organized library during startup. The effective root is
resolved in this order:

1. `--library-root` / `RAVYN_LIBRARY_ROOT`;
2. `<explicit download directory>/Ravyn`;
3. `<current user's home>/Downloads/Ravyn`;
4. `<data directory>/downloads/Ravyn` as a final platform fallback.

The layout is:

```text
Ravyn/
├── Downloads/
├── Videos/
├── Music/
├── Documents/
├── Images/
├── Archives/
├── Torrents/
├── Playlists/
├── Temporary/
└── Trash/
```

Startup first creates only the storage directories needed to open SQLite. The
library layout is created only after persistent settings are loaded, preventing
an unused default `Downloads/Ravyn` tree from appearing when a saved custom
root exists.

## Architecture decisions

### Classification

The classifier uses operator overrides first, then the visible extension, MIME
information, and bounded magic-byte inspection. No new MIME or directory-walk
crate was added. Category names are stable serialized values and map to stable
directory names. Jobs without an explicit destination carry a server-owned
marker in their persisted options. For direct HTTP primary outputs, completion-
time content detection can correct an initially generic destination (for
example, an extensionless PDF from `Downloads` to `Documents`) while explicit
user destinations and explicit post-processing moves remain untouched.

### Persistent identity

`library_entries` records source identity, mirrors, SHA-256, size, path,
category, MIME type, media/torrent metadata, tags, trust data, state, import
origin, and timestamps. A partial unique index reserves each non-trashed path
while allowing an old trashed record and a newly downloaded file to retain the
same logical original path.

### Safe local reuse

Cache reuse only runs for `reuse_existing` or `skip` jobs that provide an
expected SHA-256. Ravyn validates confinement, rejects symlinks, checks the
stored size, re-hashes the current file, and only then materializes it by hard
link or copy. A successful reuse registers an output, indexes the new path,
marks the job complete, and increments saved-bandwidth counters.

### Trash and purge consistency

Trash and restore moves include compensating filesystem rollbacks if the
corresponding database update fails. Permanent purge stages the payload under
`Temporary/purge` before deleting the database record; a database failure moves
the file back. A final staged-file deletion failure is logged and left for the
normal cleanup policy rather than resurrecting an already purged record.

### Presets and profiles

Preset values fill request fields only when the request still carries its
default or omitted value. Explicit request values win. Filename templates
produce an optional safe relative subdirectory plus filename. Profiles are
deterministic overlays on the startup configuration and are activated together
with their merged persistent settings in one SQLite transaction.

### Import and relocation

Imports and relocation scans are bounded by depth and entry count, ignore
symlinks, stay within configured Ravyn output roots, and exclude internal Trash
and Temporary directories. Relocation repair only changes missing entries after
matching the full SHA-256 of a discovered regular file.

### Trust

Trust reports are advisory and factor-by-factor explainable. They cover HTTPS,
an explicitly supplied real TLS result, checksum availability and verification,
known mirrors, metadata consistency, and optional Ed25519 signatures. Job-based
reports leave TLS certificate validity unknown because the persisted job record
does not retain handshake telemetry. Ed25519 verification uses `ed25519-dalek`
2.2 strict verification over the raw 32-byte SHA-256 digest.

## Database changes

### Migration 0019

Adds:

- `library_entries`;
- indexed hash, size, category, state, filename, date, and job lookup;
- a partial unique live-path index;
- `stat_counters`;
- `library_settings` for cleanup policy JSON.

### Migration 0020

Adds:

- `download_presets`;
- `user_profiles` with one active profile enforced by a partial unique index;
- `basket_items` with unique stable positions.

All migrations are additive and leave existing job tables unchanged.

## API changes

The router and generated OpenAPI inventory now include:

- persistent library search, detail, duplicate candidates, trash/purge,
  restore, import status, verification, and relocation;
- template preview;
- preset CRUD;
- basket CRUD, reorder, clear, and start;
- profile CRUD and activation;
- trust preview and per-job trust report;
- cleanup policy get/update and manual execution;
- personal statistics.

`CreateJob` gained the additive optional `preset_id` field. `DuplicatePolicy`
gained `skip` and `overwrite`.

## Security considerations

- Every filesystem operation is confined to the configured download or library
  root through the existing output-path validator.
- Symlinks are skipped or rejected for import, reuse, trash, restore, purge,
  verification, relocation, and output indexing.
- Templates are relative, bounded, traversal-resistant, and sanitize every
  generated segment.
- SHA-256 values are validated before database persistence.
- Cache reuse re-verifies actual bytes instead of trusting stale database
  metadata.
- Import status errors are bounded; scans have absolute entry/depth ceilings.
- Preset, profile, basket, category-override, and search inputs have explicit
  size or count limits.
- Trust scores do not silently block downloads and never treat an untrusted
  valid signature as an operator-trusted signature.

## Performance considerations

- Completion and import hashing stream through the existing checksum service.
- Library lists are paginated and indexed; duplicate hash lookup is indexed.
- Directory traversal is breadth-first and bounded.
- Basket position assignment occurs inside the insert statement and reordering
  is transactional.
- Cleanup is supervised daily, skips missed ticks, and caps filesystem work.
- Exact cache reuse can avoid the network entirely, at the cost of a local
  verification hash before reuse.

## Test coverage added

The source contains tests for:

- extension, override, MIME, and magic-byte categorization;
- template rendering, missing variables, literal braces, and traversal denial;
- library upsert/search/hash lookup, invalid hashes, duplicate candidates,
  state transitions, and counters;
- concurrent basket insertion and dense reorder/delete positions;
- preset CRUD and case-insensitive name uniqueness;
- exclusive and atomic profile activation;
- import, missing detection, and relocation repair;
- trash/restore and reuse of an old trashed logical path;
- cleanup of temporary, cache, trash, and log data;
- trust scoring and real Ed25519 signature verification;
- completion-time content-based physical organization, persistent library
  indexing, and verified local cache reuse.

## Verification performed in this environment

- All 20 SQL migrations were applied in order to a fresh SQLite database.
- SQLite behavior for one trashed plus one active row at the same logical path
  was exercised, and two live rows were correctly rejected.
- The Axum router and OpenAPI table contain exactly 128 unique method/path
  pairs with no missing or duplicate operation.
- Every OpenAPI schema reference resolves to a defined schema.
- Every Rust source file parses with zero tree-sitter Rust syntax errors.
- All `CreateJob` literals were checked for the new `preset_id` field.

The environment does not contain `cargo`, `rustc`, or `rustfmt`. Therefore this
delivery does not claim that the new snapshot passed compilation, rustfmt,
Clippy, or executable tests. The complete gate in `AGENTS.md` remains mandatory
before merge and release.

## Known limitations

- Duplicate candidate search supports exact SHA-256, size, and
  case-insensitive filename evidence. Fuzzy content comparison is deliberately
  not used for automatic reuse because it cannot prove byte identity.
- Cache reuse requires an expected SHA-256. Ravyn does not reuse by filename or
  size alone.
- Imported generic files receive core filesystem metadata; rich media and
  torrent metadata are retained when provided by their adapters but are not
  synthesized by a separate metadata extractor during generic import.
- Preset scheduler, rules, and arbitrary metadata payloads are persisted for
  orchestration and future UI workflows. Core job creation directly applies
  destination, priority, speed, duplicate policy, download options, tags,
  post-processing, and filename templates.
- Profile fields classified as restart-required are persisted immediately but
  only affect components created after restart. Existing live fields follow the
  same settings contract as the rest of Ravyn.
- The in-memory import status represents the current process and one active
  import slot. Import results are audited, but resumable import checkpoints are
  not persisted across process restarts.
- Library indexing failure is logged as `LIBRARY_INDEX_FAILED` without changing
  an otherwise successful download into a failed transfer. A later import can
  backfill the file.
