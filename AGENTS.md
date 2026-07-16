# Repository working agreement

- Keep source comments and user-facing application copy in English.
- Treat `openapi.json` generation and frontend API types as one contract; run
  `python tools/static_source_audit.py` after route changes.
- Do not weaken HTTPS, signature, checksum, path, metadata-size, archive, SSRF,
  or capability checks to make a test pass.
- Keep managed component installation transactional and retain one verified
  rollback candidate.
- Add SQLite changes as a new ordered migration; never rewrite released
  migrations.
- Run the validation commands in `docs/RELEASE_CHECKLIST.md` before release.
- The browser extension is a separate future package and must not be mixed into
  desktop completion work unless explicitly requested.
