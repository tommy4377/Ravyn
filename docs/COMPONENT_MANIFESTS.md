# Signed component manifest delivery

Ravyn provisions yt-dlp, FFmpeg, rqbit, and 7-Zip from a signed release
catalogue. Artifact URLs, versions, checksums, extraction members, installer
strategies, and size bounds are trusted only after the complete manifest passes
Ed25519 verification.

## Runtime configuration

Release builds configure:

- `RAVYN_COMPONENT_MANIFEST_ENDPOINT` — signed manifest HTTPS URL;
- `RAVYN_ENGINE_MANIFEST_PUBLIC_KEY` — 32-byte Ed25519 public key as hex;
- `RAVYN_COMPONENT_MANIFEST_CHANNEL` — normally `stable`;
- `RAVYN_COMPONENT_MANIFEST_REFRESH_SECS` — 300 to 604800 seconds;
- `RAVYN_COMPONENT_MANIFEST_STALE_GRACE_SECS` — bounded to 30 days.

A separately signed operator override may be stored at
`RAVYN_DATA_DIR/engines/manifest.json`. Invalid overrides fail closed. Remote
network/cache failures may use the still-valid last-known-good cache or the
catalogue embedded in the application.

## Artifact contract

A direct executable declares an exact `size_bytes`, artifact `sha256`, and safe
relative `filename`. A ZIP artifact additionally declares `archive_member` and
`member_sha256`; only that member is extracted and activated.

A publisher that does not expose a stable exact byte count may set
`size_bytes` to zero only when a signed `max_size_bytes` upper bound is present.
The stream is always rejected when it exceeds that bound and its final SHA-256
must still match exactly.

Package strategies are explicit and mutually exclusive with ZIP extraction.
The currently supported strategy is:

```json
{
  "filename": "Files/7-Zip/7z.exe",
  "installer": { "kind": "msi_administrative" }
}
```

On Windows, Ravyn invokes `msiexec.exe` directly with `/a`, `/qn`, and
`/norestart`, targeting a unique private candidate directory. No shell command
is constructed and no machine-wide package is registered. The produced
executable is required to exist, remain within the candidate directory, pass
the component health check, and receive a locally computed activation hash.

Every install uses a unique physical directory even when repairing the same
semantic version. Activation metadata is atomic and retains the previous
candidate, so a failed health check can roll back without the new files having
overwritten the old version.

## Remote manifest contract

Remote schema-1 manifests add a monotonic `manifest_version`, `generated_at`,
and `expires_at`. Ravyn rejects invalid signatures, wrong channels, replayed or
downgraded revisions, conflicting reuse of a revision, excessive validity
windows, unsafe paths, oversized metadata, and redirects that leave HTTPS.

Verified cache state is stored below:

`RAVYN_DATA_DIR/engines/manifests/<channel>/`

Conditional requests use ETag and Last-Modified. Cache activation is an atomic,
rollback-capable metadata transaction.

## Release validation

`tools/validate_component_manifest.py` validates catalogue metadata on every
backend CI run. Nightly and tagged Windows workflows additionally download all
selected artifacts, enforce signed size/hash data, inspect ZIP members, execute
fixed installer provisioning strategies, and verify that the expected Windows
executable is produced.

Tagged releases add freshness metadata, sign the catalogue, verify the
signature with the matching public key, and publish the immutable signed file.
The private signing key is never committed or bundled.
