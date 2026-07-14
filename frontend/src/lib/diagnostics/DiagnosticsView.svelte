<script lang="ts">
  import { describeError } from "../api/errors";
  import type {
    AuditChainStatus,
    AuditRecord,
    BackupRecord,
    DatabaseStatus,
    HostProfile,
    DependenciesStatus,
    ReadinessStatus,
    RestoreStatus,
    SystemCapabilities,
  } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import MetricCard from "../components/MetricCard.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import Surface from "../components/Surface.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes } from "../util/format";

  let readiness = $state<ReadinessStatus | null>(null);
  let database = $state<DatabaseStatus | null>(null);
  let dependencies = $state<DependenciesStatus | null>(null);
  let capabilities = $state<SystemCapabilities | null>(null);
  let audit = $state<AuditRecord[]>([]);
  let auditChain = $state<AuditChainStatus | null>(null);
  let backups = $state<BackupRecord[]>([]);
  let restoreStatus = $state<RestoreStatus | null>(null);
  let hosts = $state<HostProfile[]>([]);
  let backupWorking = $state<string | null>(null);
  let restoreBackup = $state<BackupRecord | null>(null);
  let restoreBusy = $state(false);
  let restoreError = $state<string | null>(null);
  let resetHostsOpen = $state(false);
  let resetHostsBusy = $state(false);
  let resetHostsError = $state<string | null>(null);
  let loading = $state(true);
  let working = $state(false);
  let error = $state<string | null>(null);

  const client = $derived(connection.client);

  $effect(() => {
    if (!client) return;
    void loadAll();
  });

  async function loadAll(): Promise<void> {
    if (!client) return;
    loading = true;
    error = null;

    const results = await Promise.allSettled([
      client.getReadiness(),
      client.getDatabaseStatus(),
      client.getDependencies(),
      client.getSystemCapabilities(),
      client.listAudit({ limit: 30 }),
      client.verifyAuditChain(),
      client.listDatabaseBackups({ limit: 20 }),
      client.getDatabaseRestoreStatus(),
      client.listHostProfiles({ limit: 50 }),
    ]);

    if (results[0].status === "fulfilled") readiness = results[0].value;
    if (results[1].status === "fulfilled") database = results[1].value;
    if (results[2].status === "fulfilled") dependencies = results[2].value;
    if (results[3].status === "fulfilled") capabilities = results[3].value;
    if (results[4].status === "fulfilled") audit = results[4].value.items;
    if (results[5].status === "fulfilled") auditChain = results[5].value;
    if (results[6].status === "fulfilled") backups = results[6].value.items;
    if (results[7].status === "fulfilled") restoreStatus = results[7].value;
    if (results[8].status === "fulfilled") hosts = results[8].value.items;

    const failures = results.filter((result) => result.status === "rejected");
    if (failures.length === results.length) {
      error = describeError((failures[0] as PromiseRejectedResult).reason);
    } else if (failures.length > 0) {
      notifications.warning(`${failures.length} diagnostic source(s) could not be loaded.`);
    }
    loading = false;
  }

  async function createBackup(): Promise<void> {
    if (!client || working) return;
    working = true;
    try {
      const result = await client.createDatabaseBackup();
      notifications.info("Database backup created", result.path);
      const page = await client.listDatabaseBackups({ limit: 20 });
      backups = page.items;
    } catch (cause) {
      notifications.error("Couldn't create the backup", describeError(cause));
    } finally {
      working = false;
    }
  }

  async function runMaintenance(): Promise<void> {
    if (!client || working) return;
    working = true;
    try {
      await client.runMaintenance(30);
      notifications.info("Maintenance completed");
      await loadAll();
    } catch (cause) {
      notifications.error("Maintenance failed", describeError(cause));
    } finally {
      working = false;
    }
  }


  async function verifyBackup(backup: BackupRecord): Promise<void> {
    if (!client || backupWorking) return;
    backupWorking = backup.name;
    try {
      const result = await client.verifyDatabaseBackup(backup.name);
      if (result.integrity === "ok") notifications.success("Backup verified", `${backup.name} passed its integrity check.`);
      else notifications.warning("Backup integrity warning", `${backup.name}: ${result.integrity}`);
    } catch (cause) {
      notifications.error("Couldn't verify the backup", describeError(cause));
    } finally {
      backupWorking = null;
    }
  }

  async function confirmRestore(): Promise<void> {
    if (!client || !restoreBackup || restoreBusy) return;
    restoreBusy = true;
    restoreError = null;
    try {
      restoreStatus = await client.scheduleDatabaseRestore(restoreBackup.name);
      notifications.warning("Database restore staged", "Restart Ravyn to apply the selected backup.");
      restoreBackup = null;
    } catch (cause) {
      restoreError = describeError(cause);
    } finally {
      restoreBusy = false;
    }
  }

  async function cancelRestore(): Promise<void> {
    if (!client || restoreBusy) return;
    restoreBusy = true;
    try {
      restoreStatus = await client.cancelDatabaseRestore();
      notifications.info("Pending database restore cancelled");
    } catch (cause) {
      notifications.error("Couldn't cancel the restore", describeError(cause));
    } finally {
      restoreBusy = false;
    }
  }

  async function resetHosts(): Promise<void> {
    if (!client || resetHostsBusy) return;
    resetHostsBusy = true;
    resetHostsError = null;
    try {
      const result = await client.resetHostProfiles();
      hosts = [];
      resetHostsOpen = false;
      notifications.info("Host history reset", `${result.deleted} profile(s) removed.`);
    } catch (cause) {
      resetHostsError = describeError(cause);
    } finally {
      resetHostsBusy = false;
    }
  }

  function readinessLabel(): string {
    if (!readiness) return loading ? "Checking…" : "Unknown";
    return readiness.ready ? "Ready" : "Needs attention";
  }

  function dependencyCount(): string {
    if (!dependencies) return "—";
    const available = dependencies.media.filter((item) => item.available !== false).length;
    return `${available}/${dependencies.media.length}`;
  }

  function backupName(backup: BackupRecord): string {
    return backup.name || "Database backup";
  }
</script>

<div class="diagnostics page-scroll">
  <PageHeader
    eyebrow="System"
    title="Diagnostics"
    description="Inspect backend readiness, dependencies, database health, audit integrity and maintenance data."
  >
    {#snippet actions()}
      <Button variant="standard" disabled={loading || working} onclick={() => void loadAll()}>
        <Icon name="refresh" size={15} /> Refresh
      </Button>
      <Button variant="accent" disabled={working} onclick={() => void createBackup()}>
        <Icon name="database" size={15} /> Create backup
      </Button>
    {/snippet}
  </PageHeader>

  <div class="content">
    {#if error}
      <InlineError title="Diagnostics are unavailable" message={error} retry={loadAll} />
    {/if}

    {#if restoreStatus?.pending}
      <div class="restore-banner">
        <span class="restore-icon"><Icon name="restore" size={19} /></span>
        <div><strong>Database restore pending</strong><span>{restoreStatus.pending.backup_name} is staged and will be applied after Ravyn restarts.</span></div>
        <Button disabled={restoreBusy} onclick={() => void cancelRestore()}>Cancel restore</Button>
      </div>
    {:else if restoreStatus?.last_result}
      <div class="restore-result"><Icon name={restoreStatus.last_result.outcome === "success" ? "check-circle" : "warning"} size={17} /><span>Last restore: {restoreStatus.last_result.message}</span></div>
    {/if}

    <div class="metrics">
      <MetricCard label="Backend" value={readinessLabel()} detail={readiness?.accepting_tasks ? "Accepting tasks" : "Task intake unavailable"} icon="bolt" />
      <MetricCard label="Database" value={database?.integrity ?? (loading ? "Checking…" : "Unknown")} detail="Integrity result" icon="database" />
      <MetricCard label="Audit chain" value={auditChain ? (auditChain.valid ? "Valid" : "Invalid") : "—"} detail={auditChain ? `${auditChain.chained_entries} records chained` : "Cryptographic history"} icon="shield" />
      <MetricCard label="Media tools" value={dependencyCount()} detail="Available dependencies" icon="components" />
    </div>

    <div class="grid">
      <Surface class="health-card">
        <div class="section-title">
          <div>
            <h2>Runtime health</h2>
            <p>Current state of the embedded service.</p>
          </div>
          <span class:good={readiness?.ready} class:bad={readiness && !readiness.ready} class="health-pill">
            {readinessLabel()}
          </span>
        </div>
        <dl class="details">
          <dt>Database access</dt><dd>{readiness?.database_writable === undefined ? "—" : readiness.database_writable ? "Writable" : "Read only"}</dd>
          <dt>Download root</dt><dd>{readiness?.download_root_writable === undefined ? "—" : readiness.download_root_writable ? "Writable" : "Unavailable"}</dd>
          <dt>Progress writer</dt><dd>{readiness?.progress_writer_running === undefined ? "—" : readiness.progress_writer_running ? "Running" : "Stopped"}</dd>
          <dt>Task intake</dt><dd>{readiness?.accepting_tasks === undefined ? "—" : readiness.accepting_tasks ? "Enabled" : "Disabled"}</dd>
          <dt>Backend version</dt><dd>{capabilities?.backend_version ?? "—"}</dd>
          <dt>API version</dt><dd>{capabilities?.api_version ?? "—"}</dd>
          <dt>Platform</dt><dd>{capabilities?.platform ?? "—"}</dd>
        </dl>
      </Surface>

      <Surface class="maintenance-card">
        <div class="section-title">
          <div>
            <h2>Maintenance</h2>
            <p>Clean expired operational data while preserving downloads and library files.</p>
          </div>
        </div>
        <div class="maintenance-copy">
          <Icon name="wrench" size={26} />
          <p>Run the configured maintenance routines with a 30-day retention window for eligible records.</p>
        </div>
        <Button variant="standard" disabled={working} onclick={() => void runMaintenance()}>
          {working ? "Working…" : "Run maintenance"}
        </Button>
      </Surface>
    </div>

    <Surface class="dependency-card" padding="none">
      <div class="surface-heading">
        <div><h2>Dependencies and capabilities</h2><p>Tools detected by the backend and features exposed by this build.</p></div>
      </div>
      <div class="dependency-grid">
        {#if dependencies?.media?.length}
          {#each dependencies.media as dependency, index (`${dependency.name ?? "dependency"}-${index}`)}
            <div class="dependency-row">
              <div class="dependency-icon"><Icon name={dependency.available === false ? "warning" : "check-circle"} size={16} /></div>
              <div class="dependency-copy">
                <strong>{dependency.name ?? `Media dependency ${index + 1}`}</strong>
                <span>{dependency.version ?? dependency.path ?? dependency.error ?? "No version information"}</span>
              </div>
              <span class:available={dependency.available !== false} class="state-label">{dependency.available === false ? "Unavailable" : "Available"}</span>
            </div>
          {/each}
        {:else}
          <p class="empty-copy">No dependency information was returned.</p>
        {/if}
      </div>
      {#if capabilities}
        <div class="chip-list" aria-label="Available features">
          {#each capabilities.available_features as feature (feature)}<span>{feature}</span>{/each}
        </div>
      {/if}
    </Surface>

    <div class="grid lower-grid">
      <Surface class="backups-card" padding="none">
        <div class="surface-heading">
          <div><h2>Database backups</h2><p>Recent recovery points created by Ravyn.</p></div>
        </div>
        <div class="rows">
          {#if backups.length === 0}
            <p class="empty-copy">No backups found.</p>
          {:else}
            {#each backups as backup, index (`${backup.name}-${index}`)}
              <div class="data-row backup-row">
                <div><strong>{backupName(backup)}</strong><span>{backup.modified_at ? formatAbsoluteTime(backup.modified_at) : "Modification time unavailable"}</span></div>
                <span>{formatBytes(backup.size_bytes ?? null)}</span>
                <div class="row-actions">
                  <IconButton icon="verify" label="Verify backup" variant="subtle" disabled={!!backupWorking} onclick={() => void verifyBackup(backup)} />
                  <IconButton icon="restore" label="Restore backup" variant="subtle" disabled={!!restoreStatus?.pending || !!backupWorking} onclick={() => { restoreError = null; restoreBackup = backup; }} />
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </Surface>

      <Surface class="audit-card" padding="none">
        <div class="surface-heading">
          <div><h2>Recent audit activity</h2><p>Latest security and operational records.</p></div>
        </div>
        <div class="rows">
          {#if audit.length === 0}
            <p class="empty-copy">No audit records found.</p>
          {:else}
            {#each audit.slice(0, 12) as record (record.id)}
              <div class="data-row audit-row">
                <div><strong>{record.action}</strong><span>{record.resource_type}{record.resource_id ? ` · ${record.resource_id}` : ""}</span></div>
                <div class="audit-meta"><span class="outcome">{record.outcome}</span><span>{record.timestamp || record.created_at ? formatAbsoluteTime((record.timestamp ?? record.created_at)!) : ""}</span></div>
              </div>
            {/each}
          {/if}
        </div>
      </Surface>
    </div>

    <Surface class="hosts-card" padding="none">
      <div class="surface-heading">
        <div><h2>Host reliability</h2><p>Connection history used to avoid repeatedly failing servers.</p></div>
        {#if hosts.length}<Button variant="subtle" onclick={() => (resetHostsOpen = true)}>Reset history</Button>{/if}
      </div>
      <div class="host-table">
        {#if hosts.length === 0}
          <p class="empty-copy">No host reliability records have been collected yet.</p>
        {:else}
          <div class="host-header"><span>Host</span><span>Success</span><span>Failures</span><span>Average speed</span><span>Circuit</span></div>
          {#each hosts as host (host.host)}
            <div class="host-row">
              <div><strong>{host.host}</strong>{#if host.last_error}<small title={host.last_error}>{host.last_error}</small>{/if}</div>
              <span>{host.successful_downloads}</span><span>{host.failed_downloads}</span><span>{formatBytes(host.average_throughput_bps)}/s</span>
              <span class:host-warning={!!host.circuit_open_until}>{host.circuit_open_until ? `Open until ${formatAbsoluteTime(host.circuit_open_until)}` : "Healthy"}</span>
            </div>
          {/each}
        {/if}
      </div>
    </Surface>
  </div>
</div>

<ConfirmDialog open={!!restoreBackup} title="Restore this database backup?" message="The restore will be staged now and applied after Ravyn restarts. Current database state will be replaced, while downloaded files remain untouched." confirmLabel="Stage restore" destructive busy={restoreBusy} error={restoreError} onConfirm={() => void confirmRestore()} onClose={() => !restoreBusy && (restoreBackup = null)} />
<ConfirmDialog open={resetHostsOpen} title="Reset host reliability history?" message="Ravyn will forget collected success, failure, speed, and circuit-breaker history for all hosts." confirmLabel="Reset history" destructive busy={resetHostsBusy} error={resetHostsError} onConfirm={() => void resetHosts()} onClose={() => !resetHostsBusy && (resetHostsOpen = false)} />

<style>
  .diagnostics { height: 100%; overflow-y: auto; }
  .content { display: flex; flex-direction: column; gap: var(--space-4); padding: 0 var(--page-padding) var(--space-8); }
  .restore-banner, .restore-result { display: flex; align-items: center; gap: var(--space-3); padding: var(--space-3) var(--space-4); border: 1px solid var(--status-warning); border-radius: var(--radius-layer); background: var(--status-warning-bg); }
  .restore-banner > div { display: flex; flex: 1; min-width: 0; flex-direction: column; }
  .restore-banner span, .restore-result { color: var(--text-secondary); }
  .restore-icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: var(--radius-medium); color: var(--status-warning); background: color-mix(in srgb, var(--status-warning) 12%, transparent); }
  .metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-3); }
  .grid { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(280px, .75fr); gap: var(--space-4); }
  :global(.health-card), :global(.maintenance-card) { min-height: 240px; }
  :global(.maintenance-card) { display: flex; flex-direction: column; align-items: flex-start; }
  .section-title, .surface-heading { display: flex; justify-content: space-between; gap: var(--space-4); align-items: flex-start; }
  .section-title h2, .surface-heading h2 { margin: 0; font-size: var(--text-subtitle); font-weight: 600; }
  .section-title p, .surface-heading p { margin: var(--space-1) 0 0; color: var(--text-secondary); }
  .surface-heading { padding: var(--space-4) var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .health-pill { padding: 4px 9px; border-radius: var(--radius-pill); background: var(--bg-subtle); color: var(--text-secondary); font-size: var(--text-caption); font-weight: 600; }
  .health-pill.good { color: var(--status-success); background: var(--status-success-bg); }
  .health-pill.bad { color: var(--status-error); background: var(--status-error-bg); }
  .details { display: grid; grid-template-columns: minmax(130px, max-content) 1fr; gap: var(--space-2) var(--space-5); margin: var(--space-5) 0 0; }
  .details dt { color: var(--text-secondary); }
  .details dd { margin: 0; font-weight: 500; }
  .maintenance-copy { display: flex; gap: var(--space-3); margin: auto 0 var(--space-5); color: var(--text-secondary); }
  .maintenance-copy :global(svg) { color: var(--accent-text); flex: none; }
  .maintenance-copy p { margin: 0; }
  .dependency-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .dependency-row { display: grid; grid-template-columns: 32px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-3) var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .dependency-row:nth-child(odd) { border-right: 1px solid var(--stroke-divider); }
  .dependency-icon { display: grid; place-items: center; width: 30px; height: 30px; border-radius: var(--radius-medium); background: var(--bg-subtle); color: var(--text-secondary); }
  .dependency-copy { display: flex; flex-direction: column; min-width: 0; }
  .dependency-copy strong, .dependency-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dependency-copy span, .state-label { color: var(--text-secondary); font-size: var(--text-caption); }
  .state-label.available { color: var(--status-success); }
  .chip-list { display: flex; flex-wrap: wrap; gap: var(--space-2); padding: var(--space-4) var(--space-5); }
  .chip-list span { padding: 4px 9px; border-radius: var(--radius-pill); background: var(--bg-subtle); color: var(--text-secondary); font-size: var(--text-caption); }
  .rows { display: flex; flex-direction: column; }
  .data-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); min-height: 58px; padding: var(--space-2) var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .data-row:last-child { border-bottom: none; }
  .data-row > div { min-width: 0; display: flex; flex-direction: column; }
  .data-row strong, .data-row span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .data-row span { color: var(--text-secondary); font-size: var(--text-caption); }
  .audit-meta { align-items: flex-end; flex: none; }
  .backup-row { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; }
  .row-actions { display: flex !important; flex-direction: row !important; align-items: center; }
  .host-table { overflow-x: auto; }
  .host-header, .host-row { display: grid; grid-template-columns: minmax(220px, 1.5fr) 80px 80px 110px minmax(150px, .8fr); align-items: center; gap: var(--space-3); min-width: 720px; padding: 0 var(--space-5); }
  .host-header { min-height: 38px; color: var(--text-tertiary); border-bottom: 1px solid var(--stroke-divider); font-size: var(--text-caption); font-weight: 600; }
  .host-row { min-height: 58px; border-bottom: 1px solid var(--stroke-divider); }
  .host-row > div { display: flex; min-width: 0; flex-direction: column; }
  .host-row strong, .host-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .host-row small { color: var(--status-error); }
  .host-row > span { color: var(--text-secondary); }
  .host-row .host-warning { color: var(--status-warning); }
  .outcome { text-transform: capitalize; color: var(--accent-text) !important; }
  .empty-copy { padding: var(--space-5); margin: 0; color: var(--text-secondary); }
  @media (max-width: 1100px) { .metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 820px) { .grid, .dependency-grid { grid-template-columns: 1fr; } .dependency-row:nth-child(odd) { border-right: none; } }
  @media (max-width: 580px) { .metrics { grid-template-columns: 1fr; } }
</style>
