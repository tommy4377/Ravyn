# Ravyn

Ravyn is a Rust download manager focused on reliable transfers, managed media engines and an organized local download library.

## Highlights

- Segmented and resumable HTTP transfers.
- Persistent queue, schedules, rules, priorities and tags.
- Managed yt-dlp, FFmpeg and rqbit provisioning with integrity checks.
- Automatic library organization for videos, music, documents, images, archives and torrents.
- Duplicate detection, SHA-256 identity and verified local cache reuse.
- Presets, filename templates, basket workflows and library import/repair.
- REST API, replayable events, metrics and database backup support.

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
