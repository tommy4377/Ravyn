# Frontend map
- Real Svelte 5 (runes) + Vite + TypeScript app under `frontend/`, serving both the setup flow and the main application. Not a stub — full feature surface exists.
- `lib/api/` — `transport.ts` (fetch/timeout/abort), `client.ts` (typed per-endpoint methods), `types.ts` (mirrors backend contracts, keep in sync with `/openapi.json`), `errors.ts` (stable backend error codes; network failures map to `NETWORK_UNAVAILABLE`), `events.svelte.ts` (SSE).
- `lib/native/tauri.ts` is the only place `@tauri-apps/api` `invoke()` is called from app code — see `mem:desktop/core` for the current native command surface.
- `lib/stores/*.svelte.ts` — rune-based global stores: connection, jobs, navigation, notifications, selection. Colocated `*.test.ts` run under vitest.
- `lib/setup/` — setup flow (`SetupApp.svelte`, `controller.svelte.ts`, `stages/*Stage.svelte`, `installationPolicy.ts`, `componentStates.ts`).
- `lib/shell/` — main app chrome: `AppShell.svelte`, `NavigationView.svelte`, `CommandBar.svelte`, `StatusBar.svelte`, notification host/drawer, `ConnectionBoot.svelte`.
- Feature areas, each with a `*View.svelte` root and presentation-logic `.ts` files kept separate from markup for testability: `downloads/` (`mem:frontend/downloads`), `library/`, `media/`, `torrents/`, `automation/`, `basket/`, `settings/` (category-based, `SettingsCategoryHeader.svelte` + per-category `*Settings.svelte`), `diagnostics/`, `appearance/`.
- `lib/components/` — hand-built shared UI primitives (Fluent Design 2 styling), no external component-library dependency (see `Menu`/`ContextMenu`/`MenuButton` note in `mem:frontend/downloads`).
- Pattern for every feature: presentation logic (pure, unit-testable `.ts`) is separated from the `.svelte` view that wires it to stores/API — follow this split when adding features rather than putting logic inline in components.
