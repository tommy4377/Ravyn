Before claiming backend completion, run all gates:
1. `cargo fmt --all -- --check`
2. `cargo check --locked --all-targets`
3. `cargo clippy --locked --all-targets --all-features -- -D warnings`
4. `cargo test --locked --all-targets`
5. `cargo test --locked --test http_integration`
6. `cargo build --locked --release`
Also validate migrations and relevant destructive/integration behavior for schema or lifecycle changes. Do not call beta/production ready unless the roadmap definitions are satisfied.