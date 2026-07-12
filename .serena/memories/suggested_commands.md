# Commands
- Run: `cargo run --locked -- --data-dir ./ravyn-data --listen 127.0.0.1:47821`.
- Fast backend tests: `cargo test --locked --all-targets`.
- HTTP integration: `cargo test --locked --test http_integration`.
- Bench: `cargo bench --locked --bench transfer_policy`.
- Fuzz compile: `cargo check --manifest-path fuzz/Cargo.toml --bins`.
- Search: `rg <pattern> <path>`; enumerate: `rg --files`.
- Prefer Serena symbol/structure/reference tools for source navigation and Git MCP for repository operations.