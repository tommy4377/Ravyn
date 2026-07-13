<script lang="ts">
  import { RavynClient } from "../api/client";
  import InlineError from "../components/InlineError.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import { backendInfo, mainWindowReady } from "../native/tauri";
  import { describeError } from "../setup/controller.svelte";
  import type { SetupState } from "../api/types";

  // Minimal main-window slice: verify the real backend connection, report
  // readiness for the deterministic setup handoff, and show real state.
  // The full application shell is Phase 3 of the frontend plan.
  let connection = $state<"connecting" | "ready" | "error">("connecting");
  let error = $state("");
  let setup = $state<SetupState | null>(null);

  async function connect() {
    connection = "connecting";
    error = "";
    try {
      const backend = await backendInfo();
      const client = new RavynClient(backend.base_url);
      setup = await client.getSetupState();
      connection = "ready";
      await mainWindowReady();
    } catch (cause) {
      error = describeError(cause);
      connection = "error";
      // Still show the window so the user sees the error instead of nothing.
      try {
        await mainWindowReady();
      } catch {
        // The setup window may already be gone; ignore.
      }
    }
  }

  $effect(() => {
    void connect();
  });
</script>

<div class="main">
  {#if connection === "connecting"}
    <div class="center">
      <div class="box">
        <ProgressBar value={null} label="Connecting to the Ravyn backend" />
        <p>Connecting…</p>
      </div>
    </div>
  {:else if connection === "error"}
    <div class="center">
      <div class="box">
        <InlineError
          title="Cannot reach the Ravyn backend"
          message={error}
          retry={() => void connect()}
        />
      </div>
    </div>
  {:else}
    <header class="titlebar">
      <h1>Ravyn</h1>
      <span class="status" role="status">Connected</span>
    </header>
    <section class="content">
      <p class="hint">
        Setup is complete. The main application shell arrives in the next
        milestone.
      </p>
      <dl>
        <dt>Backend version</dt>
        <dd>{setup?.app_version}</dd>
        <dt>Library</dt>
        <dd>{setup?.library_root ?? "Not configured"}</dd>
        <dt>Data folder</dt>
        <dd>{setup?.data_dir}</dd>
      </dl>
    </section>
  {/if}
</div>

<style>
  .main {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .center {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .box {
    width: min(460px, 85%);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    text-align: center;
    color: var(--text-secondary);
  }
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-6);
    border-bottom: 1px solid var(--stroke-divider);
    background: var(--bg-layer-alt);
  }
  h1 {
    margin: 0;
    font-size: var(--text-subtitle);
    font-weight: 600;
  }
  .status {
    font-size: var(--text-caption);
    color: var(--status-success);
  }
  .content {
    padding: var(--space-6);
  }
  .hint {
    color: var(--text-secondary);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-4);
    font-size: var(--text-body);
  }
  dt {
    color: var(--text-secondary);
  }
  dd {
    margin: 0;
    word-break: break-all;
  }
</style>
