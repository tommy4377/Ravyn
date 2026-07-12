# Media lifecycle
- yt-dlp stdout machine records are parsed with a 1 MiB per-line ceiling using incremental `fill_buf`; do not restore `AsyncBufReadExt::lines`, which allocates an unbounded line.
- The exported per-job yt-dlp archive snapshot is guarded by `EphemeralFile` and must be removed on every exit path.
- Process-tree cancellation, wall timeout, bounded stderr collection, destination confinement, durable per-item state, archive deduplication, and retry-parent reconciliation are existing invariants.
- Independent queue execution for every playlist item remains a separate orchestration change; preserve parent cancellation and archive consistency when implementing it.