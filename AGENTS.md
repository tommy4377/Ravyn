# Ravyn repository instructions

- Work on the Rust backend only unless the user explicitly expands the scope.
- Use every relevant configured MCP server when it is available. Prefer MCP tools over raw shell search, whole-file reading, or memory when the tool can perform the task more efficiently.
- `serena`: activate this project, run its onboarding when needed, then use its LSP-backed symbol search, references, structure, and targeted editing for both Rust and Svelte/TypeScript rather than scanning whole files.
- `git`: use the Git MCP server for repository status, diffs, history, branches, staging, and commits instead of raw Git commands when it is available.
- `sqlite`: use the SQLite MCP server to inspect and query Ravyn's download-history database. Its default development location is `ravyn-data/ravyn.sqlite3`; respect `RAVYN_DATA_DIR` when the running application overrides it.
- `context7`: resolve libraries and retrieve current, version-specific API documentation before implementing against third-party Rust, Tauri, Svelte, or TypeScript libraries.
- `svelte-docs`: use the official Svelte documentation tools for current Svelte and SvelteKit guidance.
- `shadcn-svelte`: use it to discover shadcn Svelte components, dependencies, examples, and installation details before hand-authoring equivalents.
- `tauri-mcp`: use it to drive, inspect, test, and debug a running Tauri application (UI, DOM, IPC, and logs) when the Tauri bridge/plugin is installed and the app is running.
- If a relevant MCP server is unavailable in the current task, state that briefly and continue with the best local tooling rather than blocking.
- Treat `RAVYN_MASTER_PROJECT_DOCUMENT.md` as a roadmap to verify against the code, tests, and current dependencies; do not assume its status claims are correct when repository evidence differs.
- Preserve security defaults: loopback binding, output-root confinement, private-network blocking, bounded inputs, and least privilege.
- Before claiming backend completion, run formatting, locked checks, Clippy with warnings denied, all tests, the HTTP integration test, and a locked release build.
