# Ravyn Firefox Extension

The Ravyn Firefox extension discovers downloadable page resources and delegates approved downloads to the installed Ravyn desktop application through Firefox Native Messaging.

## Features

- Safe interception modes: disabled, rules only, ask every time, or all compatible downloads.
- Pause-first handoff: Firefox downloads are cancelled only after Ravyn confirms that the job was persisted.
- Context-menu actions for links, images, media, selections, and complete pages.
- DOM, mutation, performance-entry, and optional network resource discovery.
- Sidebar resource picker with type/domain/search filters, presets, tags, media options, subtitle options, and safe conversion presets.
- Optional per-site session-cookie forwarding. Cookie values are read only when a download is submitted and are never stored by the extension.
- Media overlays for non-protected HTML media.
- Toolbar status, recent job summary, global pause/resume, and desktop navigation.
- Memory-only resource caches, including private-window metadata.

## Requirements

- Firefox 142 or newer.
- An installed Ravyn desktop build with Firefox integration registered.
- The extension package signed by Mozilla for normal Firefox installations.

Portable Ravyn builds can run the application, but Firefox Native Messaging registration intentionally requires the installed mode so the host path remains stable.

## Development

```bash
npm ci
npm run check
```

Useful commands:

```bash
npm run build
npm run run
npm run test
npm run package
npm run build:source
```

`npm run package` creates a deterministic unsigned XPI in `artifacts/`. A normal Firefox release must be signed through AMO. The release workflow supports unlisted AMO signing when `RAVYN_AMO_API_KEY` and `RAVYN_AMO_API_SECRET` are configured.

## Native protocol

The native host is the installed Ravyn executable itself. Firefox starts it in a restricted stdio mode. The protocol is versioned, capped at 1 MiB per message, validates every payload, and never forwards arbitrary HTTP requests or executable arguments.

The permitted operations are limited to health/capability checks, download creation, batch creation, media probing, job control, rule evaluation, and opening a known Ravyn section.

## Manual fixture pages

```bash
node test-pages/server.mjs
```

Open `http://127.0.0.1:4177` to validate direct downloads, dynamic resources, nested frames, media manifests, overlays, context menus, and optional network monitoring.

## Directory layout

```text
manifests/       Shared and Firefox-specific manifests
scripts/         Build, validation, and deterministic packaging
src/background/  Privileged browser orchestration
src/content/     Page scanning and media overlays
src/popup/       Toolbar popup
src/sidebar/     Resource picker
src/options/     Extension preferences
src/confirmation Interception confirmation window
static/          HTML, CSS, icons, and locales
test-pages/      Local manual integration fixtures
```
