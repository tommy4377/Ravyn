# Completion gates
Before claiming a **backend** milestone complete, run (all must pass):
1. `cargo fmt --all -- --check`
2. `cargo check --locked --all-targets`
3. `cargo clippy --locked --all-targets --all-features -- -D warnings`
4. `cargo test --locked --all-targets`
5. `cargo test --locked --test http_integration`
6. `cargo check --manifest-path fuzz/Cargo.toml --bins`
7. `cargo build --locked --release`
Also run relevant migration, destructive/fault, cancellation, corruption, and concurrency tests for affected domains.

Before claiming a **frontend/setup** milestone complete (`cd frontend` unless noted):
1. `npm run check` (svelte-check strict TS)
2. `npm run test` (vitest)
3. `npm run build` (production bundle)
4. A real Tauri smoke test of the affected flow (prefer `tauri-mcp` MCP tools) — verify all visible controls use real backend data (no mock/placeholder), and that setup-to-main-app handoff is deterministic.
5. `cargo fmt --all -- --check` / `cargo clippy ... -D warnings` still apply if `src-tauri/` Rust changed.

Per CLAUDE.md working rules: keep OpenAPI, migrations, event contracts, setup docs (`docs/SETUP_CAPABILITY_MATRIX.md`, `docs/COMPONENT_MANIFESTS.md`, `docs/APP_UPDATES.md`), and README synchronized with every completed change. Do not commit or push (see `mem:conventions`).
