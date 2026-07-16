# Privacy notice

Ravyn does not include analytics, advertising, telemetry, remote executable code, or third-party tracking.

## Data processed

The extension may process the following data locally when a related feature is used:

- page URLs, link URLs, media URLs, page titles, MIME hints, and filename hints;
- Firefox download metadata needed for safe interception;
- optional session cookies for a site explicitly approved by the user;
- extension preferences and the list of origins approved for cookie access;
- recent Ravyn job summaries returned by the local desktop application.

## Storage and transmission

- Discovered resources are cached in extension memory and are not uploaded to Ravyn or any third party until the user delegates a download or enables an automatic interception mode.
- Private-window resources remain memory-only.
- Cookie values are read at submission time, sent through Firefox Native Messaging to the local Ravyn host, and never persisted in extension storage.
- The native host communicates only with the authenticated per-user Ravyn backend on loopback.
- Ravyn may contact the selected source website to perform the requested download. That network activity is part of the download requested by the user.

## Permissions

Cookie, network-observation, and broad host permissions are optional and requested only after an explicit user action. They can be revoked from the extension options or Firefox permission controls.

## User controls

The options page can disable automatic interception, media detection, overlays, network observation, notifications, and each cookie-approved origin. “Clear extension data” removes preferences, cached state, and optional permissions.
