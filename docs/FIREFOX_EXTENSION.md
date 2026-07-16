# Firefox extension integration

Ravyn includes a Firefox-first WebExtension under `extension/` and a restricted Native Messaging host inside the installed desktop executable.

## Architecture

```text
Firefox background event page
  ├─ safe download interception
  ├─ context menus and toolbar state
  ├─ rule cache and resource cache
  ├─ optional network observer
  └─ Native Messaging client
          │
          ▼
Installed Ravyn executable in native-host mode
  ├─ framed protocol validation
  ├─ command allow-list
  ├─ payload and origin validation
  └─ authenticated loopback requests
          │
          ▼
Embedded Ravyn backend
```

Content scripts scan the DOM, observe mutations, and render closed-shadow-root media controls. The sidebar owns review and batch submission. The extension never receives the administrative backend bearer token.

## Installation lifecycle

During installed-mode setup or repair, Ravyn:

1. writes `com.ravyn.download_manager.json` to the Firefox Native Messaging directory;
2. points the manifest at the stable installed Ravyn executable;
3. restricts `allowed_extensions` to `firefox-extension@ravyn.app`;
4. registers the manifest in the current-user Mozilla registry key on Windows;
5. verifies the manifest and registration after writing them.

Uninstall removes both the registration and manifest. Portable mode intentionally does not register the integration because its executable path is not stable.

The desktop settings page exposes integration status, repair, and removal actions.

## Browser actions

When the extension asks to open Ravyn, the host publishes a validated action in the per-user runtime directory. The active desktop instance consumes it and navigates to a known section, optionally pre-filling a validated HTTP or HTTPS source URL. When Ravyn is not running, the host starts it and waits for the authenticated backend descriptor.

## Protocol security

- protocol version 1;
- maximum message size 1 MiB;
- maximum batch size 1,000;
- maximum cookie count 500;
- loopback-only backend endpoint;
- per-user bearer descriptor with live-process validation;
- credential-free HTTP/HTTPS URLs only;
- fixed command set and fixed post-processing presets;
- no arbitrary filesystem path, executable, SQL, command line, or backend route forwarding.

## Validation

```bash
npm ci --prefix extension
npm run check --prefix extension
node extension/test-pages/server.mjs
```

Tagged releases additionally require Mozilla AMO API credentials. CI uploads a signed unlisted XPI, source archive, checksums, and attestations alongside the desktop artifacts.
