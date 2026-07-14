<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import DetailsPane from "../components/DetailsPane.svelte";
  import Dialog from "../components/Dialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import ListDetailsLayout from "../components/ListDetailsLayout.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import { formatAbsoluteTime } from "../util/format";
  import { executionCanCancel, executionSeverity, type AutomationController } from "./automationController.svelte";

  let { controller }: { controller: AutomationController } = $props();

  function summaryRecord(value: unknown): Record<string, unknown> | null {
    return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
  }

  function displayValue(value: unknown): string {
    if (value === null || value === undefined || value === "") return "—";
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return String(value);
    return JSON.stringify(value);
  }
</script>

<Dialog open={!!controller.historySchedule} title="Schedule execution history" size="large" onClose={() => controller.closeHistory()}>
  {#if controller.historySchedule}
    <div class="history-heading">
      <span class="heading-icon"><Icon name="calendar" size={18} /></span>
      <div><strong>{controller.historySchedule.source}</strong><span>{controller.historySchedule.destination}</span></div>
      <Button disabled={controller.historyLoading} onclick={() => void controller.openHistory(controller.historySchedule!)}><Icon name="refresh" size={15} /> Refresh</Button>
    </div>
  {/if}

  <div class="history-layout">
    <ListDetailsLayout detailsOpen={!!controller.selectedExecution} detailsWidth="390px" detailsLabel="Execution details">
      {#snippet list()}
        {#if controller.historyError}
          <InlineError title="Couldn't load execution history" message={controller.historyError} retry={() => controller.historySchedule && void controller.openHistory(controller.historySchedule)} />
        {:else if controller.historyLoading}
          <p class="state">Loading execution history…</p>
        {:else if controller.executions.length === 0}
          <EmptyState icon="clock" title="No executions yet" message="Run this schedule now or wait for its next planned time." />
        {:else}
          <div class="execution-list" role="list">
            {#each controller.executions as execution (execution.id)}
              <button
                type="button"
                class="execution-row"
                class:selected={controller.selectedExecution?.id === execution.id}
                onclick={() => (controller.selectedExecution = execution)}
              >
                <span class="execution-dot" class:active={executionCanCancel(execution.state)}></span>
                <span class="execution-copy">
                  <strong>{formatAbsoluteTime(execution.intended_run_at)}</strong>
                  <small>Started {formatAbsoluteTime(execution.started_at)}{execution.completed_at ? ` · completed ${formatAbsoluteTime(execution.completed_at)}` : ""}</small>
                  {#if execution.error}<small class="error">{execution.error}</small>{/if}
                </span>
                <StatusBadge label={execution.state} severity={executionSeverity(execution.state)} />
              </button>
            {/each}
          </div>
        {/if}
      {/snippet}
      {#snippet details()}
        {#if controller.selectedExecution}
          {@const summary = summaryRecord(controller.selectedExecution.summary)}
          <DetailsPane
            title={formatAbsoluteTime(controller.selectedExecution.intended_run_at)}
            subtitle={controller.selectedExecution.state}
            icon="clock"
            onClose={() => (controller.selectedExecution = null)}
          >
            {#snippet actions()}
              <Button variant="subtle" onclick={() => void controller.refreshSelectedExecution()}><Icon name="refresh" size={14} /> Refresh</Button>
            {/snippet}
            <dl class="details">
              <dt>Planned</dt><dd>{formatAbsoluteTime(controller.selectedExecution.intended_run_at)}</dd>
              <dt>Started</dt><dd>{formatAbsoluteTime(controller.selectedExecution.started_at)}</dd>
              <dt>Completed</dt><dd>{controller.selectedExecution.completed_at ? formatAbsoluteTime(controller.selectedExecution.completed_at) : "Still running"}</dd>
              <dt>State</dt><dd>{controller.selectedExecution.state}</dd>
              {#if summary}
                {#each Object.entries(summary).filter(([key]) => !["raw", "debug"].includes(key)) as [key, value] (key)}
                  <dt>{key.replaceAll("_", " ")}</dt><dd>{displayValue(value)}</dd>
                {/each}
              {/if}
            </dl>
            {#if controller.selectedExecution.error}<div class="execution-error"><Icon name="warning" size={16} /><span>{controller.selectedExecution.error}</span></div>{/if}
            {#if executionCanCancel(controller.selectedExecution.state)}
              <Button disabled={!!controller.executionBusy} onclick={() => void controller.cancelExecution(controller.selectedExecution!)}>{controller.executionBusy ? "Cancelling…" : "Cancel execution"}</Button>
            {/if}
            {#if controller.selectedExecution.summary}
              <AdvancedDisclosure title="Raw execution summary" description="Technical data returned by the scheduler.">
                <pre>{JSON.stringify(controller.selectedExecution.summary, null, 2)}</pre>
              </AdvancedDisclosure>
            {/if}
          </DetailsPane>
        {/if}
      {/snippet}
    </ListDetailsLayout>
  </div>

  {#snippet footer()}<Button variant="accent" onclick={() => controller.closeHistory()}>Done</Button>{/snippet}
</Dialog>

<style>
  .history-heading { display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); margin-bottom: var(--space-3); }
  .heading-icon { width: 32px; height: 32px; display: grid; place-items: center; color: var(--text-secondary); }
  .history-heading > div { min-width: 0; display: flex; flex-direction: column; }
  .history-heading strong, .history-heading span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .history-heading span { color: var(--text-tertiary); font-size: var(--text-caption); }
  .history-layout { height: min(560px, 62vh); min-height: 360px; border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); overflow: hidden; }
  .state { padding: var(--space-5); color: var(--text-secondary); }
  .execution-list { height: 100%; overflow: auto; }
  .execution-row { width: 100%; min-height: 68px; display: grid; grid-template-columns: 12px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); color: var(--text-primary); background: transparent; text-align: left; font: inherit; }
  .execution-row:hover, .execution-row.selected { background: var(--bg-subtle-hover); }
  .execution-row.selected { box-shadow: inset 3px 0 var(--accent-default); }
  .execution-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--stroke-control-strong); }
  .execution-dot.active { background: var(--accent-default); }
  .execution-copy { min-width: 0; display: flex; flex-direction: column; }
  .execution-copy strong, .execution-copy small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .execution-copy small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .execution-copy small.error { color: var(--status-error); }
  .details { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-3); margin: 0 0 var(--space-4); }
  .details dt { color: var(--text-secondary); text-transform: capitalize; }
  .details dd { margin: 0; overflow-wrap: anywhere; }
  .execution-error { display: flex; gap: var(--space-2); margin-bottom: var(--space-4); padding: var(--space-3); border: 1px solid color-mix(in srgb, var(--status-error) 30%, transparent); border-radius: var(--radius-control); color: var(--status-error); background: color-mix(in srgb, var(--status-error) 7%, transparent); }
  pre { max-height: 240px; margin: 0; overflow: auto; font: 12px/1.5 "Cascadia Mono", Consolas, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
