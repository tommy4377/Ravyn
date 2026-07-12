# Stack
- Rust 2024 edition; package `ravyn` 0.2.0; declared MSRV 1.85.
- Axum 0.8, Tokio 1.49, Reqwest 0.13.4 with rustls ring provider, SQLx 0.8 SQLite, Tower HTTP 0.6, Serde, tracing.
- Criterion 0.7 with async Tokio benchmarks; proptest; cargo-fuzz workspace with 11 binaries.
- External adapters: yt-dlp, FFmpeg, 7-Zip/ImageMagick-compatible converter, rqbit HTTP API.
- Windows/PowerShell development environment; GitHub-only CI/releases with archives, checksums, SBOM, and GitHub attestations.