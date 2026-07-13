# Ravyn map
- Rust 2024 backend download manager (`src/`) + Tauri 2 desktop shell (`src-tauri/`) + Svelte 5/TS frontend (`frontend/`, no SvelteKit).
- Entrypoints: `src/main.rs` binary and `src/lib.rs` application assembly.
- Backend domain map and persistence invariants: `mem:backend/core`.
- Library features (15-feature persistent library, presets, basket, profiles, trust, cleanup, statistics): `mem:backend/library`.
- Frontend architecture, layering, and conventions: `mem:frontend/core`. Downloads-slice-specific rules (permitted actions, event coalescing, destructive-action labeling): `mem:frontend/downloads`.
- Dependency/toolchain pins: `mem:tech_stack`.
- Repository conventions and security defaults: `mem:conventions`.
- Commands: `mem:suggested_commands`; completion gates: `mem:task_completion`.
- `RAVYN_MASTER_PROJECT_DOCUMENT.md` is a roadmap to reconcile against code/tests, never an unquestioned status authority.
