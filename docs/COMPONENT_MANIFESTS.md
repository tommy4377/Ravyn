# Signed component manifest delivery

Ravyn can provision yt-dlp, FFmpeg, rqbit, and any future managed engines from
a signed release catalogue. The application never trusts an artifact URL,
version, checksum, or archive member until the complete manifest has passed
Ed25519 verification.

## Runtime configuration

Release builds configure:

- `RAVYN_COMPONENT_MANIFEST_ENDPOINT`: HTTPS URL of the signed JSON document.
- `RAVYN_ENGINE_MANIFEST_PUBLIC_KEY`: 32-byte Ed25519 public key encoded as 64
  hexadecimal characters and embedded at compile time.
- `RAVYN_COMPONENT_MANIFEST_CHANNEL`: currently `stable`.
- `RAVYN_COMPONENT_MANIFEST_REFRESH_SECS`: refresh cadence, 300–604800 seconds.
- `RAVYN_COMPONENT_MANIFEST_STALE_GRACE_SECS`: bounded last-known-good grace,
  at most 30 days; the default is seven days.

An operator may place a separately signed override at
`RAVYN_DATA_DIR/engines/manifest.json`. An invalid override is treated as a
configuration error. Remote cache corruption, expiry beyond the grace period,
or network failure instead falls back to the catalogue compiled into Ravyn.

## Remote manifest contract

Remote manifests use schema version 1 and include all of the following fields:

```json
{
  "schema_version": 1,
  "channel": "stable",
  "manifest_version": 1783987200,
  "generated_at": "2026-07-14T00:00:00Z",
  "expires_at": "2026-08-13T00:00:00Z",
  "artifacts": []
}
```

`manifest_version` is monotonic. `generated_at` may not be unreasonably far in
the future, `expires_at` must follow it, and the validity window may not exceed
90 days. Ravyn rejects:

- invalid signatures;
- channel mismatches;
- lower manifest versions;
- older generation timestamps;
- reuse of a manifest version with different signed content;
- HTTPS redirects to a non-HTTPS URL;
- metadata bodies larger than 1 MiB;
- caches older than the configured last-known-good grace period.

## Cache behavior

The verified cache is stored below:

`RAVYN_DATA_DIR/engines/manifests/<channel>/`

It contains the signed manifest and bounded metadata with ETag, Last-Modified,
payload digest, release sequence, validity timestamps, and check/update times.
Conditional GET requests use ETag and Last-Modified. Cache activation updates
the manifest and metadata as one rollback-capable transaction, so a failed
write does not destroy the previous known-good catalogue.

The backend exposes:

- `GET /v1/components/manifest` — current source, freshness, revision, and
  refresh error.
- `POST /v1/components/manifest` — force a new verified refresh.

The Components screen consumes both routes and never reports a refresh as
successful until signature, freshness, replay, and atomic-cache checks pass.

## Release workflow

Tagged Windows releases require these GitHub secrets/variables:

- secret `RAVYN_ENGINE_MANIFEST_PRIVATE_KEY`;
- variable `RAVYN_ENGINE_MANIFEST_PUBLIC_KEY`.

The release workflow adds a monotonic release sequence and 30-day validity
window to `assets/engines/stable.json`, signs it with `manifest-tool`, verifies
the result with the public key, publishes `ravyn-component-manifest.json`, and
embeds the matching endpoint and public key into the desktop build.

The private key must never be committed, printed, bundled, or copied into a
runtime environment.
