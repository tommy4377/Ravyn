# Ravyn Firefox Extension — Audit (2026-07-17)

Full review of `extension/src` (every module read end-to-end), plus a comparison
against the behavior of established download-manager extensions: IDM Integration
Module, Free Download Manager, DownThemAll!, and Video DownloadHelper. Items are
ordered by severity within each section. File references use current line
numbers on `master`.

Legend: 🔴 bug (incorrect behavior today) · 🟡 robustness/design gap · 🔵 improvement / feature gap vs. competitors · ⚪ optimization / polish.

---

## 1. Bugs

### 1.1 🔴 `update-settings` broadcast never reaches content scripts

`background/index.ts` → `broadcast()` uses `browser.runtime.sendMessage`, which
only reaches **extension pages** (popup, options). Content scripts are not
recipients of `runtime.sendMessage` in Firefox — they need
`browser.tabs.sendMessage(tabId, …)` per tab. The content script's
`update-settings` listener (`content/index.ts:52`) therefore never fires: after
the user changes overlay settings (disable overlays, size thresholds), every
already-open tab keeps the old behavior until reload.

**Fix**: in the `save-settings` handler, iterate `browser.tabs.query({})` and
`tabs.sendMessage` each tab (catch and ignore tabs without the content script),
or have content scripts read settings from `browser.storage.onChanged` instead
of push messages (preferred — one listener, no tab iteration, survives
background idling).

### 1.2 🔴 "Download original image" fails silently for relative `srcset` URLs

`content/index.ts::collectContext` returns raw `srcset` candidate strings
(`entry.trim().split(/\s+/, 1)[0]`) without resolving them against the document
base. A relative candidate like `/img/large.jpg` flows into
`menus/handlers.ts::create()`, where `normalizeUrl(payload.url)` (no base)
returns `null` and `create()` **silently returns**. The user right-clicks →
"Download original image" → nothing happens, no error.

**Fix**: resolve candidates in `collectContext` with
`new URL(candidate, document.baseURI).href` before returning them (the DOM
scanner already does this correctly via `normalizeResource(input.url, base)`;
`collectContext` is the only unresolved path).

### 1.3 🔴 Context-menu messages target the wrong frame for iframe content

`collectContext` and `scanTab` (`menus/handlers.ts:208–243`) call
`browser.tabs.sendMessage(tabId, …)` **without `frameId`**. The content script
runs with `all_frames: true`, so the message is delivered to every frame and
the returned value is arbitrary/top-frame. For an image inside an iframe,
`lastContextTarget` lives in the iframe's content-script instance, so
"Download original image" collects context from the wrong frame and falls back
to `directUrl` (or nothing).

**Fix**: pass `{ frameId: info.frameId }` as the third `sendMessage` option for
`collect-context`; for `scan-page`, either aggregate per-frame results with
`webNavigation.getAllFrames` or accept top-frame scanning explicitly and
document it. (`DownloadInterceptor` is unaffected; only menu flows.)

### 1.4 🔴 Menu-item and overlay failures are invisible to the user

`menus/handlers.ts::create()` awaits `native.request` but nothing catches a
rejection at the call sites; `registerMenuHandlers` wraps `handle()` in a bare
`void … .catch()`? — no, it does `void handle(…)` with **no catch**, so a
backend-offline click produces only an unhandled-rejection console line. Every
competitor surfaces this (IDM shows an error balloon; FDM shows a toast).

**Fix**: wrap the `native.request` calls in `handle()` with a catch that calls
the existing `notify()` helper (respecting `settings.notifications`), e.g.
"Ravyn is not running — the link was not added". The overlay got equivalent
feedback in this session (`overlay.ts::acknowledge`); menus still lack it.

### 1.5 🔴 Hard-coded English menu titles despite a localization pipeline

`menus/register.ts` passes literal English strings ("Download link with
Ravyn", …) while the extension ships `_locales/en` + `_locales/it` and
`shared/i18n.ts`. Italian users see English menus. All 24 titles should go
through `browser.i18n.getMessage` with new message keys in both locale files.

### 1.6 🟡 `subscribe_events` is a stub — event-driven cache invalidation is dead code

The native host answers `subscribe_events` with
`{ subscribed: true, transport: "request-refresh" }` and **never pushes
events** (`src-tauri/src/native_messaging.rs:229`). Consequently:

- `rules.invalidate()` on `rule.*` events (`background/index.ts:51`) never runs;
  rule changes in the app take up to the 10-minute TTL (`rules/cache.ts:6`) to
  reach the extension.
- `backend.connected` / `backend.disconnected` handling in
  `native/client.ts:171–175` never fires; status only updates via the 15 s
  heartbeat.
- The popup polls `get_download_summary` every 2 s as a workaround.

**Fix (backend + extension)**: implement real event push in the native host
(proxy the backend SSE stream over the native-messaging pipe), then drop the
2 s popup poll in favor of push, and keep the heartbeat only as a liveness
fallback. This is the single change that most improves popup smoothness and
rule responsiveness.

### 1.7 🟡 In-memory state does not survive event-page suspension

Firefox MV3 background scripts are event pages. `DelegationRegistry`,
`ResourceCache`, `RuleCache.snapshot`, and `DownloadInterceptor.confirmations`
are all plain in-memory maps. Today the open native-messaging port keeps the
page alive, but if the port is down (Ravyn not installed/running — a supported
state), the page can idle out and:

- delegation memory is lost → a retried download re-intercepts (2-minute window
  anyway, low impact);
- per-tab detected resources vanish → popup "Resources" empties;
- a pending confirmation promise disappears → the paused browser download stays
  paused forever (**worst case**).

**Fix**: persist the confirmation-pending set to `storage.session`, and on
background startup resume any paused downloads whose confirmation state was
lost. Consider `storage.session` for the resource cache if the port-down case
matters in practice.

### 1.8 🟡 "Ask" mode beats "always intercept" domains

`state-machine.ts::decideInterception` checks
`settings.interceptionMode === "ask"` **before** `forcedByDomain`. A domain the
user explicitly put in "always intercept" still prompts in ask mode. The
explicit per-domain override should win: check `forcedByDomain` first.

### 1.9 🟡 Multiple simultaneous confirmations open stacked popup windows

`DownloadInterceptor.confirm()` creates one `browser.windows.create` popup per
download with no queueing. A page that drops five files at once (common with
"download all" buttons) opens five overlapping windows. Queue confirmations, or
render one window listing all pending downloads.

### 1.10 🟡 `NetworkObserver` reports pre-redirect URLs

`observer.ts` records the URL at `onBeforeRequest` and never updates it on
redirect (`onBeforeRedirect` is not registered). CDN-redirected media (very
common) is catalogued under the original URL; downloading it through Ravyn then
re-follows the redirect — usually fine, but signed/expiring redirect targets
(S3 presigned URLs) will differ from what the page actually played.

**Fix**: register `onBeforeRedirect` and update `pending.url`.

### 1.11 ⚪ Minor correctness nits

- `menus/handlers.ts` `mediaSubtitles`: `url: info.pageUrl ?? directUrl ?? ""`
  can send an empty URL to the backend; guard and skip instead.
- `popup/index.ts::setBusy(false)` re-enables **all** buttons, including
  `download-selected` when the selection is empty (next `renderResources()`
  usually corrects it, but not on the `submitUrl` path).
- `popup.ts` "scan-tab" is sent with `fresh: true`, but the background handler
  ignores the field — dead contract field; remove it or implement it.
- `background/index.ts:49` `subscribeStatus(() => clearBadge())` clears a badge
  that is never set — either delete or repurpose (see §3.4).
- `interceptor.ts::confirm()` resumes the download before returning `accepted`,
  and the caller immediately re-pauses — a window where bytes flow. Keep it
  paused across the whole confirm flow and resume only on decline.

---

## 2. What competitors do that Ravyn's extension doesn't

Research sources are listed at the bottom.

### 2.1 🔵 Modifier key to bypass interception (IDM's Alt)

IDM's single most-loved integration feature: hold **Alt** while clicking to let
the browser take the download (configurable to Shift/Ctrl). Ravyn has no
equivalent — the only escape hatches are the confirmation dialog or Options.
Implementation sketch: content script tracks modifier state on click events
and stamps a short-lived "bypass" entry (URL hash, like `DelegationRegistry`);
the interceptor checks it before deciding.

### 2.2 🔵 File-type and minimum-size gates for interception

IDM and FDM intercept only a configured extension list (archives, executables,
media, …) and let everything else pass. Ravyn's "all-compatible" mode grabs
_every_ GET download, including 2 KB CSVs and files the browser handles better
inline. Add to settings: an intercept-extension list (default sensible set) and
a minimum-size threshold (skip when `Content-Length` is small/unknown — the
`DownloadItem` exposes `totalBytes`).

### 2.3 🔵 Quality/variant picker for detected streams (Video DownloadHelper)

VDH parses M3U8/MPD manifests and offers resolution variants (1080p/720p/…)
before download. Ravyn detects manifests (`classifier.ts`) but a click sends
the manifest straight to yt-dlp with default settings; the popup's max-height
dropdown is global, not per-variant. The backend already has `probe_media` —
surface its variant list in the popup as a per-manifest picker.

### 2.4 🔵 Segment-to-manifest correlation

`classifier.ts` deliberately ignores `.m4s`/`.ts` segments (correct), but
nothing links observed segments back to their manifest, and
`DetectedResource.parentManifestUrl` is never populated by any producer. Sites
that hide the manifest URL but leak segments (players that fetch the manifest
via blob) are invisible. VDH reconstructs streams from segments. At minimum,
when segments are seen without a manifest, show a "stream detected — analyze
page" hint in the popup.

### 2.5 🔵 Toolbar badge with detected-media count

VDH/FDM show a per-tab count of detected media on the toolbar icon — the main
discovery mechanism ("the icon lit up, so there's a video here"). Ravyn's badge
is intentionally blank (the comment in `background/index.ts:347` explains the
connection-flash problem — valid, but that argues against a _status_ badge, not
a _count_ badge). Use `setBadgeText({ tabId, text })` per tab with the media
resource count; clear on navigation.

### 2.6 🔵 Remembered batch selections / "OneClick" re-run (DownThemAll)

DTA remembers the last filter + renaming mask and re-applies them with one
click ("dTaOneClick"). Ravyn's popup filters reset per popup open (except type,
persisted). Persist the whole filter set (search, size bounds, same-domain) and
add a "download all matching last filter" command + keyboard shortcut.

### 2.7 🔵 Renaming masks / target-folder hints

DTA's renaming masks (`*name*.*ext*`, subfolder patterns) are the other half of
its appeal. The backend already has category templates; the extension only
passes a free-text `presetId` (a raw text input in the popup — users must
_know_ preset IDs). Replace it with a dropdown fetched from the backend
(`get_rules`-style native command listing presets/categories).

### 2.8 🔵 Chrome/Edge/Chromium port

Only Firefox is supported (`manifests/firefox.json`, gecko-specific settings,
`browser.*` namespace usage — already WebExtension-polyfill-compatible in
style). Every competitor ships Chrome-first. The codebase is close: MV3, no
Firefox-only APIs except `menus` (Chrome: `contextMenus`), `cookieStoreId`
(containers — Chrome: absent), and native-messaging registration (Windows
registry entry exists already under a Mozilla key; Chrome needs its own key
and `allowed_origins` instead of `allowed_extensions`). Both the manifest merge
script and `src-tauri/browser_integration.rs` would need a Chrome branch.

### 2.9 🔵 "Download panel" over videos vs. per-element overlays

IDM overlays a small panel above _playing_ videos. Ravyn attaches overlays to
`video`, `audio`, **and `img`** elements ≥320×180 — image-heavy pages get
hover buttons everywhere, which reads as intrusive (and the setting is called
"video overlays"). Suggest: separate `imageOverlays` setting, default **off**,
keep video/audio on.

---

## 3. Robustness & security observations

### 3.1 🟡 Popup image previews issue live network requests

`popup/index.ts::previewFor` sets `image.src = resource.url` for every visible
image resource — up to hundreds of GET requests from the extension context the
moment the Resources tab opens (with `referrerPolicy="no-referrer"`, but still
cookies-attached requests to third parties). Privacy/perf: gate previews behind
a click ("show preview"), or cap concurrent previews (e.g. first 30 rows,
IntersectionObserver for the rest — `loading="lazy"` already helps but the list
container is scrollable, so most previews still load).

### 3.2 🟡 Options "grant network permission" doesn't activate the observer

`options/index.ts::grantNetwork` requests the permission and checks the
checkbox, but `networkObservation` isn't saved (user must still click Save) and
`network.synchronize` runs only via the save path. If the user grants and
closes the page, the permission is held but unused. Save the setting in the
same flow as the grant.

### 3.3 🟡 Domain-list entries with schemes never match

Options accepts free text for disabled/always-intercept domains;
`domainMatches` compares hostnames, so a pasted `https://example.com` never
matches anything, silently. Strip scheme/path when normalizing
(`settings.ts::normalizeDomainList`) or validate with inline feedback.

### 3.4 ⚪ Heartbeat + poll pressure when the host is down

When Ravyn isn't running: popup polls summary every 2 s → each poll attempts
`connect()` → schedules reconnects; plus the 15 s heartbeat. Harmless but noisy.
Skip the summary poll while `hostAvailable === false` and rely on the
reconnect backoff's success to trigger one refresh.

### 3.5 ⚪ `ResourceCache.merge` eviction is O(n log n) per inserted resource

`cache.ts:32` sorts the whole map to find the oldest entry inside the per-item
loop — with `maxResourcesPerTab` = 2 000 and a big merge this is ~n² log n.
Track an insertion-ordered structure (Map iteration order ≈ insertion order —
delete-first-key is O(1)) or evict once after the loop.

### 3.6 ⚪ Overlay position drifts on layout changes

`overlay.ts` repositions on scroll/resize only. Layout shifts without scroll
(lazy content above, CSS animations) leave the button floating over the wrong
spot until the next scroll. A `ResizeObserver` on the element plus a
low-frequency `requestAnimationFrame` reconcile while visible would pin it.
Also: `protectedMedia` permanently replaces the icon with "!" on the first
`encrypted` event even if a later source is clear-content.

---

## 4. Testing gaps

Current coverage (25 tests) is good for pure functions (urls, srcset,
classifier, eligibility, state-machine, evaluator, validation, cache, popup
formatting). Untested and worth adding, in order of value:

1. **`DownloadInterceptor.handle`** — the highest-risk state machine in the
   extension (pause/resume/cancel ordering, the new pause-first behavior, the
   `finally` resume on every bail-out, handoff-failure resume). Mock
   `browser.downloads` + a fake NativeClient; assert exact call order.
2. **`menus/handlers.ts::handle`** — per-menu-item payload shape (the
   `srcUrl`/`linkUrl` regression this week would have been caught by a
   ten-line test).
3. **`NativeClient`** — request timeout, reconnect backoff progression, pending
   rejection on disconnect.
4. **`DelegationRegistry`** — hashing/normalization equivalence (`?utm=` vs
   hash-stripped URLs) and expiry.
5. **`RuleCache`** — TTL expiry, stale-on-error fallback, stored-snapshot load.

---

## 5. Suggested priority order

| #   | Item                                                          | Type       | Effort | Status                                                   |
| --- | ------------------------------------------------------------- | ---------- | ------ | -------------------------------------------------------- |
| 1   | 1.1 settings broadcast to content scripts (storage.onChanged) | bug        | S      | ✅ Done                                                  |
| 2   | 1.2 resolve relative srcset URLs in collectContext            | bug        | S      | ✅ Done                                                  |
| 3   | 1.4 error feedback on menu actions                            | bug        | S      | ✅ Done                                                  |
| 4   | 1.3 frameId-targeted context collection                       | bug        | S      | ✅ Done                                                  |
| 5   | 1.5 localize menu titles                                      | bug        | S      | ✅ Done                                                  |
| 6   | 1.8 forced-domain precedence over ask mode                    | bug        | S      | ✅ Done                                                  |
| 7   | 4.1 interceptor tests                                         | tests      | M      | ✅ Done (+ menu handler tests)                           |
| 8   | 2.1 Alt-to-bypass modifier                                    | feature    | M      | ✅ Done                                                  |
| 9   | 2.2 file-type + min-size interception gates                   | feature    | M      | ✅ Done                                                  |
| 10  | 2.5 per-tab media-count badge                                 | feature    | S      | ✅ Done                                                  |
| 11  | 1.6 real native event push (backend work)                     | arch       | L      | ✅ Done                                                  |
| 12  | 2.3 quality/variant picker from probe_media                   | feature    | L      | ✅ Done                                                  |
| 13  | 2.9 separate image-overlay setting (default off)              | UX         | S      | ✅ Done                                                  |
| 14  | 3.1 gated image previews in popup                             | privacy    | S      | ✅ Done                                                  |
| 15  | 3.2/3.3 options-page fixes                                    | UX         | S      | ✅ Done                                                  |
| 16  | 1.7 confirmation-state persistence                            | robustness | M      | ✅ Done (startup resume sweep)                           |
| 17  | 1.9 confirmation queueing                                     | UX         | M      | ✅ Done                                                  |
| 18  | 1.10 redirect-aware network observer                          | bug        | S      | ✅ Done                                                  |
| 19  | 2.6 persisted filters + one-click re-run                      | feature    | M      | ✅ Done (persistence; selection flow already existed)    |
| 20  | 2.7 preset dropdown instead of free-text ID                   | UX         | M      | ✅ Done                                                  |
| 21  | 2.4 segment→manifest correlation                              | feature    | L      | ✅ Done (hint, not full resource population — see below) |
| 22  | 2.8 Chrome port                                               | platform   | XL     | ⏸ Deferred — Firefox-only per direction                  |

S ≈ <½ day · M ≈ 1–2 days · L ≈ up to a week · XL ≈ multi-week.

Also fixed along the way: 3.4 (reconnect backoff now respected instead of
being re-triggered by every poll/heartbeat — fixed at the root in
`NativeClient.connect()` rather than in the popup), 3.5 (O(1) cache
eviction instead of an O(n log n) sort per merge), and all of 1.11's minor
nits except the `setBusy` one, which on inspection wasn't a real bug (the
selection-button state it affects is already reconciled by the next
`renderResources()` call on every path that changes the selection).

**Item 21 note**: implemented as a lightweight hint (a "stream detected" banner
when a segment is observed with no manifest resource yet), not by populating
`DetectedResource.parentManifestUrl` — segments are still never stored as
individual resources (would flood the popup's list with hundreds of `.ts`/
`.m4s` entries), so there was nothing for that field to attach to under this
design.

**Item 22 (Chrome/Edge port) is the only item not implemented** — out of scope
per explicit direction to stay Firefox-only. Everything else, including the
backend-touching items (11, 12, 20), is done: 1.6 required adding a
`RuleChanged` backend event (none existed before) and a real SSE-to-native-
messaging proxy thread in `native_messaging.rs`; 2.3 needed a camelCase
remapping fix in `probe_media`'s native command (it was the one command still
passing through raw snake_case, inconsistent with `get_rules`); 2.7 needed a
new `list_presets` native command backed by the backend's existing (but
previously unexposed-to-the-extension) presets API.

---

## Research sources

- [IDM: main program window / download panel](https://www.internetdownloadmanager.com/support/main.html)
- [IDM: Options dialog (Alt-key bypass, file-type list, panel customization)](https://www.internetdownloadmanager.com/support/using_idm/options.html)
- [IDM Integration Module on AMO](https://addons.mozilla.org/en-US/firefox/addon/tonec-idm-integration-module/)
- [DownThemAll! on AMO (filters, renaming masks, OneClick)](https://addons.mozilla.org/en-US/firefox/addon/downthemall/)
- [About DownThemAll!](https://about.downthemall.net/3.0/)
- [Video DownloadHelper on AMO (HLS/DASH variants, companion app)](https://addons.mozilla.org/en-US/firefox/addon/video-downloadhelper-allinone/)
- [Video DownloadHelper — feature overview](https://grokipedia.com/page/Video_DownloadHelper)
- [FDM official Firefox extension (two-level integration/observer model)](https://addons.mozilla.org/en-US/firefox/addon/free-download-manager-addon/)
- [Motrix (tray/status UX reference)](https://github.com/agalwood/Motrix)
