# Ravyn Application Updates

Ravyn uses a small signed update protocol owned by the desktop shell. It does
not trust an installer URL or checksum unless the complete update manifest has
first passed Ed25519 verification with the public key compiled into the app.

## User flow

1. An installed Ravyn build checks the configured HTTPS endpoint after the
   main window is ready.
2. A newer signed release is downloaded silently to the application cache.
3. The installer size and SHA-256 are verified while streaming to disk.
4. Settings shows the passive update state. No modal interrupts the user.
5. When the main window is closed normally, a detached Windows helper waits
   for Ravyn to exit, runs the current-user NSIS installer with `/S`, and
   relaunches the installed executable.
6. Portable and development builds never self-update.

The staged installer is verified again immediately before the helper starts.
Redirects are limited, and every redirect must remain on HTTPS.

## Release keys

Generate a dedicated keypair with the existing release tool:

```powershell
cargo run --release --bin manifest_tool -- keygen --out app-update-key.txt
```

Store the first value as the GitHub Actions environment secret:

```text
RAVYN_APP_UPDATE_PRIVATE_KEY
```

Store the second value as the GitHub Actions environment variable:

```text
RAVYN_APP_UPDATE_PUBLIC_KEY
```

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
