# Ravyn repository instructions

- Current scope includes the Rust backend, the Tauri/Svelte frontend, and the custom Ravyn setup. Implement the setup first, then the main application. Do not work on the browser extension unless explicitly requested.
- Treat `RAVYN_FRONTEND_SETUP_DESIGN_PLAN.md` as the authoritative frontend/setup specification and `RAVYN_MASTER_PROJECT_DOCUMENT.md` as a roadmap that must always be verified against the current code, tests, migrations, routes, events, and dependencies.
- Build every frontend feature as a complete vertical slice: inspect the backend contract, implement the UI, connect the real API/Tauri command and events immediately, handle loading/empty/error/recovery states, add tests, and update documentation. Never leave production mock data, placeholder actions, or visual-only screens.
- Follow the existing project architecture and coding style. Write code comments in English and add comments only where they improve maintainability.
- Use every relevant configured MCP server when available. Prefer MCP tools over raw shell search, whole-file reading, or memory when they can perform the task more precisely.
- `serena`: activate the project, run onboarding when needed, and use its LSP-backed symbol search, references, structure, and targeted editing for Rust, Svelte, and TypeScript.
- `git`: use the Git MCP server for status, diffs, history, branches, staging, and commits.
- `sqlite`: inspect and query the development database at `ravyn-data/ravyn.sqlite3`; respect `RAVYN_DATA_DIR` when overridden.
- `context7`: resolve libraries and retrieve current, version-specific documentation before implementing against third-party Rust, Tauri, Svelte, or TypeScript APIs.
- `svelte-docs`: use the official Svelte documentation tools for current Svelte guidance.
- `shadcn-svelte`: inspect available components, dependencies, and examples before hand-authoring equivalents; do not adopt components that conflict with Ravyn's native Windows/Fluent design.
- `tauri-mcp`: drive, inspect, test, and debug the running Tauri app, including UI, DOM, IPC, events, and logs, when the bridge/plugin is available.
- If a relevant MCP server is unavailable, state that briefly and continue with the best local tooling rather than blocking.
- Preserve security defaults: loopback binding, output-root confinement, private-network blocking, bounded inputs, verified managed components, atomic replacement, least privilege, and no silent installation of disabled features.
- Keep OpenAPI, migrations, event contracts, capability maps, API-to-UI maps, setup documentation, and the master project document synchronized with every completed change.
- Before claiming backend completion, run formatting, locked checks, Clippy with warnings denied, all tests, the HTTP integration test, and a locked release build.
- Before claiming frontend/setup completion, run formatting, type checking, unit/integration tests, production builds, and a real Tauri smoke test. Verify that all visible controls use real backend data and that setup-to-main-app handoff is deterministic.
