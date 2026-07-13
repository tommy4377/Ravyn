# Automatic Engine Provisioning — Design Document

Date: 2026-07-13

## Overview

Ravyn relies on four external programs for core functionality: **yt-dlp** (media
extraction), **rqbit** (BitTorrent), **7-Zip** (archive extraction), and
**ffmpeg** (media transcoding/probing). Today the user must install these
manually. This document describes a silent, automatic provisioning mechanism
that downloads, verifies, and activates managed engine binaries on first
startup — without user intervention.

## Existing Infrastructure

Most of the required machinery already ships in
`src/services/engines.rs`:

| Capability | Method | Status |
|---|---|---|
| Download from HTTPS with redirect loop detection | `EngineManager::download_and_install` | Implemented |
| SHA-256 byte-level verification | Inside `download_and_install` and `install_verified` | Implemented |
| Atomic file replacement (cross-platform) | `atomic_replace` (Windows `ReplaceFileW` / Unix rename) | Implemented |
| Managed path resolution with integrity check | `EngineManager::active_path` | Implemented |
| Rollback to previous verified version | `EngineManager::rollback` | Implemented |
| Signed manifest verification (Ed25519) | `SignedEngineManifest::verify` | Implemented |
| Manifest and artifact schema validation | `EngineManifest::validate`, `EngineArtifact::validate` | Implemented |
| Executable permission setting | `set_executable` (Unix chmod / Windows no-op) | Implemented |

What is **missing** is the startup trigger that checks whether managed
engines are present and, when they are not, fetches and installs them.

## Target Platform Triples

The manifest must carry artifacts for every supported `(engine × platform)`
combination. The current target triples:

| OS | Arch | Triple |
|---|---|---|
| Windows | x86_64 | `x86_64-pc-windows-msvc` |
| Windows | aarch64 | `aarch64-pc-windows-msvc` |
| Linux | x86_64 | `x86_64-unknown-linux-gnu` |
| Linux | aarch64 | `aarch64-unknown-linux-gnu` |
| macOS | x86_64 | `x86_64-apple-darwin` |
| macOS | aarch64 | `aarch64-apple-darwin` |

A helper function resolves the compile-time target at runtime:

```rust
fn current_target() -> &'static str {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "aarch64") {
        "aarch64-pc-windows-msvc"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "unknown"
    }
}
```

## Manifest Strategy

### Option A: Built-in Manifest (recommended starting point)

A JSON manifest is compiled into the binary. Each release of Ravyn ships
with known-good URLs, checksums, and sizes for the latest stable versions
of every engine on every supported platform.

**Advantages:**
- No external dependency at startup; works offline.
- No additional infrastructure (domain, CDN, signing key distribution).
- No network-based attack surface for the provisioning step.

**Disadvantages:**
- Engine version updates require a Ravyn rebuild and release.
- The binary grows by ~20-40 KB (compressed manifest text).

### Option B: Remote Signed Manifest

A static JSON file hosted on HTTPS (e.g. `https://releases.ravyn.app/engines/stable.json`),
signed with Ed25519. The public key is embedded in the binary.
`SignedEngineManifest::verify` already handles verification.

**Advantages:**
- Engine versions can be updated without rebuilding Ravyn.
- A single manifest serves all platforms.

**Disadvantages:**
- Requires hosting infrastructure and a signing key.
- First startup still needs network access.
- Adds a trust anchor (the signing key in the binary).

### Option C: Hybrid (recommended long-term)

The built-in manifest is the fallback. On startup, Ravyn first attempts
to fetch the remote manifest. If the fetch succeeds and the signature
verifies, the remote manifest is used. If the fetch fails (offline,
firewall, DNS failure), the built-in manifest is used.

This gives the best of both worlds: automatic version updates when
connected, full functionality when offline.

## Startup Flow

The provisioning step integrates into the existing bootstrap sequence in
`src/lib.rs`:

```
1. Config::parse()
2. apply_managed_engine_paths()      ← existing: resolves managed paths
3. ensure_managed_engines()          ← NEW: downloads missing engines
4. prepare_directories()
5. Repository::connect()
6. ... rest of startup
```

Step 3 runs after config parsing (needs `data_dir` and timeout settings)
but before directory preparation and database connection (no dependencies
on those).

### `ensure_managed_engines` — pseudocode

```rust
async fn ensure_managed_engines(
    config: &mut Config,
    cancellation: &CancellationToken,
) -> Result<()> {
    let manager = EngineManager::new(&config.data_dir);
    let target = current_target();
    let manifest = load_manifest(config)?; // built-in or remote

    let engines: &[(&str, &mut PathBuf, &str)] = &[
        ("yt-dlp", &mut config.ytdlp, "yt-dlp"),
        ("ffmpeg", &mut config.ffmpeg, "ffmpeg"),
        ("7zip",   &mut config.seven_zip, "7z"),
        ("rqbit",  &mut config.rqbit, "rqbit"),
    ];

    for &(name, ref mut path, default_name) in engines {
        // User provided an explicit path → skip
        if *path != Path::new(default_name) {
            continue;
        }
        // Already installed and checksum-verified → skip
        if manager.active_path(name).await?.is_some() {
            continue;
        }
        // Find the artifact for this engine + platform
        match manifest.artifact(name, target) {
            Ok(artifact) => {
                tracing::info!(
                    engine = name,
                    version = artifact.version,
                    "downloading managed engine"
                );
                manager
                    .download_and_install(config, artifact, cancellation)
                    .await?;
                *path = manager.active_path(name).await?.unwrap();
            }
            Err(_) => {
                tracing::warn!(
                    engine = name,
                    target,
                    "no managed engine available; user must install manually"
                );
            }
        }
    }
    Ok(())
}
```

### Parallel Downloads

To keep first-startup fast, all four engines download concurrently:

```rust
use futures_util::stream::{self, StreamExt};

let pending: Vec<_> = engines
    .iter()
    .filter(|(name, path, default)| {
        *path == Path::new(default)
    })
    .filter(|(name, _, _)| {
        manager.active_path(name).await?.is_none()
    })
    .collect();

stream::iter(pending)
    .map(|(name, path, default)| {
        let manager = manager.clone();
        let config = config.clone();
        let manifest = manifest.clone();
        async move {
            let artifact = manifest.artifact(name, target)?;
            manager
                .download_and_install(&config, &artifact, &cancellation)
                .await?;
            *path = manager.active_path(name).await?.unwrap();
            Ok::<_, RavynError>(())
        }
    })
    .buffer_unordered(4) // max 4 concurrent downloads
    .try_collect::<Vec<_>>()
    .await?;
```

## Manifest Schema

The existing `EngineManifest` and `EngineArtifact` structs are sufficient.
Example built-in manifest:

```json
{
    "schema_version": 1,
    "channel": "stable",
    "artifacts": [
        {
            "engine": "yt-dlp",
            "version": "2024.12.13",
            "target": "x86_64-pc-windows-msvc",
            "url": "https://github.com/yt-dlp/yt-dlp/releases/download/2024.12.13/yt-dlp.exe",
            "sha256": "aa...",
            "size_bytes": 12345678,
            "filename": "yt-dlp.exe",
            "capabilities": ["media-extract"]
        },
        {
            "engine": "yt-dlp",
            "version": "2024.12.13",
            "target": "x86_64-unknown-linux-gnu",
            "url": "https://github.com/yt-dlp/yt-dlp/releases/download/2024.12.13/yt-dlp",
            "sha256": "bb...",
            "size_bytes": 9876543,
            "filename": "yt-dlp",
            "capabilities": ["media-extract"]
        },
        {
            "engine": "rqbit",
            "version": "8.0.0",
            "target": "x86_64-pc-windows-msvc",
            "url": "https://github.com/ikatson/rqbit/releases/download/v8.0.0/rqbit.exe",
            "sha256": "cc...",
            "size_bytes": 5432109,
            "filename": "rqbit.exe",
            "capabilities": ["torrent"]
        },
        {
            "engine": "7zip",
            "version": "24.09",
            "target": "x86_64-pc-windows-msvc",
            "url": "https://www.7-zip.org/a/7z2409-extra.7z",
            "sha256": "dd...",
            "size_bytes": 2345678,
            "filename": "7z.exe",
            "capabilities": ["extract"]
        },
        {
            "engine": "ffmpeg",
            "version": "7.1",
            "target": "x86_64-pc-windows-msvc",
            "url": "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
            "sha256": "ee...",
            "size_bytes": 87654321,
            "filename": "ffmpeg.exe",
            "capabilities": ["transcode", "probe"]
        }
    ]
}
```

For a complete manifest, each engine needs one artifact per supported
target triple (up to 6 platforms × 4 engines = 24 artifacts total).

## Edge Cases and Error Handling

| Scenario | Behavior |
|---|---|
| First startup, no engines present | Downloads all 4 in parallel; Ravyn is ready after ~30s on a fast connection |
| Engine already managed and verified | Skipped; `active_path` succeeds, no download |
| User set explicit `--ytdlp /custom/path` | Skipped; user's path takes precedence |
| Network unreachable | Warning logged; Ravyn starts but media/torrent/archive features are unavailable until engines are installed manually |
| Download timeout or partial | Temporary file deleted; error logged; other engines continue |
| Checksum mismatch after download | `download_and_install` returns error; temporary file cleaned up; no partial activation |
| Disk full during download | `download_and_install` fails; temporary file removed by cleanup |
| Manifest has no artifact for current platform | Warning logged; user must install manually |
| Antivirus quarantines the binary | `active_path` checksum fails; error logged; user can whitelist or use manual path |
| User runs `--no-auto-install` (future flag) | Skips `ensure_managed_engines` entirely |

## Security Considerations

1. **All downloads use HTTPS only.** `EngineArtifact::validate` rejects
   non-HTTPS URLs, URLs with credentials, and URLs with fragments.
2. **SHA-256 verification before activation.** Every byte is hashed during
   download and compared against the manifest checksum. No partial or
   truncated file is ever activated.
3. **Atomic replacement.** The binary is written to a temporary file,
   synced to disk, then atomically renamed over the destination (using
   `ReplaceFileW` on Windows). A crash during installation never leaves
   a half-written binary.
4. **Rollback.** The previous version's metadata is preserved in
   `previous.json`. If the new version fails verification, the manager
   can roll back to the last known-good version.
5. **No privilege escalation.** Engines run as the same user as Ravyn.
   No system-wide installation, no PATH manipulation, no admin rights.
6. **Confinement.** Managed engines are stored under `data_dir/engines/`,
   well within the data directory boundary. No writes outside the
   configured roots.
7. **Signed manifests (Option B/C).** `SignedEngineManifest::verify`
   uses Ed25519 strict verification. The public key is hardcoded. A
   compromised manifest without the correct signature is rejected.

## Performance Impact

- **Cold start (no engines):** ~30s on a 50 Mbps connection for ~100 MB
  total across 4 engines (downloaded in parallel).
- **Warm start (engines present):** Negligible — `active_path` performs
  one stat + one 64 KB hash read per engine (~5 ms total).
- **Disk space:** ~150 MB total for all managed engines (varies by
  platform and version).
- **Binary size increase:** ~20-40 KB for the built-in manifest
  (compressed JSON).

## Implementation Checklist

- [ ] Add `current_target()` helper to `src/services/engines.rs`
- [ ] Add `load_manifest(config) -> Result<EngineManifest>` (built-in or remote)
- [ ] Add `ensure_managed_engines(config, cancellation) -> Result<()>` to `src/lib.rs`
- [ ] Wire into bootstrap sequence between `apply_managed_engine_paths` and `prepare_directories`
- [ ] Add `--no-auto-install` CLI flag (optional, for users who prefer manual control)
- [ ] Create and maintain the built-in manifest with current stable versions
- [ ] Add unit tests for target resolution, manifest loading, and skip logic
- [ ] Add integration test: verify managed engine is downloaded and activated
- [ ] Document in README and COMPATIBILITY.md

## Future Considerations

- **Auto-update:** A supervised background task could periodically check
  the remote manifest for newer engine versions and offer or silently
  apply updates (with rollback on failure).
- **Progress reporting:** Expose download progress via the event system
  (`/v1/events`) so the frontend can show a progress bar during
  first startup.
- **Engine health checks:** Periodic verification that managed binaries
  still pass checksum (defense against disk corruption or antivirus
  tampering).
- **Selective installation:** Allow the user to disable specific engines
  via config (e.g. "I never use torrents, skip rqbit").
