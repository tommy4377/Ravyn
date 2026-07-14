<script lang="ts">
  import { describeError } from "../api/errors";
  import type { ComponentId, ComponentOverview, ComponentStatus, FeatureId } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes } from "../util/format";

  let overview = $state<ComponentOverview | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state<Partial<Record<ComponentId, string>>>({});
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
    seven_zip: "Extracts supported archive formats after a download completes.",
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
    if (component.state === "failed" || component.state === "unsupported") return "error";
    if (["queued", "downloading", "verifying", "installing"].includes(component.state)) return "info";
    return "neutral";
  }

  function statusLabel(state: ComponentStatus["state"]): string {
    return state.replaceAll("_", " ").replace(/^./, (char) => char.toUpperCase());
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      overview = await connection.client.getComponents();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  $effect(() => { void load(); });

  async function run(component: ComponentStatus, operation: "install" | "update" | "verify" | "rollback" | "cleanup"): Promise<void> {
    if (!connection.client) return;
    busy = { ...busy, [component.component]: operation };
    try {
      if (operation === "install") {
        await connection.client.installComponent(component.component);
        notifications.success(`${names[component.component]} installation started`);
      } else if (operation === "update") {
        await connection.client.updateComponent(component.component);
        notifications.success(`${names[component.component]} update started`);
      } else if (operation === "verify") {
        const health = await connection.client.verifyComponent(component.component);
        if (health.healthy) notifications.success(`${names[component.component]} verified`, health.version ?? undefined);
        else notifications.error(`${names[component.component]} verification failed`, health.message ?? undefined);
      } else if (operation === "rollback") {
        await connection.client.rollbackComponent(component.component);
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

<div class="page">
  <PageHeader title="Components" description="Optional tools are installed and maintained separately from the Ravyn application.">
    {#snippet actions()}<Button onclick={() => void load()}><Icon name="refresh" size={16} /> Check again</Button>{/snippet}
  </PageHeader>

  <div class="content">
    {#if error}
      <InlineError title="Couldn't load components" message={error} retry={() => void load()} />
    {:else if loading}
      <Surface><p class="muted">Checking installed components…</p></Surface>
    {:else if !overview}
      <EmptyState icon="components" title="Component information unavailable" />
    {:else}
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
              <div><h2>{names[component.component]}</h2><p>{descriptions[component.component]}</p></div>
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
              {#if component.state === "not_installed" || component.state === "failed"}
                <Button variant="accent" disabled={!!busy[component.component]} onclick={() => void run(component, "install")}><Icon name="download" size={16} /> {busy[component.component] === "install" ? "Installing…" : "Install"}</Button>
              {:else if component.state === "update_available"}
                <Button variant="accent" disabled={!!busy[component.component]} onclick={() => void run(component, "update")}><Icon name="download" size={16} /> Update</Button>
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
  .muted, .section-heading p { color: var(--text-secondary); }
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
  @media (max-width: 520px) { .features { grid-template-columns: 1fr; } }
</style>
