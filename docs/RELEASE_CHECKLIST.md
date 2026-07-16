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
