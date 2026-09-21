# Contributing

Contributions that improve reliability, performance, accessibility, platform integration or documentation are welcome.

1. Fork the repository and create a focused branch.
2. Keep changes scoped and avoid committing generated build artifacts.
3. Run the relevant validation commands before opening a pull request.
4. Explain the user-facing behavior, testing performed and any migration or compatibility impact.

For Rust changes, run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings` and the relevant tests.

For frontend or extension changes, also run the corresponding npm checks documented in the README.
