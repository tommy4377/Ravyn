# Contributing to Ravyn

Ravyn handles downloads, local files, browser integration and operating-system registration, so correctness and security matter more than adding features quickly.

## Before opening a pull request

Run the checks relevant to your change:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets

npm ci --prefix frontend
npm run check --prefix frontend
npm test --prefix frontend

npm ci --prefix extension
npm run check --prefix extension
```

For transfer or storage changes, include tests for resume, cancellation, failure recovery and path handling where applicable. For browser integration, preserve the Native Messaging trust boundary and validate untrusted input.

Do not commit generated build output, release artifacts, logs, credentials, API tokens, local databases, screenshots used only for debugging, AI-agent memory/prompt files, temporary reports, copied third-party source trees or local editor state.

Use focused commit messages such as `feat(downloads): ...`, `fix(browser): ...`, `refactor(library): ...`, `test: ...` and `docs: ...`.
