# Ravyn

Ravyn is a Rust download manager with a native Windows desktop application built with Tauri 2 and Svelte 5.

## Product surfaces

- **Rust backend** — transfers, queueing, engines, automation, library and HTTP API.
- **Tauri desktop** — native setup, lifecycle and Windows integration.
- **Svelte frontend** — downloads, library, media, torrents, automation and settings.

## Highlights

- Resumable segmented transfers with persistent recovery.
- Managed yt-dlp, FFmpeg and rqbit engines.
- Organized download library, presets, templates and basket workflows.
- Native Windows setup and application integration.
- Fluent-inspired desktop interface with responsive list/detail layouts.
- REST API and replayable server-sent events.

## Build

```bash
npm ci --prefix frontend
npm run check --prefix frontend
npm test --prefix frontend
npm run build --prefix frontend

cargo test --locked --workspace --all-targets
cargo build --locked -p ravyn-desktop
```

## Maintainer

Maintained by [@tommy4377](https://github.com/tommy4377).

## License

Released under the [MIT License](LICENSE).
