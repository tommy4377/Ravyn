# Ravyn Library Features — Research and Implementation Plan (2026-07-13)

Fifteen features around a persistent, organized download library. This
document is the Phase 1 research summary and Phase 2 implementation plan;
`RAVYN_MASTER_PROJECT_DOCUMENT.md` remains the canonical status ledger.

## Phase 1 — Research summary

### Existing solutions surveyed
- **JDownloader / Free Download Manager**: category folders driven by
  extension maps with user overrides; presets ("download templates");
  post-download library views are thin history layers, not hash-indexed.
- **beets / Lidarr / Sonarr / Radarr**: filename templates with metadata
  variables (`{artist}/{album}/{track}`), import scanners that hash and
  dedupe, relocation repair by content identity. Their template systems
  (path format strings with per-segment sanitization) are the model for F4.
- **Firefox/Chromium download managers**: MIME-then-extension category
  routing; content sniffing only as a fallback (MIME from the server is
  spoofable; extension is user-visible truth for local organization).
- **freedesktop.org Trash spec**: trash keeps the original path + deletion
  date next to the payload so restore never guesses; we mirror that with
  database state instead of `.trashinfo` files.

### Crate evaluation and the dependency decision
Candidates: `infer` (magic bytes), `mime_guess` (extension→MIME),
`walkdir` (recursive scan). Decision: **no new dependencies.**
- The repository already ships an extension→MIME table and a bounded
  depth/entry-capped directory scan (`core::metrics::temporary_disk_usage`);
  the categorization needs ~20 magic signatures, each a documented constant
  from the format specifications (PNG, ZIP, PDF, gzip, 7z, RAR, EBML/MKV,
  ISO-BMFF `ftyp`, ID3/MPEG audio, FLAC, OGG, ELF, PE/MZ, TAR ustar).
- `deny.toml` gates the supply chain; three new crates (plus transitive
  trees) for functionality expressible in ~150 audited lines is a worse
  long-term maintainability trade.
- Context7 MCP was unavailable during this session (platform-side classifier
  outage; three retries). Because the design adds no third-party code, no
  undocumented external API is being relied on. If a later change adopts one
  of these crates, fetch its current docs through Context7 first.

### Security implications
- All new filesystem surfaces (library root, import, trash, relocation) must
  go through the existing output-root confinement (`security::validate_output_path`
  / `validate_regular_file_under`); imports and relocation scans accept only
  directories under the library root or download root.
- Server-supplied MIME is advisory; category routing prefers extension and
  local content sniffing, never remote headers alone.
- Templates render into single path segments with the existing
  `filename::sanitize`; `..`, separators, and reserved names cannot escape.
- Trust score is explicitly explainable and advisory — it never blocks a
  download by itself.

### Performance implications
- Library search: indexed columns (sha256, size, category, state, filename
  NOCASE, downloaded_at) with bounded `LIKE` on the filename; no FTS5 (the
  bundled SQLite build's FTS availability is not part of our contract, and
  a local library at 10⁵ rows is well inside btree+LIKE budgets).
- Completion-time hashing streams with the existing 1 MiB-buffer checksum
  service; import hashing is the same code path and cancellation-aware.
- Scans are depth- and entry-bounded like the existing temp-disk scan.

## Phase 2 — Implementation plan

### Dependency graph and order (minimizes breaking changes)
```
T1 F1+F2+F5  library root, categorization, library entries   (migration 0019)
T2 F4        filename templates + preview                     (no migration)
T3 F6+F13    duplicate detection + local cache reuse          (uses T1)
T4 F8        trash + restore + purge                          (uses T1)
T5 F7+F12    import + relocation repair                       (uses T1, T4)
T6 F3+F9+F10 presets, basket, profiles                        (migration 0020)
T7 F11       trust score                                      (uses T1, host profiles)
T8 F14+F15   cleanup policies + statistics                    (uses all)
T9           audit, refactor, docs, report
```
Each tranche is one commit, fully tested before the next starts.

### Modules
- `src/services/library/` — categorization (`category.rs`: extension map +
  magic sniffing), root layout (`root.rs`), templates (`template.rs`),
  import/relocation scanner (`scan.rs`), trash (`trash.rs`).
- `src/services/trust.rs` — trust report computation.
- `src/storage/library.rs`, `src/storage/presets.rs`, `src/storage/basket.rs`,
  `src/storage/profiles.rs` — repository impls.
- `src/api/routes/library.rs` — all new route handlers.
- `src/core/lifecycle.rs` — preset application, destination resolution,
  template rendering, extended duplicate policies.
- `src/core/execution.rs` — record library entry (+hash) on completion;
  cache-reuse short circuit.
- `src/core/maintenance.rs` — cleanup policies.

### Migrations
- `0019_library.sql`: `library_entries`
  (id, job_id NULL, source_url, mirrors_json, sha256 NULL, size_bytes NULL,
  path, filename, category, mime_type NULL, media_metadata_json,
  torrent_metadata_json, tags_json, trust_json NULL, state
  'active'|'trashed'|'missing', trash_path NULL, imported flag,
  downloaded_at, created_at, updated_at) with the indexes listed above,
  plus `stat_counters` (key TEXT PRIMARY KEY, value INTEGER) for saved-
  bandwidth and duplicate-avoidance counters.
- `0020_presets_profiles_basket.sql`: `download_presets` (id, name UNIQUE,
  payload_json, timestamps), `user_profiles` (id, name UNIQUE,
  settings_patch_json, default_preset_id NULL, timestamps),
  `basket_items` (id, position, request_json, preset_id NULL, timestamps).
Both are additive; existing tables are untouched. Rollback strategy:
restore the automatic pre-migration backup (migrations are forward-only by
policy); within a tranche, `git revert` of the tranche commit is safe
because no released migration is rewritten.

### Configuration changes
- `--library-root <path>` (env `RAVYN_LIBRARY_ROOT`), optional. When set,
  bootstrap creates `Downloads/ Videos/ Music/ Documents/ Images/ Archives/
  Torrents/ Playlists/ Temporary/ Trash/` under it and category routing is
  enabled for jobs without an explicit destination.
- `--library-auto-organize <bool>` default `true` (only meaningful when the
  root is set) — mirrored into persistent settings as live-reloadable.
- Cleanup policies persisted as their own settings row (JSON), not new CLI
  flags: `{temporary_max_age_days, trash_retention_days, log_retention_days,
  cache_retention_days}` with a supervised daily task plus manual endpoint.

### API changes (all additive, `/v1`)
- Library: `GET /v1/library` (search: `q`, `category`, `state`, `tag`,
  `mime`, date range, pagination), `GET /v1/library/{id}`,
  `DELETE /v1/library/{id}` (`?mode=trash|purge`), `POST
  /v1/library/{id}/restore`, `POST /v1/library/import` +
  `GET /v1/library/import` (status), `POST /v1/library/relocate`,
  `POST /v1/library/verify` (mark missing).
- Templates: `POST /v1/templates/preview`.
- Presets: CRUD `/v1/presets`; `CreateJob.preset_id` applies one.
- Basket: `GET/POST /v1/basket`, `PATCH/DELETE /v1/basket/{id}`,
  `POST /v1/basket/reorder`, `POST /v1/basket/start`, `DELETE /v1/basket`.
- Profiles: CRUD `/v1/profiles`, `POST /v1/profiles/{id}/activate`.
- Trust: `POST /v1/trust/preview`, `GET /v1/jobs/{id}/trust`.
- Cleanup: `GET/PUT /v1/system/cleanup-policies`, `POST /v1/system/cleanup`.
- Statistics: `GET /v1/statistics`.
- `DuplicatePolicy` gains `skip` and `overwrite` (serde-additive).
All registered in the OpenAPI operation inventory.

### Testing plan
- Unit: categorization (extension/magic/MIME precedence), template parsing/
  sanitization/preview, trust factor math, cleanup retention math, basket
  ordering, preset merge order (explicit > preset; rules unchanged).
- Repository: library CRUD/search filters, trash state transitions, presets/
  basket/profiles CRUD, stat counters.
- Integration (in-crate with local servers, existing pattern): create→
  complete→library entry recorded with hash and category; duplicate skip/
  reuse/overwrite/duplicate; cache reuse completes without network re-fetch;
  import scan of a seeded directory; relocation repair after a manual move;
  trash → restore round trip.
- Full gate per tranche (fmt, check, clippy -D warnings, all tests, fuzz
  check, release build at the end).

### Estimated complexity
T1 high; T3, T5, T6 medium-high; T2, T4, T7, T8 medium; T9 low.

### UI impact (future frontend)
Every feature is exposed as JSON endpoints with stable names; search uses
the same pagination envelope as existing lists, so the future frontend can
reuse its table/pager components. Trust reports return factor-by-factor
explanations designed for direct rendering.
