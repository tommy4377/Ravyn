# Ravyn GitHub release process

Ravyn releases are published only through GitHub Releases. No external signing
service, certificate profile, or cloud signing secret is required.

## Cutting a release

1. Confirm the version in `Cargo.toml` and update the master project document.
2. Run the complete local gate documented in `AGENTS.md`.
3. Push an annotated `v<version>` tag.
4. Let `.github/workflows/release.yml` build the Windows, Linux, and macOS
   archives and publish the GitHub Release.
5. Verify the SHA-256 checksum files, CycloneDX SBOM, and GitHub attestations.

The workflow publishes portable archives directly to the tagged GitHub
Release. Older GitHub Releases remain available for manual rollback.
