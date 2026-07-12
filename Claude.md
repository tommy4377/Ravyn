# Ravyn repository instructions

- Work on the Rust backend only unless the user explicitly expands the scope.
- Use every relevant configured MCP server when it is available. Prefer Serena for semantic code navigation and editing, Context7 for current library documentation, Svelte Docs and shadcn-svelte for future Svelte work, and the Tauri MCP for future Tauri integration/testing.
- If a relevant MCP server is unavailable in the current task, state that briefly and continue with the best local tooling rather than blocking.
- Treat `RAVYN_MASTER_PROJECT_DOCUMENT.md` as the authoritative implementation roadmap.
- Preserve security defaults: loopback binding, output-root confinement, private-network blocking, bounded inputs, and least privilege.
- Before claiming backend completion, run formatting, locked checks, Clippy with warnings denied, all tests, the HTTP integration test, and a locked release build.
