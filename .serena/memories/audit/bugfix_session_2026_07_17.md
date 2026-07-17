# Bug-list resolution session — 2026-07-17 (machine: Administrator)

## Scope
Worked the user's bug report/feature list against commits 059e90c/5482d8a/bad3bf5. Verified committed fixes live in an isolated QA profile (`%TEMP%\ravyn-qa-retry`, RAVYN_DATA_DIR + RAVYN_ALLOW_PRIVATE_NETWORK=true, controlled flaky Python HTTP server on 127.0.0.1:48123) and fixed newly found defects. No commit/push.

## Verified live (screenshots in target/qa-*.png)
- Retry: failed job at frozen 25% → UI Retry → same row completes; DB shows 1 job/1 output, no duplicates (integration test also asserts this).
- Setup follows dark system theme; deterministic setup→main handoff; dev-mode integration boxes unchecked.
- Downloads layout clean at 800×560 / 1100×720 / 1720×960 (scrollbar-gutter + row-stretch fixes).
- New Rule editor + Dropdown listbox fully styled in top layer; quality menu shares the same Dropdown.
- Extension gates: eslint/web-ext 0 issues, 25 tests, typecheck, XPI. Overlay X + badge clearing verified in code.

## New defects found & fixed this session
1. **Compact window blank white + IPC stall**: `open_compact_window` was a sync Tauri command creating a webview → wry#583 deadlock on Windows. Made async + `decorations(false)` (CompactApp draws its own titlebar). Verified: renders live progress, auto-closes ~2.5s after transfers finish.
2. **Completed jobs stuck at 94%**: last periodic progress flush lags completion; jobs rendered stale percent after reload. Completion now snaps counters (src/core/execution.rs) + migration `0027_completed_progress_backfill.sql` backfills history; assertions added to retry integration test. Verified live (rows went 94%→100% after migration).
3. **Show in Explorer → Documents (still)**: second root cause beyond verbatim prefix — std::process quotes the whole `/select,path with spaces` arg; Explorer misparses → Documents. Fixed with `raw_arg(/select,"<path>")` in shell_paths.rs + unit test; verified the exact argument shape opens the right folder.
4. **Default build broken**: committed capabilities referenced `mcp-bridge:default` while plugin is feature-gated → manifest validation fails without `--features mcp-automation`. Removed from committed capabilities; build.rs now generates `capabilities/mcp-automation.gen.json` (gitignored) only when the feature is on. `withGlobalTauri` back to false; QA builds use `TAURI_CONFIG='{"app":{"withGlobalTauri":true}}' cargo build -p ravyn-desktop --features mcp-automation` (documented in CLAUDE.md).
5. **move_root tests environment-sensitive**: fixture compared canonical (long-name) persisted paths against `%TEMP%` 8.3 short-name (`ADMINI~1`) paths. Fixture now canonicalizes its base; product code was correct.
6. Raw setup step label `restore_persisted_integration` → "Restoring the verified installation" (InstallStage STEP_LABELS).
7. Extension lint error (unused `_status`) → `clearBadge()`; dropped unused ConnectionStatus import.
8. Migrated last native `<select>` (CategoryOverridesEditor) to shared Dropdown; verified live.

## Tooling notes
- tauri-mcp bridge needs the QA build above; bridge JS relies on `window.__TAURI__`. Bridge dies if a webview is created from a sync command (fixed) and sessions must be stop/start-ed after app restarts.
- Native window enumeration/capture scripts used when the bridge was down (scratchpad enum_windows.ps1 / capture_hwnd.ps1 patterns).
- Extension Prettier failures on this machine are CRLF checkout artifacts (core.autocrlf=true), not code issues.

## Final gates (this session)
cargo fmt/clippy(-D warnings)/test --all-targets (252+12) green; frontend svelte-check 0/0, 119 tests, production build green; extension lint/tests/typecheck/package green; release build gate run at session end.

## Installer replacement (Option A, user-approved, implemented same session)
- MSI/NSIS dropped: `bundle.active:false`, targets removed, `src-tauri/windows/hooks.nsh` + `firefox-native-host.wxs` git-rm'ed. Distribution is the single self-installing `Ravyn.exe` (in-app setup performs per-user installation; UninstallString already `Ravyn.exe --uninstall`; Firefox host registration runtime-only).
- Updater: helper replaces installed binary in place (Copy-Item, no `/S`), refreshes `DisplayVersion`; module docs + new test `helper_script_replaces_the_binary_in_place_without_an_installer`.
- New `webview_runtime.rs`: WebView2 startup guard (MessageBox + download link) since no installer bootstraps the runtime.
- Release workflow: direct `npx @tauri-apps/cli build --no-bundle`, explicit signtool signing, publishes `Ravyn.exe` + zip, new exe smoke test (readiness + native-host register/unregister round trip); lifecycle harness reworked without the mock installer (also fixed cargo-stdout polluting its return value).
- Docs synced: README, docs/APP_UPDATES.md, docs/RELEASE_CHECKLIST.md, mem desktop/core.

## Open items
- Component-update flicker: reactivity fixes committed; not re-reproduced live here (QA profile has no managed components). Torrent status: engine-init retry fixes tested at unit level; live rqbit pass was done in the 2026-07-16 audit.
- Installer replacement (MSI/NSIS → self-extracting to Program Files): NOT started; conflicts with install-dir==data-dir + binaries-only rollback architecture; needs user decision.
