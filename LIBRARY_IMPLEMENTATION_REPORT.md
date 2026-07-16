# Library implementation report

The organized Ravyn library is implemented as a persistent backend domain, not
as a frontend-only folder view.

Implemented capabilities include categorized library roots, automatic routing,
MIME and bounded magic-byte reclassification, explicit destination precedence,
extension overrides, persistent records, SHA-256 identity, verified cache reuse,
duplicate lookup, filename template preview, presets, profiles, basket ordering
and batch start, bounded folder import, missing-file verification, relocation
repair, managed trash/restore/purge, retention cleanup, trust reports, and
storage/activity/speed/saved-bandwidth statistics.

The frontend exposes library browsing, search/filtering, imports, basket,
duplicates, trash, trust information, statistics, category overrides, and
relevant settings. Heavy library UI code is loaded only when the section is
opened.

Validation is provided by route/OpenAPI/frontend contract parity, repository and
service tests, migration application in the static audit, and the normal Rust CI
gate. The browser extension is not part of this report.
