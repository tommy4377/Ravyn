# Security policy

## Reporting a vulnerability

Report suspected vulnerabilities privately through GitHub Security Advisories
("Report a vulnerability" on the repository's Security tab). Do not open a
public issue for an unpatched vulnerability.

Include, where possible: the affected version or commit, a reproduction, the
impact you believe it has, and any suggested fix. You will receive an
acknowledgement within 7 days and a triage decision within 30 days.

## Scope

Ravyn's security model assumes a single trusted operator on the local machine:

- the API binds to loopback by default; exposing it beyond loopback is the
  deployer's responsibility and requires the authentication token support that
  Ravyn ships;
- downloaded content is untrusted input: parsers (Metalink, rqbit JSON,
  yt-dlp output, archives) are bounded and fuzzed, and file outputs are
  confined to the configured output root;
- private-network and special-address targets are blocked by default for
  remote-supplied URLs;
- external tools (yt-dlp, FFmpeg, 7-Zip, rqbit) run under a supervisor with
  wall-clock, CPU, memory, output-size, and process-tree limits, but they are
  not fully sandboxed; only run engine binaries you trust. Managed-engine
  manifests are signature- and checksum-verified before activation.

Reports about weakening any of the above defaults are in scope. Denial of
service that requires local API access is generally out of scope because the
API is local-trust by design.

## Supported versions

Only the most recent tagged release receives security fixes. Fixes are
published as a new patch release with a GitHub Security Advisory; there are no
backports to older tags.
