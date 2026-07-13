<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import InlineError from "../../components/InlineError.svelte";
  import { COMPONENT_LABEL, COMPONENT_STATE_UI } from "../componentStates";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  const componentSummary = $derived(
    (controller.overview?.components ?? []).map((component) => ({
      id: component.component,
      enabled: component.enabled,
      state: component.state,
    })),
  );

  const attention = $derived(
    componentSummary.filter((c) => c.enabled && c.state === "failed").length,
  );
</script>

<StageShell
  title="Ravyn is ready"
  subtitle={attention > 0
    ? `${attention} component${attention > 1 ? "s" : ""} requires attention. You can retry from Settings › Components.`
    : "Everything you selected is installed."}
  showBack={false}
  onnext={() => void controller.openRavyn()}
  nextLabel="Open Ravyn"
>
  {#if controller.stepError}
    <InlineError
      title="Could not open Ravyn"
      message={controller.stepError}
      retry={() => void controller.openRavyn()}
    />
  {/if}

  <div class="summary">
    <p class="label">Your library</p>
    <p class="value">{controller.libraryPath || "Not configured"}</p>
  </div>

  <div class="components">
    {#each componentSummary as component (component.id)}
      <div class="row">
        <span>{COMPONENT_LABEL[component.id]}</span>
        <span
          class="state"
          class:ok={component.state === "installed" ||
            component.state === "custom_path"}
          class:error={component.state === "failed"}
        >
          {component.enabled
            ? COMPONENT_STATE_UI[component.state].label
            : "Not selected"}
        </span>
      </div>
    {/each}
  </div>
</StageShell>

<style>
  .summary {
    padding: var(--space-4);
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-layer);
    background: var(--bg-layer);
  }
  .label {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
  .value {
    margin: var(--space-1) 0 0;
    word-break: break-all;
  }
  .components {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-layer);
    background: var(--bg-layer);
  }
  .row {
    display: flex;
    justify-content: space-between;
  }
  .state {
    color: var(--text-secondary);
  }
  .state.ok {
    color: var(--status-success);
  }
  .state.error {
    color: var(--status-error);
  }
</style>
