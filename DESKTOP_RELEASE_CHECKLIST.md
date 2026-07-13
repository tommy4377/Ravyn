# Desktop Release Checklist

> Companion to `DESIGN_PLAN(2).md` §26.4. Tracks release-engineering work that runs in parallel with frontend vertical slices and must not block them. Status here reflects the backend/desktop-shell state as observed while building the Downloads slice; it does not reflect any change made during that work — this document is read-only observation, since backend/`src-tauri` code is explicitly out of scope for the frontend track.

| Item | Status | Notes |
|---|---|---|
| Frontend CI | Not verified this pass | No CI config was inspected/changed. `npm run check`, `npm test`, and `npm run build` all pass locally as of this update. |
| Tauri dev workflow | **Broken** (newly observed) | `cargo tauri dev` run from `src-tauri/` fails: its `beforeDevCommand` (`npm run dev --prefix ../frontend`) resolves against a different working directory than expected, producing `ENOENT` for `.../GitHub/frontend/package.json` (missing the `Ravyn` path segment). Worked around for manual verification by running `npm run dev` (frontend) and `cargo run --no-default-features` (src-tauri) separately, bypassing the broken hook. This is a `src-tauri/tauri.conf.json` fix and was **not** made, per this session's scope (backend/shell untouched). |
| Tauri release build / setup artifact | Not attempted this pass | Out of scope; `TODO.md` already tracks this as release-critical. |
| Binary signing | Not attempted this pass | Tracked in `TODO.md`. |
| Application updater (fetch/verify/replace/rollback) | Not attempted this pass | Tracked in `TODO.md` as partial. |
| Remote signed component manifests | Not attempted this pass | Tracked in `TODO.md`; 7-Zip has no verified artifact yet (also called out in `DESIGN_PLAN(2).md` §15.4). |
| Portable mode | Not attempted this pass | Tracked in `TODO.md`. |
| Tauri command restrictions by window/state | Not attempted this pass | Confirmed still open: `src-tauri/capabilities/default.json` grants all 5 commands to both `setup` and `main` windows with no per-window restriction. Relevant to the frontend only in that the main window currently could (but does not, in the code written this pass) call setup-only commands like `apply_windows_integration`. |
| Desktop CI (build the frontend before packaging) | Not attempted this pass | Tracked in `TODO.md`. |
| Windows clean-machine E2E tests | Not attempted this pass | Requires a packaged build; out of scope for a source-level frontend pass. |
| Uninstall | Reported complete in `TODO.md` | Not re-verified this pass. |

## What *was* verified this pass (frontend-only, source-level)

- `npm run check` (svelte-check + TypeScript): 0 errors, 0 warnings across 242 files.
- `npm test` (vitest): 55/55 passing, including new coverage for the Downloads slice.
- `npm run build` (vite production build): succeeds, ~156 KB JS / ~34 KB CSS before gzip.
- Manual Tauri smoke test: attempted via the workaround above; see the session notes for the outcome achieved before this document was written (a real running-app screenshot pass, not just a build check, is the bar per `AGENTS.md`'s frontend completion checklist).
