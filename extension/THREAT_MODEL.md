# Threat model

## Trust boundaries

1. Web pages and downloaded metadata are untrusted.
2. Content scripts are less privileged than the background context.
3. Firefox Native Messaging is the only extension-to-desktop transport.
4. The short-lived native host is less privileged than the full Ravyn backend API.
5. The authenticated loopback descriptor is per-user runtime state and must not be accepted from another process or origin.

## Primary threats and mitigations

### Malicious page input

- Only credential-free HTTP and HTTPS URLs are accepted.
- Control characters, oversized strings, path separators in filename hints, malformed UUIDs, excessive batches, and excessive cookie counts are rejected.
- DOM text is assigned through `textContent`; extension pages do not render remote HTML.
- Resource caches are bounded by count and age.

### Privilege expansion through Native Messaging

- The protocol has an explicit command allow-list and version field.
- Messages are length-prefixed and limited to 1 MiB.
- Arbitrary API paths, SQL, local paths, executable paths, shell commands, and FFmpeg arguments are forbidden.
- Post-processing is limited to named Ravyn presets.
- Source context must identify Firefox and every URL is revalidated in Rust.

### Stolen backend credentials

- The desktop writes the loopback endpoint and bearer token to a per-user runtime descriptor.
- The descriptor is permission-restricted on Unix and stored below the current user application-data directory on Windows.
- The host accepts only loopback HTTP endpoints and validates the authenticated backend readiness endpoint.
- The descriptor is removed when the backend stops.

### Download interception loss

- Firefox is paused before handoff.
- Ravyn must return a persisted job before Firefox is cancelled.
- Any timeout, rejection, host disconnect, or backend failure resumes the Firefox download.
- Extension-created downloads and recently delegated hashes are ignored to prevent loops.

### Cookie exposure

- Cookie access requires optional permission and an explicit per-origin grant.
- Only cookies matching the download host are forwarded.
- Values are never written to extension storage or logs.
- Revoking one origin does not revoke unrelated approved origins.

### Private browsing

- Incognito state and container identity are propagated as metadata.
- Page-resource caches are memory-only.
- Private browsing support can be disabled in extension settings.

### Protected media

The extension does not attempt to bypass DRM, decrypt encrypted media, extract CDM secrets, or reconstruct protected streams. Media overlays become disabled when encrypted-media activity is detected.

## Out of scope

- A fully compromised Firefox profile or operating-system account.
- Malware with access to the same user account.
- Trustworthiness or legality of user-selected download sources.
- Circumvention of website access controls or copyright protection.
