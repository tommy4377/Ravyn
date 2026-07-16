<script lang="ts">
  import { describeError } from "../api/errors";
  import type {
    ComponentId,
    ComponentManifestStatus,
    ComponentOverview,
    ComponentStatus,
    FeatureId,
  } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import { connection } from "../stores/connection.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes } from "../util/format";

  let { embedded = false }: { embedded?: boolean } = $props();

  let overview = $state<ComponentOverview | null>(null);
  let manifestStatus = $state<ComponentManifestStatus | null>(null);
  let loading = $state(true);
  let manifestRefreshing = $state(false);
  let error = $state<string | null>(null);
  let busy = $state<Partial<Record<ComponentId, string>>>({});
  let restartRequired = $state(false);
  let removeTarget = $state<ComponentStatus | null>(null);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);

  const names: Record<ComponentId, string> = {
    ytdlp: "yt-dlp",
    ffmpeg: "FFmpeg",
    rqbit: "rqbit",
    seven_zip: "7-Zip",
  };
  const descriptions: Record<ComponentId, string> = {
    ytdlp: "Extracts media information and downloads from supported sites.",
    ffmpeg: "Merges, converts, probes, and post-processes audio and video.",
    rqbit: "Provides the managed BitTorrent engine and seeding controls.",
    seven_zip: "Installs or uses 7-Zip for verified archive listing, testing, and extraction.",
  };
  const icons: Record<ComponentId, IconName> = { ytdlp: "video", ffmpeg: "components", rqbit: "torrent", seven_zip: "archive" };
  const featureNames: Record<FeatureId, string> = {
    standard_downloads: "Standard downloads",
    video_extraction: "Video extraction",
    media_merging: "Media merging",
    torrent_support: "Torrent support",
    archive_extraction: "Archive extraction",
  };

  function statusSeverity(component: ComponentStatus): "neutral" | "info" | "success" | "warning" | "error" {
    if (component.state === "installed" || component.state === "custom_path") return "success";
    if (component.state === "update_available") return "warning";
    if (component.component === "seven_zip" && component.state === "unsupported") return "warning";
    if (component.state === "failed" || component.state === "unsupported" || component.state === "custom_path_invalid") return "error";
    if (component.state === "cancelled") return "warning";
    if (["queued", "downloading", "verifying", "installing"].includes(component.state)) return "info";
    return "neutral";
  }

  function statusLabel(state: ComponentStatus["state"]): string {
    return state.replaceAll("_", " ").replace(/^./, (char) => char.toUpperCase());
  }

  function componentDescription(component: ComponentStatus): string {
    return descriptions[component.component];
  }

  function openComponentSettings(): void {
    navigation.section = "settings";
  }

  function manifestSeverity(status: ComponentManifestStatus): "neutral" | "info" | "success" | "warning" | "error" {
    if (status.phase === "current") return "success";
    if (status.phase === "checking" || status.phase === "idle") return "info";
    if (status.phase === "stale") return "warning";
    if (status.phase === "error") return "error";
    return "neutral";
  }

  function manifestLabel(status: ComponentManifestStatus): string {
    if (!status.configured) return "Built-in catalog";
    if (status.phase === "current") return "Catalog current";
    if (status.phase === "stale") return "Using cached catalog";
    if (status.phase === "checking") return "Checking catalog";
    if (status.phase === "error") return "Catalog unavailable";
    return "Catalog ready";
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      [overview, manifestStatus] = await Promise.all([
        connection.client.getComponents(),
        connection.client.getComponentManifestStatus(),
      ]);
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  $effect(() => { void load(); });

  async function refreshCatalog(): Promise<void> {
    if (!connection.client || !manifestStatus?.configured) return;
    manifestRefreshing = true;
    try {
      manifestStatus = await connection.client.refreshComponentManifest();
      overview = await connection.client.getComponents();
      notifications.success("Component catalog refreshed", manifestStatus.manifest_version ? `Manifest ${manifestStatus.manifest_version}` : undefined);
    } catch (cause) {
      notifications.error("Couldn't refresh the component catalog", describeError(cause));
      try {
        manifestStatus = await connection.client.getComponentManifestStatus();
      } catch {
        // Keep the previous status visible when the follow-up status request also fails.
      }
    } finally {
      manifestRefreshing = false;
    }
  }

  async function run(component: ComponentStatus, operation: "install" | "update" | "verify" | "rollback" | "cleanup"): Promise<void> {
    if (!connection.client) return;
    busy = { ...busy, [component.component]: operation };
    try {
      if (operation === "install") {
        await connection.client.installComponent(component.component);
        const completed = await waitForComponent(component.component);
        if (completed.state !== "installed" && completed.state !== "custom_path") throw new Error(completed.error_message ?? `${names[component.component]} installation did not complete.`);
        restartRequired = true;
        notifications.success(`${names[component.component]} installed`, "Restart Ravyn before using the new version.");
      } else if (operation === "update") {
        await connection.client.updateComponent(component.component);
        const completed = await waitForComponent(component.component);
        if (completed.state !== "installed" && completed.state !== "custom_path") throw new Error(completed.error_message ?? `${names[component.component]} update did not complete.`);
        restartRequired = true;
        notifications.success(`${names[component.component]} updated`, "Restart Ravyn before using the new version.");
      } else if (operation === "verify") {
        const health = await connection.client.verifyComponent(component.component);
        if (health.healthy) notifications.success(`${names[component.component]} verified`, health.version ?? undefined);
        else notifications.error(`${names[component.component]} verification failed`, health.message ?? undefined);
      } else if (operation === "rollback") {
        await connection.client.rollbackComponent(component.component);
        restartRequired = true;
        notifications.success(`${names[component.component]} rolled back`);
      } else {
        const report = await connection.client.cleanupComponent(component.component);
        notifications.info(`${names[component.component]} cleanup complete`, `${formatBytes(report.bytes_freed)} freed · ${report.removed_versions.length} old version(s) removed`);
      }
      await load();
    } catch (cause) {
      notifications.error(`Couldn't ${operation} ${names[component.component]}`, describeError(cause));
    } finally {
      const next = { ...busy };
      delete next[component.component];
      busy = next;
    }
  }

  async function waitForComponent(component: ComponentId): Promise<ComponentStatus> {
    if (!connection.client) throw new Error("Ravyn is not connected.");
    for (let attempt = 0; attempt < 300; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 1_000));
      const next = await connection.client.getComponents();
      overview = next;
      const status = next.components.find((item) => item.component === component);
      if (!status) throw new Error("The component disappeared while it was being updated.");
      if (!["queued", "downloading", "verifying", "installing"].includes(status.state)) return status;
    }
    throw new Error("The component installation timed out. Check its status and retry if needed.");
  }

  async function removeComponent(): Promise<void> {
    if (!connection.client || !removeTarget) return;
    removeBusy = true;
    removeError = null;
    try {
      await connection.client.removeComponent(removeTarget.component);
      notifications.info(`${names[removeTarget.component]} removed`);
      removeTarget = null;
      await load();
    } catch (cause) {
      removeError = describeError(cause);
    } finally {
      removeBusy = false;
    }
  }
</script>

<div class="page" class:embedded>
  {#if !embedded}
  <PageHeader title="Components" description="Optional tools are installed and maintained separately from the Ravyn application.">
    {#snippet actions()}
      {#if manifestStatus?.configured}
        <Button disabled={manifestRefreshing} onclick={() => void refreshCatalog()}>
          <Icon name={manifestRefreshing ? "spinner" : "cloud"} size={16} />
          {manifestRefreshing ? "Refreshing…" : "Refresh catalog"}
        </Button>
      {/if}
      <Button variant="subtle" onclick={() => void load()}><Icon name="refresh" size={16} /> Recheck system</Button>
    {/snippet}
  </PageHeader>
  {/if}

  <div class="content">
    {#if error}
      <InlineError title="Couldn't load components" message={error} retry={() => void load()} />
    {:else if loading}
      <Surface><p class="muted">Checking installed components…</p></Surface>
    {:else if !overview}
      <EmptyState icon="components" title="Component information unavailable" />
    {:else}
      {#if restartRequired}
        <Surface padding="small" class="restart-surface">
          <Icon name="warning" size={18} />
          <div><strong>Restart Ravyn to activate updated components</strong><p>Media and torrent engines are loaded when the backend starts.</p></div>
        </Surface>
      {/if}
      {#if manifestStatus}
        <Surface padding="small" class="catalog-surface">
          <div class="catalog-row">
            <span class="catalog-icon"><Icon name={manifestStatus.configured ? "cloud" : "components"} size={18} /></span>
            <div class="catalog-copy">
              <div class="catalog-title">
                <strong>Component catalog</strong>
                <StatusBadge label={manifestLabel(manifestStatus)} severity={manifestSeverity(manifestStatus)} spinning={manifestStatus.phase === "checking" || manifestRefreshing} />
              </div>
              <p>
                {manifestStatus.source === "remote-cache" ? "Signed remote release catalog" : "Catalog bundled with this Ravyn build"}
                · {manifestStatus.channel}
                {#if manifestStatus.manifest_version} · revision {manifestStatus.manifest_version}{/if}
                {#if manifestStatus.last_checked_at} · checked {formatAbsoluteTime(manifestStatus.last_checked_at)}{/if}
              </p>
              {#if manifestStatus.last_error}<small class="catalog-warning">{manifestStatus.last_error}</small>{/if}
            </div>
            {#if manifestStatus.expires_at}
              <div class="catalog-expiry"><span>{manifestStatus.stale ? "Expired" : "Valid until"}</span><strong>{formatAbsoluteTime(manifestStatus.expires_at)}</strong></div>
            {/if}
          </div>
        </Surface>
      {/if}

      <Surface padding="normal" class="feature-surface">
        <div class="section-heading"><div><h2>Enabled features</h2><p>Profile: {overview.setup_profile} · manifest: {overview.manifest_provider}</p></div><StatusBadge label={overview.platform} severity="neutral" /></div>
        <div class="features">
          {#each overview.features as feature (feature.feature)}
            <div class="feature"><span class:disabled={!feature.enabled}><Icon name={feature.satisfied ? "check-circle" : feature.enabled ? "warning" : "cancel"} size={17} /></span><div><strong>{featureNames[feature.feature]}</strong><small>{feature.enabled ? feature.satisfied ? "Ready" : `Needs ${feature.required_components.map((id) => names[id]).join(", ")}` : "Disabled"}</small></div></div>
          {/each}
        </div>
      </Surface>

      <div class="component-grid">
        {#each overview.components as component (component.component)}
          <Surface padding="normal" class="component-card">
            <header>
              <span class="component-icon"><Icon name={icons[component.component]} size={22} /></span>
              <div><h2>{names[component.component]}</h2><p>{componentDescription(component)}</p></div>
              <StatusBadge label={statusLabel(component.state)} severity={statusSeverity(component)} icon={component.state === "installed" || component.state === "custom_path" ? "check-circle" : component.state === "failed" ? "alert-circle" : undefined} spinning={["queued", "downloading", "verifying", "installing"].includes(component.state)} />
            </header>

            <dl>
              <dt>Installed version</dt><dd>{component.managed_version ?? component.detected_version ?? "—"}</dd>
              {#if component.available_version}<dt>Available version</dt><dd>{component.available_version}</dd>{/if}
              <dt>Effective path</dt><dd class="path">{component.effective_path ?? "Not configured"}</dd>
              <dt>Last checked</dt><dd>{component.last_checked_at ? formatAbsoluteTime(component.last_checked_at) : "Never"}</dd>
              <dt>Last verified</dt><dd>{component.verified_at ? formatAbsoluteTime(component.verified_at) : "Never"}</dd>
            </dl>

            {#if component.error_message}<div class="component-error"><Icon name="alert-circle" size={16} /><span>{component.error_message}</span></div>{/if}

            <div class="actions">
              {#if component.state === "not_installed" || component.state === "failed" || component.state === "cancelled"}
                <Button variant="accent" disabled={!!busy[component.component]} onclick={() => void run(component, "install")}><Icon name="download" size={16} /> {busy[component.component] === "install" ? "Installing…" : "Install"}</Button>
              {:else if component.state === "update_available"}
                <Button variant="accent" disabled={!!busy[component.component]} onclick={() => void run(component, "update")}><Icon name="download" size={16} /> Update</Button>
              {/if}
              {#if component.component === "seven_zip" && component.state === "unsupported"}
                <Button variant="accent" onclick={openComponentSettings}><Icon name="settings" size={16} /> Configure path</Button>
              {/if}
              {#if component.state === "installed" || component.state === "custom_path" || component.state === "update_available"}
                <Button disabled={!!busy[component.component]} onclick={() => void run(component, "verify")}><Icon name="verify" size={16} /> Verify</Button>
                {#if component.rollback_available}<Button disabled={!!busy[component.component]} onclick={() => void run(component, "rollback")}><Icon name="restore" size={16} /> Roll back</Button>{/if}{#if component.managed_path}<Button disabled={!!busy[component.component]} onclick={() => (removeTarget = component)}><Icon name="trash" size={16} /> Remove</Button>{/if}
              {/if}
              <Button variant="subtle" disabled={!!busy[component.component]} onclick={() => void run(component, "cleanup")}><Icon name="wrench" size={16} /> Clean old files</Button>
            </div>
          </Surface>
        {/each}
      </div>
    {/if}
  </div>
</div>

<ConfirmDialog open={!!removeTarget} title={`Remove ${removeTarget ? names[removeTarget.component] : "component"}?`} message="The managed component files will be removed. Features that require this tool will stop working until it is installed again or a custom path is configured." confirmLabel="Remove component" destructive busy={removeBusy} error={removeError} onConfirm={() => void removeComponent()} onClose={() => !removeBusy && (removeTarget = null)} />

<style>
  .page { height: 100%; display: flex; flex-direction: column; }
  .content { flex: 1; min-height: 0; overflow: auto; padding: 0 var(--page-padding) var(--page-padding); display: flex; flex-direction: column; gap: var(--space-4); }
  .embedded { height: auto; }
  .embedded .content { overflow: visible; padding: 0; }
  .embedded .component-grid { grid-template-columns: 1fr; }
  .embedded :global(.component-card) { border-radius: var(--radius-control); box-shadow: none; }
  .muted, .section-heading p { color: var(--text-secondary); }
  .catalog-row { display: grid; grid-template-columns: 36px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); }
  .catalog-icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: var(--radius-medium); color: var(--text-secondary); background: var(--bg-subtle); border: 1px solid var(--stroke-subtle); }
  .catalog-copy { min-width: 0; }
  .catalog-title { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
  .catalog-copy p { color: var(--text-secondary); font-size: var(--text-caption); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .catalog-warning { display: block; margin-top: var(--space-1); color: var(--status-warning); }
  :global(.restart-surface) { display: flex; align-items: flex-start; gap: var(--space-3); color: var(--status-warning); }
  :global(.restart-surface p) { color: var(--text-secondary); font-size: var(--text-caption); }
  .catalog-expiry { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; font-size: var(--text-caption); }
  .catalog-expiry span { color: var(--text-tertiary); }
  .catalog-expiry strong { font-weight: 500; }
  .section-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-4); }
  h2 { margin: 0; font-size: var(--text-body-strong); }
  p { margin: var(--space-1) 0 0; }
  .features { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: var(--space-3); margin-top: var(--space-4); }
  .feature { display: flex; gap: var(--space-2); min-width: 0; padding: var(--space-3); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .feature > span { color: var(--status-success); } .feature > span.disabled { color: var(--text-disabled); }
  .feature div { display: flex; flex-direction: column; min-width: 0; }
  .feature strong { font-size: var(--text-caption); }
  .feature small { color: var(--text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .component-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  :global(.component-card) { display: flex; flex-direction: column; }
  :global(.component-card) header { display: grid; grid-template-columns: 44px minmax(0, 1fr) auto; align-items: start; gap: var(--space-3); }
  :global(.component-card) header p { color: var(--text-secondary); font-size: var(--text-caption); }
  .component-icon { display: grid; place-items: center; width: 42px; height: 42px; border-radius: var(--radius-layer); color: var(--accent-text); background: var(--accent-subtle); }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-4); margin: var(--space-5) 0; }
  dt { color: var(--text-secondary); } dd { margin: 0; } .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 12px/20px Consolas, monospace; }
  .component-error { display: flex; gap: var(--space-2); padding: var(--space-3); border-radius: var(--radius-medium); color: var(--status-error); background: var(--status-error-bg); }
  .actions { display: flex; align-items: center; flex-wrap: wrap; gap: var(--space-2); margin-top: auto; }
  @media (max-width: 1120px) { .features { grid-template-columns: repeat(3, minmax(0, 1fr)); } }
  @media (max-width: 820px) { .component-grid { grid-template-columns: 1fr; } .features { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 520px) { .features { grid-template-columns: 1fr; } .catalog-row { grid-template-columns: 34px minmax(0, 1fr); } .catalog-expiry { display: none; } }
</style>
