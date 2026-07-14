# Ravyn Full-Stack Updater Phase 8

Date: 2026-07-14

## Completed

- Replaced the one-shot startup update check with a persistent native scheduler.
- Added a six-hour normal interval and bounded 15–120 minute retry backoff.
- Added cooperative cancellation across metadata requests, installer download,
  file writes, persistence, and staged activation.
- Added discard support for an already verified staged installer.
- Added **Restart and install** while retaining install-on-normal-close as the
  default behavior.
- Added explicit `cancelling` and `cancelled` states to Rust, Tauri IPC, and the
  Svelte UI.
- Added last-check, next-check, interval, package, and install-behavior details.
- Added unit coverage for presentation, retry backoff, and cancellation tokens.
- Extended the source audit to validate frontend Tauri invokes against the
  native handler, permission declarations, and window capabilities.
- Added Windows CI parsing of the generated updater PowerShell helper.
- Strengthened the installed-app release smoke test with an opt-in readiness
  marker and a real `/health/ready` request.
- Made all loopback health probes public while keeping every non-health API
  route bearer-token protected.

## Still requires native release qualification

- Compile and run the full Rust/Tauri workspace on Windows.
- Perform an actual signed N-to-N+1 update between two published installers.
- Force readiness timeout, process crash, helper interruption, and rollback.
- Validate same-version repair and every restored registry/shortcut state.
- Run clean-machine WebView2 product automation and accessibility checks.
