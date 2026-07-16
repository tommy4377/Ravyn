# Compatibility

## Supported release target

Ravyn's release-qualified desktop target is 64-bit Windows 10 and Windows 11
(`x86_64-pc-windows-msvc`). The Tauri shell, Windows integration, silent
application updater, MSI administrative component provisioning, and release
smoke tests are designed for that target.

The root Rust backend remains structured for other desktop operating systems,
but non-Windows builds are not release-qualified and Windows-specific setup,
updater, uninstall, registry, shortcut, and MSI features report unsupported
rather than silently emulating success.

## Runtime dependencies

The managed Windows catalogue supplies yt-dlp, FFmpeg, rqbit, and 7-Zip.
Explicit custom executable paths remain supported and take precedence over
managed defaults. The backend validates effective paths and component health
before marking a feature ready.

## Display and accessibility

The frontend supports light/dark appearance, reduced motion, forced-colors/high
contrast, keyboard navigation, responsive narrow windows, and DPI-independent
CSS sizing. Release visual QA should cover Windows 10/11 at 100%, 125%, 150%,
and 200% scaling.

## Data compatibility

SQLite schema upgrades are append-only migrations and are applied in order.
Managed engine activation metadata remains backward compatible with the older
version-named directory layout. Update and component metadata reads are size
bounded before JSON parsing.
