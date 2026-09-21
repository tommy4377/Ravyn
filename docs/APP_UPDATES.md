# Ravyn Application Updates

Ravyn uses a signed update protocol owned by the desktop shell. Installer URLs,
release notes, sizes, and checksums are trusted only after the complete update
manifest passes Ed25519 verification with the public key compiled into Ravyn.

## User flow

1. An installed Ravyn build checks the configured HTTPS endpoint after the
   backend and main webview are ready.
2. A newer signed NSIS release is streamed silently to the private application
   cache with a strict signed-size limit and incremental SHA-256 verification.
3. Settings exposes passive download and readiness state without interrupting
   the user.
4. On a normal close, Ravyn verifies the staged installer again and writes a
   durable update transaction.
5. A detached Windows helper waits for the old process to exit, retains the
   previous application binary in the update cache, runs the current-user NSIS
   installer with `/S`, and launches the installed executable.
6. The new process confirms readiness only after both the embedded backend and
   the main webview are operational.
7. If readiness is not confirmed within 180 seconds, or the new process exits,
   the helper stops it, restores the retained previous binary, and relaunches
   the previous version.
8. The helper persists a success, rollback, or failure result. Settings displays
   that result on the next launch.

Portable and development builds never self-update. Redirects are limited and
must remain on HTTPS. Metadata reads are bounded before parsing.

## Transaction files

Update state lives below the Tauri application cache in `updates/`:

- `ravyn-pending-update.exe` — verified staged NSIS installer;
- `ravyn-update-transaction.json` — bounded transaction contract shared with
  the detached helper;
- `.ravyn.update.previous.exe` — retained previous application binary;
- `ravyn-update-ready-<token>.marker` — one-shot readiness acknowledgement;
- `ravyn-update-result.json` — persisted outcome displayed in Settings.

The current rollback guarantee covers the main Ravyn executable. Full repair of
all installed files, registry entries, and uninstall metadata remains a
separate recovery feature and must be validated by Windows product E2E.

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

## Release requirements

- `Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and
  `frontend/package.json` must have the same version.
- The release tag must be exactly `v<version>`.
- The NSIS installer must use `currentUser` mode so silent installation does
  not require elevation.
- The update private/public key pair must match.
- Windows CI must exercise a real N-to-N+1 update, readiness acknowledgement,
  forced readiness failure, binary rollback, and result persistence before the
  updater is considered release-qualified.
