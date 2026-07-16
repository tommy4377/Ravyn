<script lang="ts">
  import { untrack } from "svelte";
  import { describeError } from "../api/errors";
  import type { ReadinessStatus } from "../api/types";
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import Surface from "../components/Surface.svelte";
  import DiagnosticsView from "../diagnostics/DiagnosticsView.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
  let readiness = $state<ReadinessStatus | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let copying = $state(false);

  async function runDiagnostics(): Promise<void> {
    if (!connection.client || loading) return;
    loading = true;
    error = null;
    try {
      readiness = await connection.client.getReadiness();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  async function copyReport(): Promise<void> {
    if (!connection.client || copying) return;
    copying = true;
    try {
      const [readinessResult, databaseResult, dependenciesResult, capabilitiesResult, auditResult] = await Promise.allSettled([
        connection.client.getReadiness(),
        connection.client.getDatabaseStatus(),
        connection.client.getDependencies(),
        connection.client.getSystemCapabilities(),
        connection.client.verifyAuditChain(),
      ]);
      const value = (result: PromiseSettledResult<unknown>) => result.status === "fulfilled" ? result.value : { error: describeError(result.reason) };
      const report = {
        generated_at: new Date().toISOString(),
        backend: controller.backend,
        installation: controller.installation,
        readiness: value(readinessResult),
        database: value(databaseResult),
        dependencies: value(dependenciesResult),
        capabilities: value(capabilitiesResult),
        audit_chain: value(auditResult),
      };
      await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
      notifications.success("Diagnostic report copied");
    } catch (cause) {
      notifications.error("Couldn't copy the diagnostic report", describeError(cause));
    } finally {
      copying = false;
    }
  }

  $effect(() => {
    if (connection.client) untrack(() => void runDiagnostics());
  });
</script>

<SettingsCategoryHeader title="Troubleshooting" description="Check overall health first, then open technical diagnostics only when they are needed." />
<div class="stack">
  {#if error}<InlineError title="Health check failed" message={error} retry={runDiagnostics} />{/if}
  <Surface padding="none">
    <div class="health-row">
      <span class:good={readiness?.ready} class:bad={readiness && !readiness.ready} class="health-icon"><Icon name={readiness?.ready ? "check-circle" : "warning"} size={22} /></span>
      <div>
        <strong>{loading ? "Checking Ravyn…" : readiness?.ready ? "Everything is working" : "Ravyn needs attention"}</strong>
        <span>{readiness?.ready ? "The backend can write to the database and download folder and is accepting tasks." : "Run diagnostics to identify the unavailable service or path."}</span>
      </div>
      <div class="actions">
        <Button disabled={loading} onclick={() => void runDiagnostics()}><Icon name="refresh" size={15} /> {loading ? "Checking…" : "Run diagnostics"}</Button>
        <Button variant="subtle" disabled={copying} onclick={() => void copyReport()}><Icon name="copy" size={15} /> {copying ? "Copying…" : "Copy report"}</Button>
        <Button variant="subtle" disabled={controller.repairBusy} onclick={() => void controller.repairInstalledApplication()}><Icon name="wrench" size={15} /> Repair installation</Button>
      </div>
    </div>
  </Surface>

  <Surface padding="none">
    <AdvancedDisclosure title="Advanced diagnostics" description="Database recovery, dependencies, audit integrity, capabilities, and host reliability.">
      <div class="advanced-diagnostics"><DiagnosticsView embedded /></div>
    </AdvancedDisclosure>
  </Surface>
</div>

<style>
  .stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .health-row { min-height: 104px; display: grid; grid-template-columns: 42px minmax(0, 1fr) auto; align-items: center; gap: var(--space-4); padding: var(--space-5); }
  .health-icon { width: 42px; height: 42px; display: grid; place-items: center; color: var(--status-warning); }
  .health-icon.good { color: var(--status-success); }
  .health-icon.bad { color: var(--status-error); }
  .health-row > div:nth-child(2) { display: flex; flex-direction: column; gap: var(--space-1); }
  .health-row span { color: var(--text-secondary); font-size: var(--text-caption); }
  .actions { display: flex; gap: var(--space-2); flex-wrap: wrap; justify-content: flex-end; }
  .advanced-diagnostics { padding: var(--space-2) var(--space-4) 0 0; }
  .advanced-diagnostics :global(.page-header) { display: none; }
  .advanced-diagnostics :global(.diagnostics) { height: auto; overflow: visible; }
  .advanced-diagnostics :global(.content) { padding: 0; overflow: visible; }
  @media (max-width: 880px) { .health-row { grid-template-columns: 42px minmax(0, 1fr); } .actions { grid-column: 1 / -1; justify-content: flex-start; } }
</style>
