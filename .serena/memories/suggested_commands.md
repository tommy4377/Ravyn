# Commands
## Backend (repo root)
- Run: `cargo run --locked -- --data-dir ./ravyn-data --listen 127.0.0.1:47821`.
- Fast backend tests: `cargo test --locked --all-targets`.
- HTTP integration: `cargo test --locked --test http_integration`.
- Single test by name filter: `cargo test --locked <test_name>`.
- Bench: `cargo bench --locked --bench transfer_policy`.
- Fuzz compile: `cargo check --manifest-path fuzz/Cargo.toml --bins`.
- Format check: `cargo fmt --all -- --check`.
- Clippy strict: `cargo clippy --locked --all-targets --all-features -- -D warnings`.
- Release build: `cargo build --locked --release`.

## Frontend (`cd frontend`)
- Install: `npm install`.
- Typecheck: `npm run check` (svelte-check, strict TS).
- Tests: `npm run test` (vitest run); single file: `npx vitest run src/lib/stores/jobs.test.ts`.
- Build: `npm run build`; dev server: `npm run dev`.

## Desktop shell
- `cargo build -p ravyn-desktop`; run built exe at `target/debug/ravyn-desktop.exe`.
- Before claiming frontend/setup completion: also do a real Tauri smoke test (prefer `tauri-mcp` MCP tools) and verify all visible controls use real backend data with a deterministic setup-to-main-app handoff.

## Search/navigation
- Search: `rg <pattern> <path>`; enumerate: `rg --files`.
- Prefer Serena symbol/structure/reference tools for source navigation and Git MCP for repository operations.
