# Firefox Extension (extension/)
- Firefox Manifest V3 browser extension for the Ravyn download manager, version synchronized with the application (currently 0.2.0).
- Build system: TypeScript, web-ext for packaging/lint, vitest for tests, deterministic zip via Python `scripts/deterministic-zip.py`. Build scripts in `scripts/`: `build-firefox.mjs`, `package-firefox.mjs`, `package-source.mjs`, `verify-packages.mjs`, `clean.mjs`.
- Extension ID: `firefox-extension@ravyn.app`. Native messaging host: `com.ravyn.download_manager`.

## Architecture
- `src/background/index.ts` — main background worker entrypoint.
  - **Downloads**: `downloads/interceptor.ts` (download creation listener), `downloads/delegation.ts` (delegation to native messaging), `downloads/eligibility.ts` (rule-based/ask/all-compatible interception modes), `downloads/state-machine.ts` (download state machine with tests).
    - Non-obvious invariants added 2026-07-18 (full audit pass — `mem:audit/deep_dive_2026_07_18` doesn't exist, this is folded in here directly):
      - `interceptor.ts` persists which download ids it paused for handoff to `browser.storage.local["ravyn.pendingPausedDownloadIds"]` (`markPending`/`clearPending`). `resumeOrphanedPausedDownloads()` (run on every background-script start) only force-resumes ids in that set — NOT every `{paused:true, state:"in_progress"}` download — because a plain paused-state search can't distinguish an interrupted Ravyn handoff from a download the user paused manually for their own reasons.
      - `DelegationRegistry` (`delegation.ts`) has a `claim(url)`/`release(url)` pair in addition to `remember`/`contains`. `interceptor.handle()` calls `claim()` right before the native `create_download` request, after all async eligibility/rule/confirm checks — this closes a race where two `onCreated` events for the same URL (double-click, or a page firing near-simultaneous requests) could both pass `contains()` (neither has an entry yet) and both hand off, creating two Ravyn jobs for one click.
      - `NetworkObserver.pending` (`network/observer.ts`) is swept every 60s, evicting entries older than 5 minutes — a request that never fires `onCompleted`/`onErrorOccurred` (long-lived SSE/EventSource kept open for a page's lifetime) would otherwise sit there forever while `networkObservation` is enabled.
      - `MediaOverlayController`'s `MutationObserver` (`content/media/overlay.ts`) is debounced 200ms — `attach()` appends a control host element into the document, which is itself a `childList` mutation that would otherwise re-trigger the same observer, turning attaching N controls into a self-inflicted cascade of full-document `querySelectorAll` scans.
      - `BoundedMutationScanner.start()` (`content/scanner/mutation-observer.ts`) now resets its mutation counter — previously, once a page tripped the `maximumMutations` cap and called `stop()`, any future `start()` (e.g. the user re-toggling "Monitor page") would immediately re-trip the same already-exceeded counter, silently disabling monitoring for the tab's remaining lifetime despite the UI showing it re-enabled.
  - **Menus**: `menus/register.ts` (context menu setup), `menus/handlers.ts` (menu click handlers for link/image/media/page downloads).
  - **Native**: `native/client.ts` (native messaging stdin/stdout client), `native/capabilities.ts` (capability negotiation with the desktop host).
  - **Network**: `network/observer.ts` (optional webRequest observer), `network/classifier.ts` (MIME-based resource classification), `network/cache.ts` (response cache).
  - **Rules**: `rules/evaluator.ts` (rule evaluation engine), `rules/cache.ts` (rule cache).
- `src/content/index.ts` — content script entrypoint.
  - **Media**: `media/overlay.ts` (download overlay on media elements), `media/source-collector.ts` (HLS/DASH source extraction).
  - **Scanner**: `scanner/dom-scanner.ts` (DOM resource scanning), `scanner/mutation-observer.ts` (dynamic DOM change detection), `scanner/normalizer.ts` (URL normalization), `scanner/srcset.ts` (srcset parsing).
- `src/shared/` — shared types: `contracts.ts` (native messaging protocol contracts), `errors.ts`, `i18n.ts`, `logger.ts`, `settings.ts`, `urls.ts`, `validation.ts`.
- `src/confirmation/index.ts` — download confirmation page logic.
- `src/options/index.ts` — options page logic.
- `src/popup/index.ts` — toolbar popup logic.
- `src/sidebar/index.ts` — batch resource sidebar logic.

## Pages (static/)
- `static/confirmation/index.html` — download confirmation dialog.
- `static/options/index.html` — extension options/settings.
- `static/popup/index.html` — toolbar popup.
- `static/sidebar/index.html` — batch resource sidebar.
- Common styles in `static/common.css`; locale strings in `static/_locales/{en,it}/messages.json`.
- Icons: `static/icons/ravyn-{16,32,48,96}.png`.

## Manifests
- `manifests/base.json` — shared manifest properties (version, permissions, host_permissions).
- `manifests/firefox.json` — Firefox-specific manifest overrides.
- Build merges base + firefox into the final `manifest.json`.

## Tests & Fixtures
- Colocated `*.test.ts` files in `src/` (vitest).
- `test-pages/` — HTML fixtures (direct.html, dynamic.html, iframe.html, frame-child.html, index.html) served by `test-pages/server.mjs` for integration tests.

## CI
- Validated in `.github/workflows/backend-ci.yml` (firefox-extension job): lint, check, deterministic package verification.
- Signed in `.github/workflows/release.yml` (firefox-extension job): AMO unlisted signing for tagged releases, checksums, GitHub attestations.
- Version synchronization enforced: extension version must match app/desktop/frontend versions.

## Docs
- `docs/FIREFOX_EXTENSION.md` — high-level extension overview.
- `docs/FIREFOX_EXTENSION_IMPLEMENTATION_PLAN.md` — original implementation plan.
- `docs/FIREFOX_EXTENSION_IMPLEMENTATION_REPORT.md` — implementation report.
- `extension/PRIVACY.md` — privacy policy.
- `extension/THREAT_MODEL.md` — threat model.
- `extension/AMO_SUBMISSION.md` — AMO submission guide.
- `extension/README.md` — extension-specific README.
