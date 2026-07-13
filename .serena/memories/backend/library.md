# Library features

## Architecture
Fifteen-feature persistent download library implemented 2026-07-13. Migrations 0019 and 0020 are additive-only; no existing tables modified.

### New modules
- `src/services/library/` — category.rs (extension+MIME+magic classification), root.rs (layout creation), template.rs (filename templates with variable expansion and traversal denial), scan.rs (bounded BFS import and SHA-256-based relocation repair), trash.rs (trash/restore/purge with compensating filesystem rollbacks), cleanup.rs (retention policies for temporary/cache/trash/log data)
- `src/services/presets.rs` — preset application with explicit-wins merge order, template rendering for subdirectory paths
- `src/services/trust.rs` — advisory trust scoring with 7 factors, Ed25519-dalek 2.2 strict signature verification over raw SHA-256 digests
- `src/storage/library.rs` — upsert with partial unique live-path index, paginated search with dynamic QueryBuilder, SHA-256 validation before persist, stat counters
- `src/storage/presets.rs` — CRUD with case-insensitive unique names
- `src/storage/basket.rs` — atomic position allocation via self-join INSERT, transactional reorder
- `src/storage/profiles.rs` — atomic activation via transaction (deactivate all, activate target), settings+profile persisted in one operation
- `src/api/routes/library.rs` — all new route handlers for library, presets, basket, profiles, trust, cleanup, statistics

### Migrations
- `0019_library.sql`: `library_entries` (21 columns, 8 indexes including partial unique live-path index), `stat_counters`, `library_settings` singleton
- `0020_presets_profiles_basket.sql`: `download_presets` (unique NOCASE name), `user_profiles` (unique single-active-profile index), `basket_items` (unique dense positions)

### API additions (~30 new routes)
- Library: GET/DELETE `/v1/library`, GET `/v1/library/duplicates`, POST `/v1/library/{id}/restore`, GET+POST `/v1/library/import`, POST `/v1/library/verify`, POST `/v1/library/relocate`
- Templates: POST `/v1/templates/preview`
- Presets: CRUD `/v1/presets`
- Basket: GET+POST+DELETE `/v1/basket`, PATCH+DELETE `/v1/basket/{id}`, POST `/v1/basket/reorder`, POST `/v1/basket/start`
- Profiles: CRUD `/v1/profiles`, POST `/v1/profiles/{id}/activate`
- Trust: POST `/v1/trust/preview`, GET `/v1/jobs/{id}/trust`
- Cleanup: GET+PUT `/v1/system/cleanup-policies`, POST `/v1/system/cleanup`
- Statistics: GET `/v1/statistics`

### Config additions
- `--library-root` / `RAVYN_LIBRARY_ROOT` — explicit library root (fallback: `<download-dir>/Ravyn` or `~/Downloads/Ravyn`)
- `--library-auto-organize` — auto-classify and move primary outputs to category directories (default: true)
- `library_category_overrides` — persistent extension→category mapping (max 521 extensions)

### Key invariants
- Library indexing on download completion is non-fatal (`LIBRARY_INDEX_FAILED` audit log, never aborts the job)
- Cache reuse requires expected SHA-256; validates confinement, rejects symlinks, re-hashes before materialization
- Trash/restore/purge include compensating filesystem rollbacks on DB failure
- Import and relocation scans are bounded by depth (max 256) and entry count (max 1M), skip symlinks, exclude Trash/Temporary
- Templates are relative, traversal-resistant, every segment sanitized via `filename::sanitize`
- DuplicatePolicy gained `skip` and `overwrite` variants (serde `rename_all = "snake_case"`)
- CreateJob gained `preset_id: Option<Uuid>` with `#[serde(default)]`
- Dispatcher spawns a library-cleanup task on 24h interval with cancellation token

### Library layout
```
Ravyn/
├── Downloads/  ├── Videos/  ├── Music/  ├── Documents/  ├── Images/
├── Archives/  ├── Torrents/  ├── Playlists/  ├── Temporary/  └── Trash/
```

### Test coverage
Unit: categorization, templates, library CRUD/search/hash/state transitions, concurrent basket, preset CRUD, profile activation, trash/restore round-trip, cleanup retention, trust scoring, Ed25519 verification.
Integration: auto-organization, library entry recording with hash, verified cache reuse without second transfer.
