# Ravyn Completion Pass 2 Checkpoint

Checkpoint created: 2026-07-14

This checkpoint preserves the second implementation pass after the frontend
refinement package. The browser extension remains intentionally excluded.

## Added or completed in this pass

- Production remote component-manifest delivery:
  - HTTPS-only endpoint validation;
  - conditional requests with ETag and Last-Modified;
  - bounded response reads and request timeouts;
  - Ed25519 signature verification;
  - channel, generation-time, expiry, replay, and downgrade validation;
  - atomic cache activation, last-known-good recovery, and status reporting;
  - backend routes, OpenAPI contracts, Components UI status, and manual refresh;
  - tagged-release manifest generation and verification.
- Installed-app updater reliability:
  - persisted staged-update metadata restored after restart;
  - signed current-version repair package support;
  - backend and webview readiness acknowledgement;
  - retained previous executable and automatic rollback;
  - persisted update result shown in Settings;
  - automatic retry suppression for a version that previously failed or rolled
    back, while still allowing an explicit manual retry.
- Durable setup integration consent:
  - database migration and repository round-trip;
  - exact request matching in the backend and Tauri shell;
  - restart-safe idempotency and verified installation reporting.
- Secure-secret product flow:
  - create/replace/delete/list UI;
  - one-way secret submission to the authenticated loopback backend;
  - operating-system credential-store persistence;
  - backend type-specific validation;
  - rqbit credential reference selection in Settings;
  - secret values are never returned by the API.
- Executable configuration:
  - yt-dlp, FFmpeg, rqbit, and 7-Zip path pickers;
  - rqbit API URL and credential reference;
  - restart-required handling for rqbit executable changes.
- Frontend refinement:
  - calmer navigation and shell hierarchy;
  - reduced decorative gradients and redundant surfaces;
  - clearer page headers, metrics, transfer rows, dialogs, and form controls;
  - consistent light/dark/high-contrast behavior across primary screens;
  - improved empty, loading, error, and disabled states.
- Release workflow:
  - Authenticode certificate import and Tauri signing configuration;
  - signature and timestamp verification for EXE/NSIS/MSI artifacts;
  - signed application-update and component manifests;
  - installer smoke checks, checksums, SBOM, and attestations.

## Validation completed in this environment

- `npm run check`: 0 errors and 0 warnings.
- `npm test`: 67/67 tests passed.
- `npm run build`: production build completed.
- 126 Rust source files parsed without syntax errors using a Rust grammar.
- All project JSON and TOML files parsed successfully.
- The release workflow YAML parsed successfully.

The environment does not contain Cargo, rustc, rustfmt, the Windows SDK, or a
WebView2 desktop runtime. Native compilation and Windows behavior are therefore
not claimed as verified by this checkpoint.

## Effective remaining work

### Release-critical verification and external configuration

1. Run `cargo test --locked --workspace --all-targets` on Windows and fix any
   native-only compiler, linker, Tauri, registry, or installer issue.
2. Execute a complete tagged release using the real Authenticode certificate,
   timestamp service, application-update key, and component-manifest key.
3. Validate a clean-machine install, launch, real managed component download,
   direct/media/torrent download, uninstall, and upgrade from version N to N+1.
4. Force update readiness failure, process crash, and power interruption to
   validate rollback and stranded-transaction recovery.

### Remaining implementation hardening

1. Extend updater rollback from the retained main executable to a fully defined
   transaction for every installed file plus registry and uninstaller state.
2. Add explicit startup recovery for an update helper interrupted between
   installer execution and final result cleanup.
3. Add full WebView2 UI automation for keyboard use, DPI/scaling, high contrast,
   reduced motion, screen readers, errors, retries, and update rollback.
4. Add per-monitor wallpaper selection through `IDesktopWallpaper` when Windows
   assigns different images to different monitors.

### Secondary product surfaces

1. Richer Metalink and large batch-import workflows.
2. Dedicated tag management, filename-template preview, and automation-rule
   preview interfaces.
3. Deeper DHT, peer, host, and diagnostic tables.
4. More structured secret editors for cookies, rqbit credentials, certificates,
   and private keys; the secure generic flow is already functional.
5. Optional native `.7z` extraction or managed 7-Zip provisioning. Ravyn 0.2
   already supports a validated system or user-selected `7z`/`7za` executable.

## Explicitly deferred

- Browser-extension capture, packaging, and product UX.
