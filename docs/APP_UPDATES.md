# Ravyn Application Updates

Ravyn uses a signed update protocol owned by the desktop shell. Installer URLs,
release notes, sizes, and checksums are trusted only after the complete update
manifest passes Ed25519 verification with the public key compiled into Ravyn.

## User flow

1. An installed Ravyn build restores any valid staged update and recovers an
   interrupted update transaction after the embedded backend starts.
2. Ravyn checks the configured HTTPS endpoint immediately and then every six
   hours. Transient failures retry after 15, 30, 60, and at most 120 minutes.
3. A newer signed NSIS release is streamed silently to the private application
   cache with a strict signed-size limit and incremental SHA-256 verification.
4. Settings shows passive check, download, cancellation, readiness, and result
   state. The user can stop an active transfer or discard a staged installer.
5. By default, a verified installer waits until Ravyn closes normally. The user
   may instead choose **Restart and install** to begin the same transaction
   immediately.
6. Ravyn verifies the staged installer again and writes a durable update
   transaction before a detached Windows helper is started.
7. The helper waits for the old process to exit, retains the previous
   installation binaries, registry entries, and shortcuts, runs the current-user
   NSIS installer with `/S`, and launches the installed executable.
8. The new process confirms readiness only after both the embedded backend and
   the main webview are operational.
9. If readiness is not confirmed within 180 seconds, or the new process exits,
   the helper stops it, restores the retained installation state, and relaunches
   the previous version.
10. The helper persists a success, rollback, or failure result. Settings displays
    that result on the next launch.

Portable and development builds never self-update. Redirects are limited and
must remain on HTTPS. Metadata reads are bounded before parsing. Cancellation is
cooperative: network reads, file writes, persistence, and final activation all
check the same token, and partial files are removed.

## Scheduler state

The native status contract reports:

- the last completed check time;
- the next scheduled automatic check;
- the normal automatic interval;
- current and available versions;
- downloaded and expected bytes;
- whether installation is waiting for normal close;
- whether the staged package is a same-version repair;
- the most recent persisted update outcome.

A release that previously failed or rolled back is not retried automatically at
the same version. The user must explicitly choose **Check now** before Ravyn
will stage that version again.

## Transaction files

Update state lives below the Tauri application cache in `updates/`:

- `ravyn-pending-update.exe` — verified staged NSIS installer;
- `ravyn-pending-update.json` — signed manifest and staging metadata;
- `ravyn-update-transaction.json` — bounded transaction contract shared with
  the detached helper;
- `.ravyn.update.previous.exe` — legacy schema-2 backup removed during cleanup;
- `backup-<token>/` — schema-3 snapshot of application `.exe` and `.dll` files;
- `shortcuts-<token>/` and registry exports — integration rollback state;
- `ravyn-update-ready-<token>.marker` — one-shot readiness acknowledgement;
- `ravyn-update-journal.txt` — helper phase marker;
- `ravyn-update-result.json` — persisted outcome displayed in Settings.

The install directory also contains user data in the current layout. Therefore,
backup and rollback intentionally operate only on application binaries and
integration metadata; they never restore the entire directory and never erase
user data written after the update began.

## Release keys

Generate a dedicated keypair with the existing release tool:

```powershell
cargo run --release --bin manifest_tool -- keygen --out app-update-key.txt
```

Store the private key as the GitHub Actions environment secret
`RAVYN_APP_UPDATE_PRIVATE_KEY`, and the public key as the environment variable
`RAVYN_APP_UPDATE_PUBLIC_KEY`.

Use a key dedicated to application updates rather than reusing the managed
component-manifest key. Losing the private key means installed clients cannot
trust future releases without a manual reinstall.

## Build-time configuration

Tagged desktop builds receive these compile-time values from
`.github/workflows/release.yml`:

```text
RAVYN_APP_UPDATE_ENDPOINT
RAVYN_APP_UPDATE_PUBLIC_KEY
```

A local build without both values remains functional but reports application
updates as disabled.

## Signed feed

Tagged releases create `ravyn-app-update.json` with `manifest-tool
sign-app-update`. The signed payload contains:

- schema and stable channel;
- release version and publication timestamp;
- target (`windows-x86_64`);
- exact NSIS filename and HTTPS URL;
- installer size and SHA-256;
- optional release notes.

The workflow verifies the produced signature and installer before uploading
both to the immutable GitHub Release.

## CI validation

Windows CI regenerates the exact detached PowerShell helper from the Rust
source and parses it with the real PowerShell parser. It then builds disposable
mock old/new application binaries and a mock silent installer and executes three
complete transactions: an N-to-N+1 success, a forced readiness failure with
binary rollback, and a same-version repair. Each scenario verifies the installed
binary, transaction cleanup, and persisted result metadata.

The installed-app smoke test also waits for an opt-in desktop readiness marker
and calls the real unauthenticated loopback `/health/ready` endpoint. Merely
keeping `Ravyn.exe` alive is no longer sufficient: the database, download root,
progress writer, and task manager must report ready.

The marker is enabled only when the test runner sets
`RAVYN_DESKTOP_READY_FILE`. It contains no bearer token or other credential and
is removed by the workflow after validation.

## Release requirements

- `Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and
  `frontend/package.json` must have the same version.
- The release tag must be exactly `v<version>`.
- The NSIS installer must use `currentUser` mode so silent installation does
  not require elevation.
- The update private/public key pair must match.
- Windows CI must pass the N-to-N+1, forced-readiness rollback, and same-version
  repair lifecycle harness, plus the installed application readiness smoke test.
