# Ravyn Firefox Extension — Complete Implementation Plan

> Update (2026-07-16): the implemented resource picker now lives in the compact
> toolbar popup. Historical sidebar sections below describe the original plan
> and are superseded by `docs/FIREFOX_EXTENSION_IMPLEMENTATION_REPORT.md`.

## 1. Purpose

The Ravyn Firefox extension will act as the browser-facing entry point for the Ravyn download manager. The extension will discover downloadable resources, intercept eligible Firefox downloads, expose context-menu actions, add download controls to media elements, and communicate securely with the Ravyn desktop backend.

The extension will be designed for Firefox first. Chrome support will be added later through a compatibility layer and browser-specific manifests.

---

## 2. Core Product Principles

1. Ravyn remains the actual download engine.
2. The extension only discovers, classifies, and delegates downloads.
3. Firefox downloads must never be cancelled until Ravyn confirms that the job was persisted.
4. Automatic interception must be configurable and disabled by default or limited to rules.
5. The extension must not expose unrestricted backend privileges.
6. Native Messaging should be the primary browser-to-desktop transport.
7. No DRM circumvention should be attempted.
8. No remote executable code, external scripts, or telemetry should be included.
9. Permissions must be requested only when the related feature is enabled.
10. The first release should remain useful, reviewable, and safe rather than attempting every browser edge case.

---

## 3. Recommended Technology Stack

- **Language:** TypeScript
- **Manifest:** Firefox Manifest V3
- **Package manager:** pnpm
- **Bundler:** Vite or esbuild
- **Firefox development tooling:** web-ext
- **Unit testing:** Vitest
- **DOM testing:** happy-dom or jsdom
- **Browser API style:** `browser.*`
- **Native bridge:** Small Rust executable
- **Popup/sidebar UI:** Lightweight Preact or vanilla TypeScript
- **Schema validation:** Zod or hand-written validators
- **Linting and formatting:** ESLint and Prettier

---

## 4. High-Level Architecture

```text
Firefox
│
├── Background event page
│   ├── Download interception
│   ├── Context-menu controller
│   ├── Native Messaging client
│   ├── Network resource observer
│   ├── Rule evaluator
│   ├── Per-tab resource cache
│   └── Extension state coordinator
│
├── Content scripts
│   ├── DOM resource scanner
│   ├── Video overlay
│   ├── Mutation observer
│   ├── Element context collector
│   └── Page-to-extension messaging
│
├── Extension UI
│   ├── Toolbar popup
│   ├── Firefox sidebar
│   ├── Resource picker
│   ├── Interception confirmation
│   └── Options page
│
└── Native Messaging
    │
    ▼
ravyn-native-host
    │
    ▼
Ravyn backend
```

The background page owns privileged browser APIs. Content scripts inspect pages and send structured messages to the background context. The native host exposes a narrow, versioned protocol instead of forwarding arbitrary backend API requests.

---

## 5. Manifest Strategy

Use separate manifests generated from a shared base:

```text
manifest.base.json
manifest.firefox.json
manifest.chrome.json   # added later
```

The Firefox package should use `background.scripts`, because Firefox Manifest V3 currently runs background logic through an event page rather than Chrome-style service workers.

Recommended initial Firefox manifest capabilities:

```json
{
  "manifest_version": 3,
  "name": "Ravyn Download Manager",
  "version": "0.1.0",
  "description": "Send downloads and page resources to Ravyn.",
  "background": {
    "scripts": ["background/index.js"],
    "type": "module"
  },
  "action": {
    "default_popup": "popup/index.html",
    "default_title": "Ravyn"
  },
  "sidebar_action": {
    "default_title": "Ravyn Resources",
    "default_panel": "sidebar/index.html"
  },
  "permissions": [
    "activeTab",
    "downloads",
    "menus",
    "nativeMessaging",
    "notifications",
    "scripting",
    "storage"
  ],
  "optional_permissions": [
    "cookies",
    "webRequest"
  ],
  "optional_host_permissions": [
    "<all_urls>"
  ],
  "browser_specific_settings": {
    "gecko": {
      "id": "firefox-extension@ravyn.app",
      "strict_min_version": "140.0",
      "data_collection_permissions": {
        "required": [
          "websiteActivity",
          "websiteContent"
        ]
      }
    }
  }
}
```

The final permission list should be reviewed before AMO submission.

---

## 6. Permission Model

### Required permissions

- `downloads`
- `menus`
- `nativeMessaging`
- `storage`
- `activeTab`
- `scripting`
- `notifications`

### Optional permissions

- `webRequest`
- `<all_urls>`
- `cookies`

Optional permissions should be requested only through explicit user actions.

Suggested onboarding switches:

- Enable automatic download interception
- Enable media detection on all websites
- Allow authenticated downloads using site cookies
- Enable video download buttons

Each switch should request only the permissions needed for that feature.

---

## 7. Native Messaging

### Primary transport

```text
Firefox extension
→ Firefox Native Messaging
→ ravyn-native-host
→ Ravyn backend
```

The extension should not connect directly to the administrative HTTP API.

### Host identity

Suggested host name:

```text
com.ravyn.download_manager
```

Example Firefox native-host manifest:

```json
{
  "name": "com.ravyn.download_manager",
  "description": "Ravyn Firefox integration host",
  "path": "C:\\Program Files\\Ravyn\\ravyn-native-host.exe",
  "type": "stdio",
  "allowed_extensions": [
    "firefox-extension@ravyn.app"
  ]
}
```

### Versioned native protocol

```ts
interface NativeRequest<T> {
  id: string;
  protocolVersion: 1;
  command: NativeCommand;
  payload: T;
}

interface NativeResponse<T> {
  id: string;
  ok: boolean;
  result?: T;
  error?: {
    code: string;
    message: string;
    retryable: boolean;
  };
}
```

### Supported commands

- `ping`
- `get_capabilities`
- `create_download`
- `create_batch`
- `probe_media`
- `get_download_summary`
- `get_job`
- `pause_job`
- `resume_job`
- `cancel_job`
- `get_rules`
- `evaluate_url`
- `open_ravyn`
- `subscribe_events`

### Forbidden native capabilities

The extension must not be allowed to provide:

- arbitrary FFmpeg arguments
- arbitrary executable paths
- raw local file paths
- arbitrary cookie database paths
- raw SQL
- unrestricted post-processing commands
- arbitrary filesystem destinations outside approved presets

Use `runtime.connectNative()` for a persistent connection and live events. Use `sendNativeMessage()` only for short, isolated requests.

---

## 8. Download Interception

### Modes

```ts
type InterceptionMode =
  | "disabled"
  | "rules-only"
  | "ask"
  | "all-compatible";
```

Recommended default:

```text
rules-only
```

### Safe interception state machine

```text
downloads.onCreated
        │
        ▼
Ignore Ravyn-created downloads
        │
        ▼
Check interception rules
        │
        ▼
Pause Firefox download
        │
        ▼
Collect URL, referrer, MIME, filename hint, tab context
        │
        ▼
Send create_download to native host
        │
        ├── Ravyn confirms persisted job
        │       ├── cancel Firefox download
        │       ├── remove partial browser file
        │       └── optionally erase browser download entry
        │
        └── Ravyn unavailable or rejects
                └── resume Firefox download
```

### Loop prevention

Ignore items where:

```ts
download.byExtensionId === browser.runtime.id
```

Also maintain a short-lived delegation registry:

```ts
interface DelegatedDownload {
  normalizedUrlHash: string;
  createdAt: number;
  ravynJobId: string;
}
```

### Downloads not intercepted automatically in the first release

- `blob:` URLs
- `data:` URLs
- `file:` URLs
- POST-based downloads
- downloads requiring unknown request bodies
- unknown encrypted streams
- private-window downloads when private support is disabled
- URLs rejected by Ravyn probing

For uncertain cases, show confirmation or leave the download in Firefox.

---

## 9. Context Menus

Create one parent menu:

```text
Ravyn
```

### Link context

- Download link with Ravyn
- Add link paused
- Analyze link
- Schedule link
- Scan linked page

### Image context

- Download image with Ravyn
- Download original image
- Choose image source
- Convert and download
- Download all page images

### Video and audio context

- Download media with Ravyn
- Analyze available formats
- Download audio only
- Download subtitles
- Open media picker

### Selected text context

- Download URLs in selection
- Scan selection for links

### Page context

- Scan page resources
- Download all images
- Download all media
- Send page to yt-dlp
- Monitor page for new resources
- Open Ravyn sidebar

For complex images, the content script should collect:

- `currentSrc`
- `src`
- `srcset`
- `<picture>` sources
- parent link
- CSS background image
- natural dimensions
- alt text

---

## 10. Toolbar Popup

The popup should remain compact.

```text
Ravyn
Connected · 3 active · 18.4 MB/s

[ Paste URL                         ]
[ Download ] [ Add paused ]

Current page
• Analyze video
• Scan resources
• Download all images
• Monitor this page

Recent jobs
video.mp4                64%
archive.zip              Queued
```

### Popup responsibilities

- native host connection status
- quick URL submission
- current-page actions
- active job summary
- open sidebar
- open desktop application
- global pause/resume

The popup should not duplicate the full desktop interface.

---

## 11. Firefox Sidebar

Firefox’s native sidebar is the ideal resource-picker surface.

### Sidebar features

- page-resource list
- search
- type tabs
- selection controls
- extension and MIME filters
- domain filters
- minimum and maximum size filters
- same-domain-only filter
- new-resources-only filter
- destination preset
- tags
- post-processing preset
- download selected

### Resource model

```ts
interface DetectedResource {
  id: string;
  url: string;
  normalizedUrl: string;
  pageUrl: string;
  frameUrl?: string;
  type:
    | "image"
    | "video"
    | "audio"
    | "manifest"
    | "document"
    | "archive"
    | "other";
  mime?: string;
  extension?: string;
  filename?: string;
  size?: number;
  source:
    | "dom"
    | "performance"
    | "webRequest"
    | "context-menu"
    | "video-element";
  confidence: number;
}
```

---

## 12. DOM Resource Scanner

The content script should scan:

```text
a[href]
img[src]
img[srcset]
picture source[srcset]
video[src]
video source[src]
audio[src]
audio source[src]
track[src]
object[data]
embed[src]
link[href]
script[src]
```

It should also inspect:

```js
performance.getEntriesByType("resource")
```

A bounded `MutationObserver` should detect dynamically added resources.

### Normalization rules

- resolve relative URLs
- remove URL fragments
- preserve query strings
- reject unsupported schemes
- decode HTML entities
- deduplicate normalized URLs
- record source element and frame
- enforce a maximum resource count

The scanner should send structured resource records instead of the complete page HTML unless the user explicitly requests backend HTML scanning.

---

## 13. Network Resource Observer

This is an optional advanced feature based on Firefox `webRequest`.

Observe:

- `onBeforeRequest`
- `onHeadersReceived`
- `onCompleted`
- `onErrorOccurred`

### Request model

```ts
interface ObservedRequest {
  requestId: string;
  tabId: number;
  frameId: number;
  method: string;
  url: string;
  documentUrl?: string;
  type: string;
  statusCode?: number;
  mime?: string;
  contentLength?: number;
  contentDisposition?: string;
  startedAt: number;
}
```

### Recognized media types and extensions

- `video/*`
- `audio/*`
- `application/octet-stream`
- `application/vnd.apple.mpegurl`
- `application/x-mpegurl`
- `application/dash+xml`
- `.m3u8`
- `.mpd`
- `.mp4`
- `.webm`
- `.mkv`
- `.mp3`
- `.flac`
- `.m4a`
- `.ogg`

HLS `.ts` and DASH `.m4s` fragments should be grouped beneath their parent manifest rather than shown individually.

### Cache limits

- maximum 2,000 entries per tab
- maximum age 30 minutes
- clear when the tab closes
- no persistent browsing history

---

## 14. Video Download Overlay

### Detection

Observe:

- `<video>` elements
- `<audio>` elements
- dynamic player containers
- elements inserted by single-page applications

### UI isolation

Use a Shadow DOM root:

```html
<ravyn-media-control>
  <button>Download</button>
</ravyn-media-control>
```

### Display rules

Show the overlay:

- on video hover
- only for videos above a configurable size
- only on supported origins
- never on tiny previews
- once per player
- with accessible labels and keyboard support

### Collected metadata

- page URL
- page title
- video `currentSrc`
- video `src`
- `<source>` elements
- poster
- duration
- video dimensions
- matching network-observed manifests

### Decision logic

```text
Direct MP4 or WebM
→ direct Ravyn download

HLS or DASH manifest
→ media job

Supported website
→ send page URL to yt-dlp probe

blob: URL
→ resolve through observed network sources
→ otherwise offer Analyze page

DRM indication
→ show unsupported message
```

Ravyn must not attempt to bypass DRM.

---

## 15. Rules

The backend remains the source of truth. The extension caches a reduced rule snapshot.

```ts
interface BrowserRule {
  id: string;
  priority: number;
  enabled: boolean;
  domains: string[];
  extensions: string[];
  mimePatterns: string[];
  minimumSize?: number;
  action:
    | "browser"
    | "ravyn"
    | "ask"
    | "ignore";
  presetId?: string;
}
```

### Example rules

```text
*.github.com + zip/exe/msi
→ Ravyn

youtube.com
→ media probe

localhost
→ Firefox

image/* over 1 MB
→ Ravyn Images preset

*.bank.example
→ never intercept
```

Cache rules with:

- revision
- updated timestamp
- expiration timestamp

Refresh the cache when Ravyn reports a rule change.

---

## 16. Authenticated Downloads and Cookies

Do not request the `cookies` permission in the first release.

### First-release behavior

- public direct downloads: supported
- URL-token downloads: usually supported
- browser-session-cookie downloads: browser fallback or explicit confirmation

### Later per-site cookie access

Add an explicit option:

```text
Allow Ravyn to use cookies from this site
```

When enabled:

1. request `cookies` and the target host permission
2. read cookies only for the target origin
3. send cookies only for the specific job
4. never save them in extension storage
5. mark them as sensitive in the native protocol
6. wipe them from memory after acknowledgement

The native host should not read the Firefox cookie database directly.

---

## 17. Private Browsing and Firefox Containers

### Private browsing

Default:

```text
Disabled in private windows
```

When explicitly enabled:

- do not write private activity to extension history
- do not persist private page resources
- mark the request as incognito
- optionally create temporary Ravyn jobs
- avoid page-monitoring history

### Multi-Account Containers

Pass source metadata:

```ts
sourceContext: {
  browser: "firefox",
  containerId?: string,
  incognito: boolean
}
```

A container ID does not automatically provide Ravyn with access to that container’s cookies.

---

## 18. Extension Storage

### Safe to store in `browser.storage.local`

- extension settings
- permission state
- reduced rule cache
- disabled-domain list
- overlay preferences
- native protocol capabilities
- last successful native connection

### Memory-only data

- network request cache
- pending interception decisions
- native request promises
- current tab resource lists
- temporary cookie material

### Never store

- backend administrator tokens
- plaintext cookies
- complete browsing history
- page HTML by default
- arbitrary request headers
- filesystem paths supplied by pages

---

## 19. Privacy and AMO Compliance

The extension should follow these rules:

- all executable code is bundled in the signed package
- no remotely loaded JavaScript
- no analytics by default
- no third-party telemetry
- no CDN scripts
- no executable remotely updated rules
- explicit permission explanations
- clear onboarding disclosure
- one-click Clear extension data action
- privacy policy describing local URL and resource transmission
- source package and reproducible build instructions for AMO review

All data should be sent only to the locally installed Ravyn application.

---

## 20. Repository Structure

```text
ravyn-firefox-extension/
├── package.json
├── pnpm-lock.yaml
├── tsconfig.json
├── vite.config.ts
├── web-ext-config.mjs
│
├── manifests/
│   ├── base.json
│   └── firefox.json
│
├── src/
│   ├── background/
│   │   ├── index.ts
│   │   ├── downloads/
│   │   │   ├── interceptor.ts
│   │   │   ├── eligibility.ts
│   │   │   ├── delegation.ts
│   │   │   └── state-machine.ts
│   │   ├── menus/
│   │   │   ├── register.ts
│   │   │   └── handlers.ts
│   │   ├── native/
│   │   │   ├── client.ts
│   │   │   ├── protocol.ts
│   │   │   ├── reconnect.ts
│   │   │   └── capabilities.ts
│   │   ├── network/
│   │   │   ├── observer.ts
│   │   │   ├── cache.ts
│   │   │   ├── classifier.ts
│   │   │   └── manifests.ts
│   │   ├── rules/
│   │   │   ├── cache.ts
│   │   │   └── evaluator.ts
│   │   └── messages.ts
│   │
│   ├── content/
│   │   ├── index.ts
│   │   ├── scanner/
│   │   │   ├── dom-scanner.ts
│   │   │   ├── normalizer.ts
│   │   │   ├── srcset.ts
│   │   │   └── mutation-observer.ts
│   │   ├── media/
│   │   │   ├── detector.ts
│   │   │   ├── overlay.ts
│   │   │   ├── overlay.css
│   │   │   └── source-collector.ts
│   │   └── messages.ts
│   │
│   ├── popup/
│   ├── sidebar/
│   ├── options/
│   ├── confirmation/
│   │
│   ├── shared/
│   │   ├── contracts.ts
│   │   ├── errors.ts
│   │   ├── logger.ts
│   │   ├── permissions.ts
│   │   ├── settings.ts
│   │   ├── urls.ts
│   │   └── validation.ts
│   │
│   └── tests/
│
├── native-host/
│   ├── Cargo.toml
│   └── src/
│
└── scripts/
    ├── build-firefox.mjs
    ├── package-firefox.mjs
    └── install-native-host.ps1
```

---

## 21. Implementation Milestones

### Milestone 0 — Foundation

Deliverables:

- TypeScript project
- Firefox MV3 manifest
- stable Firefox extension ID
- build pipeline
- web-ext run/lint/package commands
- shared contracts
- logging and error types
- basic popup

Acceptance criteria:

- extension installs temporarily
- background event page starts
- popup opens
- no console errors
- AMO linter passes

### Milestone 1 — Native Messaging

Deliverables:

- Rust native host
- native-host manifest installer
- ping command
- capability negotiation
- reconnect logic
- connection status
- open Ravyn command

Acceptance criteria:

- Firefox detects Ravyn
- extension reconnects after Ravyn restart
- invalid extension IDs are rejected
- malformed native messages are rejected

### Milestone 2 — Manual Download Actions

Deliverables:

- paste URL
- context menus
- current-page download
- direct image/video/link handoff
- batch URL handoff
- backend acknowledgement

Acceptance criteria:

- no automatic interception yet
- each action creates one idempotent Ravyn job
- backend-offline errors are visible and recoverable

### Milestone 3 — Safe Download Interception

Deliverables:

- `downloads.onCreated`
- disabled/rules/ask/all modes
- pause → delegate → cancel-or-resume state machine
- loop prevention
- MIME/extension/domain rules
- Firefox fallback

Acceptance criteria:

- successful handoff cancels the Firefox download
- failed handoff resumes it
- Ravyn-created downloads are not intercepted again
- blob/data/POST cases fall back safely

### Milestone 4 — Page Scanner and Sidebar

Deliverables:

- DOM scanner
- `srcset` handling
- dynamic DOM observation
- resource normalization
- sidebar picker
- filtering
- batch import

Acceptance criteria:

- static and dynamically added resources are detected
- duplicate URLs are removed
- large pages remain responsive
- selected resources can be sent to Ravyn

### Milestone 5 — Media Detection

Deliverables:

- optional `webRequest`
- per-tab network cache
- HLS/DASH detection
- segment grouping
- video overlay
- yt-dlp probe
- quality selection

Acceptance criteria:

- direct video is detected
- HLS/DASH manifests are shown instead of individual fragments
- blob video resolves to a network source when possible
- DRM media is reported as unsupported

### Milestone 6 — Authentication and Advanced Rules

Deliverables:

- optional cookie permission
- per-site cookie consent
- Firefox Container metadata
- full rule synchronization
- page monitoring actions
- safe post-processing presets

Acceptance criteria:

- cookies are never persisted
- cookie access is origin-scoped
- rules behave identically in extension and backend
- browser tokens cannot invoke privileged operations

### Milestone 7 — Hardening and AMO Release

Deliverables:

- accessibility review
- localization
- privacy policy
- permission explanations
- AMO data declarations
- source package
- reproducible build instructions
- threat model
- signed beta channel

Acceptance criteria:

- web-ext lint passes
- no remote executable code
- no unnecessary permissions
- native protocol is fuzz-tested
- AMO review package is reproducible

---

## 22. Test Plan

### Unit tests

- URL normalization
- resource classification
- rule priority
- download eligibility
- interception transitions
- native-protocol parsing
- `srcset` parsing
- media-manifest detection
- loop prevention
- permission state

### Firefox integration test pages

- direct file download
- Content-Disposition attachment
- unknown file size
- redirected download
- signed expiring URL
- POST-generated file
- blob video
- direct MP4
- HLS
- DASH
- dynamic infinite scroll
- `srcset` and `<picture>`
- nested iframe
- private window
- Firefox Container
- backend offline
- native host restart

### Security tests

- hostile page sending forged messages
- oversized resource list
- malformed native JSON
- native-message length overflow
- attempts to invoke privileged commands
- malicious filenames
- non-HTTP URL schemes
- duplicate event storms
- page CSS trying to hide or hijack the overlay
- extension-origin spoofing
- incorrect extension ID in native manifest

---

## 23. Chrome Migration Strategy

Chrome support should be added only after the Firefox version is stable.

Keep browser-specific behavior behind adapters:

```ts
interface BackgroundRuntimeAdapter {}
interface SidebarAdapter {}
interface NativeHostManifestAdapter {}
interface PermissionAdapter {}
```

Expected Chrome changes:

```text
Firefox background.scripts
→ Chrome background.service_worker

Firefox sidebar_action
→ Chrome sidePanel

Firefox native allowed_extensions
→ Chrome allowed_origins

Firefox browser.*
→ internal wrapper or WebExtension polyfill
```

The following logic should remain shared:

- resource scanning
- rule evaluation
- native protocol
- interception state machine
- media classification
- normalization
- filtering
- batch delegation

---

## 24. Recommended First Public Release

Include:

- Native Messaging
- manual URL submission
- context-menu downloads
- page scanner
- Firefox sidebar resource picker
- rules-only automatic interception
- safe fallback to Firefox
- basic video overlay
- yt-dlp page analysis
- connection notifications
- error notifications

Do not include initially:

- automatic cookie extraction
- arbitrary post-processing configuration
- persistent browsing history
- full POST-download interception
- DRM handling
- remote telemetry
- Chrome compatibility code

---

## 25. Final Product Vision

The complete Firefox extension should support this workflow:

```text
Normal browser download
→ configurable automatic interception

Right click on an element
→ direct element download

Video overlay
→ quick media download or format selection

Scan page
→ DownloadThemAll-style resource picker

Network observer
→ dynamic media and stream discovery

Native Messaging
→ secure delegation to Ravyn
```

The extension should become a universal browser-side resource detector while Ravyn remains the high-performance, persistent download engine.
