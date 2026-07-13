<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import ProgressBar from "../../components/ProgressBar.svelte";
  import InlineError from "../../components/InlineError.svelte";
  import Button from "../../components/Button.svelte";
  import {
    COMPONENT_LABEL,
    COMPONENT_STATE_UI,
    formatBytes,
  } from "../componentStates";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  const componentRows = $derived(
    controller.requiredComponents().map((component) => ({
      component,
      progress: controller.componentProgress(component),
    })),
  );

  const busyStates = ["queued", "downloading", "verifying", "installing"];

  async function next() {
    if (await controller.completeSetup()) {
      controller.step = "done";
    }
  }
</script>

<StageShell
  title="Installing Ravyn"
  subtitle="Each operation below is a real backend task."
  showBack={false}
  onnext={next}
  nextLabel="Continue"
  nextDisabled={!controller.provisioningFinished}
  busy={controller.busy}
>
  {#if controller.stepError}
    <InlineError title="Installation problem" message={controller.stepError} />
  {/if}

  <!-- Windows integration steps -->
  {#if controller.integrationReport}
    <section class="group" aria-label="Application installation">
      {#each controller.integrationReport.steps as step (step.step)}
        <div class="row">
          <span class="row-title">{stepLabel(step.step)}</span>
          <span
            class="row-state"
            class:error={!!step.error}
            class:ok={step.applied}
          >
            {#if step.applied}
              Done
            {:else if step.error}
              Failed — {step.error}
            {:else}
              Skipped{step.skipped_reason ? ` — ${step.skipped_reason}` : ""}
            {/if}
          </span>
        </div>
      {/each}
    </section>
  {:else}
    <section class="group" aria-label="Application installation">
      <div class="row">
        <span class="row-title">Installing Ravyn</span>
        <span class="row-state">Working…</span>
      </div>
    </section>
  {/if}

  <!-- Component provisioning -->
  {#if componentRows.length > 0}
    <section class="group" aria-label="Component installation">
      {#each componentRows as row (row.component)}
        {@const ui = COMPONENT_STATE_UI[row.progress.state]}
        <div class="component">
          <div class="row">
            <span class="row-title">
              Installing {COMPONENT_LABEL[row.component]}
            </span>
            <span
              class="row-state"
              class:error={row.progress.state === "failed"}
              class:ok={row.progress.state === "installed"}
              aria-live="polite"
            >
              {ui.label}
              {#if row.progress.bytesDownloaded !== null && row.progress.bytesTotal !== null}
                · {formatBytes(row.progress.bytesDownloaded)} of
                {formatBytes(row.progress.bytesTotal)}
              {/if}
            </span>
          </div>
          {#if busyStates.includes(row.progress.state)}
            <ProgressBar
              value={row.progress.progressPct}
              label="Installing {COMPONENT_LABEL[row.component]}"
            />
          {/if}
          {#if row.progress.state === "failed"}
            <div class="row-actions">
              <Button onclick={() => void controller.retryComponent(row.component)}>
                Retry
              </Button>
            </div>
            {#if row.progress.message}
              <p class="row-message">{row.progress.message}</p>
            {/if}
          {/if}
          {#if busyStates.includes(row.progress.state)}
            <div class="row-actions">
              <Button
                variant="subtle"
                onclick={() => void controller.cancelComponent(row.component)}
              >
                Cancel
              </Button>
            </div>
          {/if}
        </div>
      {/each}
    </section>
  {:else}
    <p class="none">No optional components were selected.</p>
  {/if}
</StageShell>

<script lang="ts" module>
  const STEP_LABELS: Record<string, string> = {
    install_application: "Installing Ravyn",
    register_installed_app: "Registering in Installed Apps",
    start_menu_shortcut: "Creating Start Menu shortcut",
    desktop_shortcut: "Creating desktop shortcut",
    launch_at_startup: "Registering startup with Windows",
  };

  function stepLabel(step: string): string {
    return STEP_LABELS[step] ?? step;
  }
</script>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-layer);
    background: var(--bg-layer);
  }
  .component {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-4);
  }
  .row-title {
    font-weight: 600;
  }
  .row-state {
    font-size: var(--text-caption);
    color: var(--text-secondary);
    text-align: right;
  }
  .row-state.ok {
    color: var(--status-success);
  }
  .row-state.error {
    color: var(--status-error);
  }
  .row-actions {
    display: flex;
    gap: var(--space-2);
  }
  .row-message {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--status-error);
    word-break: break-word;
  }
  .none {
    color: var(--text-secondary);
  }
</style>
