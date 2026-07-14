# Ravyn Remaining Work

Last reconciled with source: 2026-07-14

This roadmap lists only work that remains after the full frontend redesign and
current backend hardening pass. Browser-extension work is intentionally
excluded from the desktop completion target.

## Completed in the current source

- Direct, media, torrent, Metalink, text batch, JSON batch, and basket creation
  flows connected to the typed backend client.
- Downloads command bar, persistent sort/filter preferences, selection actions,
  drag-and-drop, keyboard commands, and standardized details.
- Library Files/Trash/Duplicates, tags, statistics, cleanup, verification,
  moved-file repair, bounded import, cancellation, truncation reporting,
  resilient scan warnings, and transactional physical root relocation.
- Media and torrent list/details workspaces, torrent file tree, peers, trackers,
  DHT/engine details, and explicit keep/delete removal choices.
- Visual automation rule builder, readable schedule editor, rule preview,
  execution history, and cancellation.
- Categorized Settings with dirty-state protection, restart-required state,
  appearance, downloads, storage, tools, network, updates, secrets,
  troubleshooting, and About.
- Component install/update/repair/verify/rollback/cancel/remove flows for
  yt-dlp, FFmpeg, and rqbit; custom/system 7-Zip policy.
- Signed component manifests and signed installed-app updater transactions with periodic checks, retry backoff, cancellation, discard, and restart-now installation.
- Persistent notification history and unread state.
- Static source audit for API/OpenAPI/client parity, Tauri invoke/handler/permission/capability parity, configuration syntax, Rust syntax parsing, and duplicate-UI-stack prevention.

## Release-critical remaining work

### 1. Native Windows compile and runtime validation

Run:

- `cargo test --locked --workspace --all-targets`;
- Tauri debug and release builds;
- NSIS and MSI bundles;
- installed, portable, and development startup;
- real component provisioning and real direct/media/torrent downloads.

### 2. Production release credentials and first signed release

Supply and validate:

- Authenticode PFX and password;
- trusted timestamp URL;
- application-update signing key pair;
- component-manifest signing key pair;
- expected certificate publisher identity.

### 3. Full clean-machine updater and repair E2E

Validate:

- update from N to N+1;
- readiness timeout and crash rollback;
- interrupted helper recovery in every journal phase;
- binary, registry, uninstaller, shortcut, and result restoration;
- repair of the current version;
- power-loss and forced-process-termination scenarios.

### 4. Native Library relocation fault-injection E2E

The transactional move is implemented in source. Validate it on real Windows
filesystems with:

- same-volume and cross-volume destinations;
- insufficient disk space and destination conflicts;
- cancellation during hashing and copying;
- process termination in every journal phase;
- restart verification and source cleanup;
- forced checksum mismatch and settings/database rollback;
- active, missing, imported, and trashed records;
- locked files, long paths, antivirus interference, and removable drives.

### 5. Full WebView2 product automation

Add clean-VM setup, navigation, focus restoration, keyboard-only operation,
DPI/scaling, light/dark/high contrast, reduced motion, screen-reader labels,
real downloads, failure/retry, provisioning, repair, and updater rollback.

## Secondary work

- Per-monitor different-wallpaper selection through `IDesktopWallpaper`.
- Optional native `.7z` extraction or managed 7-Zip provisioning after a trusted
  bootstrap policy is selected.
- Browser extension after the desktop release gate is complete.
