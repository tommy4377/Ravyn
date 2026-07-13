# Stack
- Rust 2024 edition; package `ravyn` 0.2.0; declared MSRV 1.85.
- Axum 0.8, Tokio 1.49, Reqwest 0.13.4 with rustls ring provider, SQLx 0.8 SQLite, Tower HTTP 0.6, Serde, tracing.
- Additional: ed25519-dalek 2.2, chrono/chrono-tz, clap 4 (derive+env), sha2, hex, uuid (v4+serde), url, regex, quick-xml, futures-util, tokio-util, tokio-stream, keyring 3.6.3, percent-encoding.
- Platform: windows-sys 0.61 (Windows), libc 0.2 (Unix).
- Dev: Criterion 0.7 with async Tokio benchmarks; proptest; tempfile; cargo-fuzz workspace with 11 binaries.
- 92 Rust source files; 20 migrations; 29 database tables; 128 Axum/OpenAPI operations.
- External adapters: yt-dlp, FFmpeg, 7-Zip/ImageMagick-compatible converter, rqbit HTTP API.
- Managed engine infrastructure: `EngineManager` with SHA-256 verified download, atomic install, and rollback.
- Windows/PowerShell development environment; GitHub-only CI/releases with archives, checksums, SBOM, and GitHub attestations.
