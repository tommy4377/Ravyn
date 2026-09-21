# Ravyn

Ravyn is a high-performance download manager backend written in Rust.

## Core capabilities

- Segmented HTTP downloads with strict range validation and resume support.
- Persistent SQLite queue with pause, resume, cancel, retry and recovery.
- Global and per-host concurrency and bandwidth controls.
- yt-dlp, FFmpeg and rqbit integration.
- REST API, server-sent events, readiness checks and metrics.
- Checksum verification and post-processing pipelines.

## Run

```bash
cargo run --release -- --data-dir ./ravyn-data --listen 127.0.0.1:47821
```

The API binds to loopback by default.

## Development

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

## Maintainer

Maintained by [@tommy4377](https://github.com/tommy4377).

## License

Released under the [MIT License](LICENSE).
