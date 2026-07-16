# Firefox Extension (extension/)
- Firefox Manifest V3 browser extension for the Ravyn download manager, version synchronized with the application (currently 0.2.0).
- Build system: TypeScript, web-ext for packaging/lint, vitest for tests, deterministic zip via Python `scripts/deterministic-zip.py`. Build scripts in `scripts/`: `build-firefox.mjs`, `package-firefox.mjs`, `package-source.mjs`, `verify-packages.mjs`, `clean.mjs`.
- Extension ID: `firefox-extension@ravyn.app`. Native messaging host: `com.ravyn.download_manager`.

## Architecture
- `src/background/index.ts` — main background worker entrypoint.
  - **Downloads**: `downloads/interceptor.ts` (download creation listener), `downloads/delegation.ts` (delegation to native messaging), `downloads/eligibility.ts` (rule-based/ask/all-compatible interception modes), `downloads/state-machine.ts` (download state machine with tests).
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
