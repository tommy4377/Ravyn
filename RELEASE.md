# Ravyn GitHub release process

Ravyn releases are published only through GitHub Releases. No external signing
service, certificate profile, or cloud signing secret is required.

## Release checklist

1. Confirm the version in `Cargo.toml` matches the tag you intend to push.
2. Update `RAVYN_MASTER_PROJECT_DOCUMENT.md` with the milestone evidence.
3. Run the complete local gate documented in `AGENTS.md`:
   fmt, locked check, strict clippy, all tests, HTTP integration tests,
   fuzz-target build, and the locked release build. For releases containing
   the library feature set, also verify all 20 migrations on a fresh database
   and exercise trash/restore, import/relocation, and cache reuse.
4. Review `cargo audit` / `cargo deny` results from the latest CI run.
5. Push an annotated `v<version>` tag.
6. Let `.github/workflows/release.yml` build the Windows, Linux, and macOS
   archives and publish the GitHub Release.
7. Verify the published assets: per-archive SHA-256 files, the CycloneDX SBOM,
   GitHub provenance/SBOM attestations, and `ravyn-release.json`.
8. Confirm `COMPATIBILITY.md` still describes the released API/database
   guarantees; note any newly deprecated behavior in the release notes.

## Updater metadata

Every release includes `ravyn-release.json`:

```json
{
  "schema": 1,
  "channel": "stable",
  "version": "1.2.3",
  "published_at": "2026-07-13T00:00:00Z",
  "source_commit": "<tagged commit sha>",
  "artifacts": [
    {"name": "ravyn-windows-x86_64.zip", "sha256": "..."}
  ]
}
```

Clients discover updates by fetching the latest release's metadata, comparing
`version`, and verifying the artifact's SHA-256 (and, optionally, its GitHub
attestation) before installing. Ravyn itself does not yet self-update: the
backend ships as portable archives, and in-place binary replacement is
deferred until a separately installed client can coordinate process
replacement safely. Managed external engines (yt-dlp/FFmpeg/rqbit-compatible)
already have verified install and checksum-verified rollback through the
managed-engine manifests.

## Rollback

Older GitHub Releases remain available permanently. To roll back, install the
previous release's archive (verify its checksum) and start Ravyn against the
same data directory. Database migrations are forward-only: if the newer
version added migrations, restore the pre-upgrade database backup taken by
Ravyn's backup flow before starting the older binary.

## Reproducibility

Builds are pinned by `Cargo.lock` (all release commands run `--locked`) and
compiled with `--remap-path-prefix` so the binaries do not embed workspace
paths. Binaries rebuilt from the same tag with the same toolchain are
byte-comparable; the surrounding `zip`/`tar.gz` containers may differ in
archive timestamps. When comparing a rebuild, compare the extracted binary,
not the archive.

## Host-side protections (not enforceable from this repository)

Branch protection for `master`, tag protection for `v*`, and a required
`release` environment with reviewers are GitHub repository settings. They must
be configured by a repository administrator; committed workflow YAML cannot
enforce them.
