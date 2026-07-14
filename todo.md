# Ravyn Remaining Work

Last reconciled with source: 2026-07-14

This is the current roadmap. Browser-extension work is intentionally excluded.

## Completed in the current source

- Managed artifacts and transactional activation for yt-dlp, FFmpeg, and rqbit,
  including checksum/version/capability validation, cancellation, update,
  cleanup, and rollback.
- Signed remote component manifests with HTTPS conditional refresh, bounded
  reads, expiry, replay/downgrade protection, atomic activation, and LKG cache.
- Rqbit process supervision and loopback API health checks.
- Installed/portable/development setup modes, installation reporting, installed
  copy handoff, shortcuts/registry/startup integration, and uninstallation.
- Persisted setup integration consent, exact request validation in backend and
  Tauri, restart-safe idempotency, and verified installation reports.
- Main/setup capability separation, API process token, CSP, caller checks, and
  process transition guards.
- Multipage frontend for downloads, library, media, torrents, basket,
  automation, components, settings, diagnostics, and secure secrets.
- Executable-path configuration for yt-dlp, FFmpeg, rqbit, and 7-Zip plus the
  rqbit API URL and credential reference.
- Signed silent installed-app updates with persisted staging, readiness
  acknowledgement, retained executable rollback, failed-version retry blocking,
  repair staging, and persisted result display.
- Windows release workflow for test/build/package, Authenticode signing and
  verification, NSIS smoke checks, signed manifests, checksums, SBOM, and
  attestations.

## Release-critical remaining work

### 1. Native Windows compile and runtime pass

Run:

- `cargo test --locked --workspace --all-targets`;
- Tauri debug and release builds;
- NSIS and MSI bundles;
- installed, portable, and development startup;
- real component provisioning and real direct/media/torrent downloads.

### 2. Production release credentials and first signed release

The workflow is implemented. Supply and validate:

- Authenticode PFX and password;
- trusted timestamp URL;
- application-update signing key pair;
- component-manifest signing key pair;
- expected certificate publisher identity.

### 3. Full updater recovery and Windows E2E

Still required:

- rollback/recovery for every installed file and registry/uninstaller state;
- startup recovery after an interrupted detached helper;
- clean-machine update from N to N+1;
- forced readiness timeout/crash and automatic rollback;
- persisted-result verification after relaunch;
- power-loss/interruption tests.

### 4. Full WebView2 product automation

Automate setup, navigation, keyboard operation, DPI/scaling, light/dark/high
contrast, reduced motion, screen-reader labels, real downloads, failures,
retries, repair, and updater rollback on a clean Windows VM.

## Secondary product work

- Richer Metalink and large batch-import UX.
- Dedicated tag management and filename-template preview.
- Automation-rule preview.
- Deeper DHT, peer, host, and diagnostic tables.
- Structured editors for each secret type; generic secure storage is connected.
- Per-monitor different-wallpaper selection through `IDesktopWallpaper`.
- Optional native `.7z` extraction or managed 7-Zip provisioning.

## Product decisions

- Ravyn 0.2 uses a system or user-selected `7z.exe`/`7za.exe`.
- Browser-extension capture is outside this pass.

## Recommended order

1. Windows Rust/Tauri compile, tests, and clean install.
2. Production signed tagged release.
3. Updater/repair and rollback E2E.
4. Full installed-state recovery.
5. Advanced frontend surfaces.
