# Ravyn

Ravyn is a native Windows download manager with a Rust backend, a Svelte/Tauri desktop client and Firefox integration.

## Highlights

- Segmented HTTP transfers, resume, retries and bandwidth controls.
- yt-dlp media downloads, rqbit torrents and FFmpeg post-processing.
- Persistent organized library with automation, presets and schedules.
- Native Windows desktop shell and self-installing single-executable distribution.
- Firefox Manifest V3 extension with Native Messaging.
- Resource discovery, download interception and context-menu actions.
- Loopback API with authenticated integrations, events, metrics and readiness checks.

## Windows distribution

Ravyn is distributed as a single `Ravyn.exe`. The executable can install per-user under `%LOCALAPPDATA%\Ravyn` or run portably.

## Firefox extension

```bash
npm ci --prefix extension
npm run check --prefix extension
npm run package:verify --prefix extension
```

## Maintainer

Maintained by [@tommy4377](https://github.com/tommy4377).

## License

Released under the [MIT License](LICENSE).
