# Firefox extension implementation report

This report adapts `Ravyn-Firefox-Extension-Implementation-Plan` to the current Ravyn architecture. The original plan assumed a separate `ravyn-native-host` executable and an extension-specific backend API. The completed design reuses the installed Ravyn executable in a restricted short-lived mode and delegates to the existing authenticated loopback API.

## Phase status

| Phase | Status | Implementation |
|---|---|---|
| Repository and build system | Complete | TypeScript, esbuild, Firefox MV3 manifests, ESLint, Prettier, Vitest, web-ext, deterministic packaging |
| Manifest and permission model | Complete | Minimal required permissions plus optional cookies, webRequest, and host permissions |
| Native Messaging host | Complete | Protocol v1 in `src-tauri/src/native_messaging.rs`; registration in `browser_integration.rs` |
| Desktop integration | Complete | Setup registration, uninstall cleanup, status/repair/remove UI, browser action handoff |
| Download interception | Complete | Four modes, pause-first persistence, fallback resume, loop prevention, confirmation flow |
| Context menus | Complete | Links, images, media, selections, pages, paused/scheduled/probed variants |
| Page resource scanner | Complete | DOM, srcset, picture, CSS backgrounds, objects, scripts, links, performance entries, mutations, frames |
| Network discovery | Complete | Optional webRequest observer, bounded cache, manifest/media classification, segment suppression |
| Resource picker/sidebar | Complete | Search, type/domain/new/size filters, batch selection, preset/tags/media/subtitle/conversion options |
| Media controls | Complete | Closed Shadow DOM overlay, source fallback, yt-dlp probe, protected-media refusal |
| Cookies and containers | Complete | Explicit per-origin permission, container metadata, host matching, no extension persistence |
| Popup and options | Complete | Connection state, summaries, global job actions, interception and discovery preferences |
| Security hardening | Complete | Command allow-list, payload limits, URL/path validation, per-user descriptor, no remote code |
| Tests and fixtures | Complete | Unit suites plus local direct/dynamic/frame/media fixture server |
| CI and release | Complete | Cross-platform checks, deterministic XPI/source archive, AMO unlisted signing path, checksums and attestations |
| Chrome compatibility | Deferred | Deliberately outside the Firefox-first scope |

## Design adaptations

### Reused desktop executable

Using the installed Ravyn executable as the host prevents version skew and avoids shipping a second privileged binary. Native-host mode exits without creating a Tauri window.

### Existing backend contract

The host translates a small set of browser commands into existing `/v1` operations. The extension cannot provide an arbitrary method, path, body, or bearer token.

### Browser authentication

The older browser-token endpoints remain available for future integrations, but Firefox Native Messaging does not need to expose or persist a token in extension storage.

### Event transport

Native Messaging remains connected. The protocol acknowledges event subscription while the extension also refreshes capability state periodically, allowing clean recovery after the desktop backend restarts.

### Release signing

The repository can create a byte-reproducible unsigned XPI and source archive. Mozilla signing is an external trust step and is automated in tagged CI releases when AMO credentials are supplied.

## Remaining external release prerequisites

No source implementation phase remains open. A public installation still requires:

- valid Mozilla AMO API credentials and successful signing/review;
- execution of the Windows Rust, installer, native-host, and Firefox smoke tests in CI;
- final product icons and store listing media approved by the project owner.
