# Release checklist

## Cross-platform source gates

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
python tools/static_source_audit.py
python -m unittest tools/test_validate_component_manifest.py
python tools/validate_component_manifest.py
npm ci --prefix frontend
npm run check --prefix frontend
npm test --prefix frontend
npm run build --prefix frontend
npm ci --prefix extension
npm run check --prefix extension
npm run package:verify --prefix extension
```

## Windows release gates

- Run `tools/validate_update_helper.ps1` and require all three lifecycle
  scenarios to pass.
- Run component validation with `--provision --target
  x86_64-pc-windows-msvc`.
- Build the signed NSIS updater artifact and MSI distribution artifact.
- Install silently as the current user and wait for the desktop readiness
  marker and `/health/ready` response.
- Verify setup, application launch, Installed Apps registration, repair-safe
  update state, and uninstall.
- Verify light/dark/high-contrast UI at 100%, 125%, 150%, and 200% scaling.
- Verify all release versions and the `v<version>` tag match.
- Verify app-update and component-manifest signing keys are distinct and match
  their compiled public keys.


## Firefox extension release gates

- Verify the application, desktop, frontend, extension manifest, and release tag versions match.
- Require `web-ext lint --warnings-as-errors` to pass.
- Build the XPI and source archive twice and require identical SHA-256 hashes.
- Sign tagged releases through AMO using the unlisted channel and upload the human-readable source archive.
- Install the signed XPI in a clean Firefox 142+ profile.
- Verify installed-mode Native Messaging registration, repair, and uninstall cleanup.
- Exercise disabled, rules-only, ask, and all-compatible interception modes.
- Verify a failed native handoff resumes the Firefox download.
- Verify link, image, media, selection, page, dynamic DOM, nested-frame, HLS, and DASH fixture cases.
- Verify optional cookie and network permissions are requested from user gestures and can be revoked independently.
- Verify container and private-window metadata without persistent private resource caches.
- Verify protected media is reported as unsupported and no DRM-bypass behavior exists.
- Verify malformed, oversized, and unknown native messages are rejected.
