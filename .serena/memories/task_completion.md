# Completion gates
Before claiming a backend milestone complete, run:
1. `cargo fmt --all -- --check`
2. `cargo check --locked --all-targets`
3. `cargo clippy --locked --all-targets --all-features -- -D warnings`
4. `cargo test --locked --all-targets`
5. `cargo test --locked --test http_integration`
6. `cargo check --manifest-path fuzz/Cargo.toml --bins`
7. `cargo build --locked --release`
Also run relevant migration, destructive/fault, cancellation, corruption, and concurrency tests for affected domains. Update `RAVYN_MASTER_PROJECT_DOCUMENT.md` with exact evidence and distinguish complete foundations, blockers, and unfinished work.