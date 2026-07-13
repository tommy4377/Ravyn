
# What is still missing exactly

## ✅ IMPLEMENTED — items resolved since the original audit

### ~~2. Installed engines are not activated in the current backend~~

`finish_setup_handoff()` (`src-tauri/src/lib.rs:47-77`) now launches the installed copy via `std::process::Command::new(&expected)` and exits the original process. The spawned copy boots a fresh backend, so engine paths are deterministically applied.

### ~~3. rqbit is installed but not truly managed~~

`src/services/rqbit_process.rs` implements a full `RqbitProcessManager` with a six-state lifecycle (`Stopped`, `Starting`, `Ready`, `Degraded`, `Restarting`, `Failed`, `Stopping`). It binds a loopback port, spawns rqbit, waits for HTTP readiness, verifies required API endpoints, handles port collisions with retry, and stops cleanly on shutdown.

### ~~4. Setup does not launch the installed copy~~

`finish_setup_handoff()` spawns the executable at `%LOCALAPPDATA%\Programs\Ravyn\Ravyn.exe`, then exits via `app.exit(0)`. The installed copy boots its own backend before opening the main window.

### ~~5. Registered uninstallation does not exist~~

`src-tauri/src/uninstall.rs` provides `try_handle_command_line()` parsing `--uninstall`, registry cleanup, shortcut removal, data-directory purge (with `--purge-data`), and self-delete via PowerShell.

### ~~9. Version comparison is just string inequality~~

`version_cmp()` in `src/services/components.rs:1103-1126` compares numeric runs of dotted and date-style versions correctly (e.g. `2025.10.1 > 2025.9.30`), with non-numeric suffixes as tie-breaker.

### ~~10. The health check does not validate the expected version~~

`install_component_with_progress()` (`src/services/components.rs:890-993`) extracts `detected_version` from the binary's `--version` output and compares it against `artifact.version`. A mismatch deactivates the component and returns an error.

### ~~13. A global limit on API-requested installations is missing~~

`ProvisioningCancellation` (`src/services/components.rs:385-471`) includes a `tokio::sync::Semaphore::new(2)` limiting concurrent installations. The `acquire()` method blocks until a permit is available, and cancellation remains responsive.

### ~~14. Cancellation still has a race condition near activation~~

Token checks are inserted before `set_executable()`, before `atomic_replace()`, and after the download block in `download_and_install()` (`src/services/engines.rs:374-400`).

### ~~16. Executable installation is not fully transactional~~

`install_executable()` (`src-tauri/src/integration.rs:203-236`) copies to `.ravyn.install.tmp`, verifies SHA-256, renames the old target to `.ravyn.previous.exe`, activates the staged file, and restores from backup on failure. `confirm_installed_copy_ready()` removes the backup after a successful startup.

### ~~21. The embedded API does not mandatorily use a per-process token~~

The desktop shell (`src-tauri/src/backend.rs:101`) generates a `uuid::Uuid::new_v4()` token per process and passes it only through Tauri IPC. The `require_token` middleware in `src/api/mod.rs` enforces it for every API call.

---

## 🔶 PARTIALLY IMPLEMENTED — exists but needs finishing

### 7. The signed manifest system exists but is not connected

`SignedEngineManifest` and Ed25519 verification are present in `src/services/engines.rs:132-151`. The `FileManifestProvider` in `components.rs:527-565` loads and verifies a signed manifest from the data directory. The `HybridManifestProvider` uses it as the primary source, falling back to the built-in.

**Still missing:**
* public key embedded in the app (env var only: `RAVYN_ENGINE_MANIFEST_PUBLIC_KEY`);
* no operational **remote** provider uses the signed manifest;
* no periodic remote refresh with ETag/channel identification;
* no downgrade/replay protection (manifest version/timestamp/expiration);
* no controlled remote refresh with fallback to last verified.

### 8. There is no real remote engine update

`update_available` (`components.rs:751-758`) correctly compares manifest version vs active version via `version_cmp`. But the manifest only changes when a new Ravyn binary ships with a different embedded manifest, or when the user places a local signed file.

**Still missing:**
* periodic manifest download from a remote URL;
* cache with ETag/Last-Modified;
* effectively updatable stable/beta channel switching;
* new version notification;
* refresh retry and fallback to the last verified manifest.

### ~~11. Declared capabilities are not truly verified~~ ✅

`rqbit_api_health()`, `ffmpeg_capability_check()`, `seven_zip_capability_check()`, and `ytdlp_capability_check()` (`src/services/components.rs`) each run a real functional probe from `ComponentManager::health_check`, not just a version banner:
* FFmpeg runs a minimal `lavfi` color source through a null muxer (`-f lavfi -i color=... -f null -`), proving encode/decode actually works;
* 7-Zip is handed a hand-built minimal ZIP archive and asked to `t` (test) it, proving real archive I/O;
* yt-dlp's `--help` output is checked for the option flags the adapter layer depends on (`--dump-single-json`, `--download-archive`, `--ffmpeg-location`, `--progress-template`).

A failed capability check now fails the health check (`healthy: false`) with a descriptive message, exactly like the existing rqbit check.

### ~~15. Cleaning and retention of engine versions are missing~~ ✅

`EngineManager::cleanup_versions()` (`src/services/engines.rs`) deletes every versioned directory for an engine except the active version and the single previous version kept for rollback/diagnostics (satisfying the max-one-diagnostic retention policy — this also covers failed-download versioned directories, since a failed `download_and_install` leaves its version directory un-adopted by `active.json`/`previous.json`), and separately deletes stale `.download` partial-download temp files even from directories that are kept. `ComponentManager::cleanup_component()` wraps it per component and is called automatically (best-effort, logged on failure, never fails the parent operation) after every successful install and rollback. It's also exposed directly via `POST /v1/components/{id}/cleanup`, which returns an `EngineCleanupReport` (removed versions, removed temp files, bytes freed).

### 17. In case of failed copy, the wrong executable may be registered

`integration.rs:111-128` guards against registering when `install_application` is true but `installed_exe` is None (copy failed). However, `effective_exe = installed_exe.clone().or(source_exe)` at line 130 can still fall back to the source executable when `install_application` was not requested but other steps were.

**Still missing:**
* explicit blocking of all dependent registrations when the copy step did not succeed for any reason.

### 19. Portable mode is only detected, not fully implemented

`installation.rs:54` correctly detects portable mode (`portable: !in_install_dir`). The setup frontend (`InstallationInfo`) surfaces it.

**Still missing:**
* explicit user choice during setup;
* data alongside the executable or configurable data-dir mode;
* no automatic data move to `%LOCALAPPDATA%`;
* updater compatible with portable mode.

### 🔶 22. Provisioning errors are now structured, most call sites converted

`RavynError::Provisioning { code, message, details, retryable }` (`src/error.rs`) carries a `ProvisioningErrorCode` (`MANIFEST_UNAVAILABLE`, `PLATFORM_UNSUPPORTED`, `INVALID_MANIFEST_SIGNATURE`, `CHECKSUM_MISMATCH`, `INSUFFICIENT_SPACE`, `DOWNLOAD_INTERRUPTED`, `QUARANTINED`, `HEALTH_CHECK_FAILED`, `ROLLBACK_FAILED`, `INVALID_CUSTOM_PATH`, `APP_INSTALL_FAILED` — each with its own `api_code()`, HTTP status, `FailureClass`, and default `retryable`) plus a `ProvisioningErrorDetails` struct (`component`, `stage`, `expected_version`, `detected_version`, `path`, `target`) that is now serialized into the API error response's `details` field (previously always `{}`). Built with a fluent `RavynError::provisioning(code, message).with_component(...).with_stage(...)` builder.

Converted call sites: `EngineManifest::artifact()` (platform unsupported), `SignedEngineManifest::verify()` (invalid signature), `EngineManager::install_verified()`/`download_and_install()` (checksum mismatch vs. download interrupted, now distinguished), `FileManifestProvider::load()` (manifest unavailable), `ComponentManager::install_component_with_progress()` and `rollback_component()` (health check / rollback failed, with component + stage + expected/detected version attached).

**Still missing:**
* insufficient space is only reachable indirectly via the existing OS `ENOSPC` → `FailureClass::DiskFull` IO-error path, not a proactive free-space precheck before download;
* antivirus/quarantine detection (no OS-level signal is currently probed);
* invalid custom path and failed app install are desktop/Tauri-layer concerns (`src-tauri/`), not yet wired to this backend error type.

### 24. End-to-end provisioning tests are missing

`tests/http_integration.rs` (795 lines) covers HTTP download scenarios with a mock server. Unit tests exist for engines, components, and storage.

**Still missing:**
* simulated HTTP manifest with mock server;
* real download from mock server;
* incorrect checksum;
* insecure redirect;
* cancellation at every stage;
* simultaneous installation;
* restart and activation of the new engine;
* update and rollback;
* rqbit spawn/readiness/crash;
* application installation;
* copy failure;
* launch of the installed copy;
* uninstall.

---

## ❌ NOT IMPLEMENTED — still needs to be built

### 🔶 1. The embedded manifest is now populated for 3 of 4 engines

`assets/engines/stable.json` now carries real, independently verified `x86_64-pc-windows-msvc` artifacts for `yt-dlp` (2026.07.04, raw `yt-dlp.exe` from the official GitHub release), `rqbit` (9.0.0-beta.2, raw `rqbit.exe`), and `ffmpeg` (a pinned BtbN static-build release, `autobuild-2026-07-13-14-11`, not the rolling `latest` tag). Every URL, size, and SHA-256 was checked against the real download (GitHub's server-computed asset digest, cross-checked with a local `Get-FileHash` after download) before being written into the manifest, and `embedded_manifest_parses_validates_and_covers_every_windows_engine` (`src/services/components.rs`) asserts the embedded manifest actually parses, validates, and resolves an artifact for each of these three engines on the Windows target at test time.

**Resolved:** the FFmpeg/archive distribution question — `EngineArtifact` gained `archive_member`/`member_sha256` fields (`src/services/engines.rs`), and `EngineManager::install_verified()`/`download_and_install()` now extract and checksum-verify a single named member out of a downloaded ZIP (via the new `zip` crate dependency, run on a blocking task) instead of requiring the artifact to already be a bare executable. The activation checksum stored in `active.json` is the *member's* hash, not the archive's. FFmpeg's manifest entry uses this: the artifact `sha256`/`size_bytes` describe the 158 MB build archive, and `archive_member`/`member_sha256` point at and verify the extracted `bin/ffmpeg.exe` (138 MB) inside it.

**Still missing:**
* 7-Zip has no manifest entry. The official distribution offers no single-file Windows binary that both (a) requires no separate 7-Zip installation to unpack and (b) supports the ZIP format used by the item-11 capability probe: `7zr.exe` (the one genuinely raw GitHub release asset) only reads `.7z` archives, while the full `7za.exe` (which reads ZIP) ships only inside a `.7z`-compressed "Extra" package — a circular dependency until this crate can also read `.7z`, or the capability probe is changed to hand-build a minimal `.7z` test archive instead of a `.zip` one.

---

### 6. The release still publishes the CLI backend, not the desktop app

`.github/workflows/release.yml` copies only `target/release/ravyn.exe`. It does not build the frontend or the Tauri desktop executable.

**Missing:**
* frontend build;
* `ravyn-desktop` Tauri build;
* `RavynSetup.exe`;
* asset/icon resources;
* populated engine manifest;
* binary signing;
* installation smoke test;
* test launch of the installed copy;
* uninstallation test.

---

### ~~12. Manual rollback does not execute a full health check~~ ✅

The low-level `EngineManager::rollback()` (`engines.rs:471-495`) still only verifies the checksum, but `ComponentManager::rollback_component()` (`src/services/components.rs`) — the method actually used by `POST /v1/components/{id}/rollback` and by the automatic rollback-on-failed-install path — now runs the checksum swap, then the same `health_check()` used after a fresh install (process launch, version detection, and capability verification, including the rqbit HTTP check). If the restored version fails, it is deactivated instead of being reported as the active/verified version.

---

### 18. The backend does not know the result of the Windows installation

`IntegrationReport` remains in the Tauri/frontend layer and is not persisted in the backend setup state. `POST /v1/setup/complete` does not know if the app was copied, registration succeeded, or the mode is portable or installed.

**Needs persisted:**
* `installation_mode`, `installed_exe`, `installed_version`, `installed_sha256`;
* `integration_completed`, `integration_errors`;
* `restart/relaunch_pending`.

---

### 20. Real repair and application update do not exist yet

The release generates `ravyn-release.json` with checksums, but no code consumes it as an updater.

**Missing:**
* checking a remote release;
* verifying signed metadata;
* downloading a new application;
* replacing the executable;
* restarting;
* verifying readiness;
* rollback;
* repairing missing or corrupted files.

---

### 23. Privileged Tauri commands are not sufficiently constrained

The commands (`apply_windows_integration`, `finish_setup_handoff`, etc.) are available via the invoke handler without verification of:
* calling window;
* setup not yet completed;
* allowed mode;
* single execution;
* persisted consent.

The main window should not be able to freely call setup commands.

---

### 25. CI and release do not explicitly verify the entire desktop workspace

Workflows run `cargo` but do not include:
* `npm ci`;
* `npm run check`;
* `npm run test`;
* `npm run build`;

before the Tauri build. Windows tests on a clean machine (install → restart → readiness → download test → update → rollback → uninstall) are also missing.

---

# Exact order to complete it

The correct order now is:

1. ~~Populate the embedded manifest with real artifacts.~~ ✅ (partial: yt-dlp/rqbit/ffmpeg done, 7-Zip still blocked on a format decision)
2. **Add manifest generation, checksum, and signing pipeline.**
3. ~~Correct engine activation after provisioning via controlled restart.~~ ✅
4. ~~Implement `RqbitProcessManager` and HTTP health check.~~ ✅
5. ~~Actually launch the installed copy and close the original setup.~~ ✅
6. ~~Implement `--uninstall`.~~ ✅
7. **Make Ravyn executable install/update/rollback transactional.**
8. **Persist desktop installation mode and result in the setup backend.**
9. ~~Correct version comparison, detected version, and capability checks.~~ ✅
10. ~~Add global limit, cleanup, and close cancellation race conditions.~~ ✅ (partial for cleanup)
11. **Build the desktop in CI and publish it in the release.**
12. **Add end-to-end tests on Windows.**
13. ~~Apply per-process authentication and restrict Tauri commands.~~ ✅ (partial for command restriction)
14. **Update documentation and capability matrix.**

---

# Realistic final state

**The download backend core is not the main problem.** Scheduler, library, API, storage, reliability, and adapters are already very advanced.

The real remaining blocks are:

```text
real manifest
+ desktop release
+ uninstall/application update (uninstall done, update pending)
+ command restriction and CI verification
+ e2e tests
```

Until these blocks are resolved, Ravyn may have a powerful backend but not an installable and reliable desktop beta.

I also verified all migrations present in the ZIP:

* **23 migrations applied**;
* **32 tables**;
* **44 indexes**;
* `PRAGMA integrity_check: ok`.
