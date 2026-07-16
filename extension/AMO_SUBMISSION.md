# AMO submission notes

## Build

```bash
npm ci
npm run check
npm run build:source
```

Artifacts:

- `artifacts/ravyn-firefox-<version>.xpi` — deterministic unsigned package for validation or AMO upload.
- `artifacts/ravyn-firefox-source-<version>.zip` — human-readable source archive.

## Signing

Normal Firefox installations require a Mozilla-signed XPI. The release workflow signs an unlisted package when these repository secrets are configured:

- `RAVYN_AMO_API_KEY`
- `RAVYN_AMO_API_SECRET`

Equivalent local command:

```bash
npx web-ext sign \
  --source-dir dist/firefox \
  --artifacts-dir artifacts/signed \
  --api-key "$RAVYN_AMO_API_KEY" \
  --api-secret "$RAVYN_AMO_API_SECRET" \
  --channel unlisted \
  --upload-source-code artifacts/ravyn-firefox-source-<version>.zip \
  --no-input
```

## Reviewer summary

- No remote code or dynamically downloaded scripts.
- No telemetry, analytics, advertising, or data sale.
- Native Messaging target: `com.ravyn.download_manager`.
- Native protocol source: `src-tauri/src/native_messaging.rs`.
- Cookie access is optional, per-origin, and never persisted by the extension.
- Network observation is optional.
- Website content is inspected locally to identify resources selected for download.
- DRM circumvention is explicitly unsupported.

## Manual review checklist

- Run `web-ext lint` with warnings treated as errors.
- Compare the manifest version with the application and Cargo versions.
- Confirm the extension ID is `firefox-extension@ravyn.app`.
- Confirm optional permissions are requested only from user gestures.
- Exercise every fixture in `test-pages/`.
- Verify native registration is removed during Ravyn uninstall.
