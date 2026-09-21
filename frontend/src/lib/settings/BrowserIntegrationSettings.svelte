<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import {
    browserIntegrationStatus,
    removeBrowserIntegration,
    repairBrowserIntegration,
    promptTorrentDefaultApp,
    type BrowserIntegrationStatus,
  } from "../native/tauri";
  import { notifications } from "../stores/notifications.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";

  let status = $state<BrowserIntegrationStatus | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    void refresh();
  });

  async function refresh(): Promise<void> {
    loading = true;
    error = null;
    try {
      status = await browserIntegrationStatus();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading = false;
    }
  }

  async function repair(): Promise<void> {
    busy = true;
    error = null;
    try {
      status = await repairBrowserIntegration();
      notifications.success("Firefox integration repaired", "The native messaging host is registered for the current user.");
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function remove(): Promise<void> {
    busy = true;
    error = null;
    try {
      status = await removeBrowserIntegration();
      notifications.success("Firefox integration removed");
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function setTorrentDefault(): Promise<void> {
    busy = true;
    error = null;
    try {
      await promptTorrentDefaultApp();
      notifications.info("Choose Ravyn for torrent files", "Windows Default Apps is open. Select Ravyn for .torrent files and magnet links.");
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<SettingsCategoryHeader
  title="Firefox Integration"
  description="Connect the Ravyn Firefox extension through a restricted per-user native messaging host."
/>

{#if error}
  <InlineError title="Firefox integration could not be updated" message={error} retry={() => void refresh()} />
{/if}

<Surface padding="none">
  <div class="status-row">
    <div class="status-copy">
      <div class="heading-row">
        <h3>Native messaging host</h3>
        {#if loading}
          <StatusBadge label="Checking" severity="info" spinning />
        {:else if status?.registered}
          <StatusBadge label="Registered" severity="success" icon="check-circle" />
        {:else}
          <StatusBadge label="Not registered" severity="error" icon="alert-circle" />
        {/if}
      </div>
      <p>Firefox starts a short-lived Ravyn process that accepts only validated download, media, rule, and job-control commands.</p>
    </div>
    <div class="actions">
      <Button disabled={loading || busy || !status?.installed_mode} onclick={() => void repair()}>
        <Icon name={busy ? "spinner" : "refresh"} size={15} />
        {busy ? "Working…" : "Repair registration"}
      </Button>
      <Button variant="subtle" disabled={loading || busy || !status?.registered} onclick={() => void remove()}>
        Remove
      </Button>
    </div>
  </div>

  <dl>
    <dt>Host name</dt><dd>{status?.host_name ?? "com.ravyn.download_manager"}</dd>
    <dt>Extension ID</dt><dd>{status?.extension_id ?? "firefox-extension@ravyn.app"}</dd>
    <dt>Manifest</dt><dd class="path">{status?.manifest_path ?? "Unavailable"}</dd>
    <dt>Executable</dt><dd class="path">{status?.executable_path ?? "Unavailable"}</dd>
  </dl>
</Surface>

<Surface>
  <div class="status-row">
    <div class="status-copy">
      <div class="heading-row"><h3>Torrent default app</h3></div>
      <p>Register Ravyn for .torrent files and magnet links, then choose it in Windows Default Apps. Windows keeps this choice under your control.</p>
    </div>
    <Button disabled={busy} onclick={() => void setTorrentDefault()}>Set as default…</Button>
  </div>
</Surface>

<Surface>
  <div class="instructions">
    <Icon name="shield" size={22} />
    <div>
      <h3>Install the signed Firefox package</h3>
      <p>The release workflow produces <code>ravyn-firefox.xpi</code> and a matching source archive. Install the XPI from the same Ravyn release, then open its popup to verify the connection.</p>
      {#if status && !status.installed_mode}
        <p class="warning">Native messaging requires installed mode. Complete Ravyn installation before repairing this registration.</p>
      {/if}
    </div>
  </div>
</Surface>

<style>
  :global(.category-content) { display: flex; flex-direction: column; gap: var(--space-4); }
  .status-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-5); padding: var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .status-copy { min-width: 0; }
  .heading-row { display: flex; align-items: center; gap: var(--space-3); }
  h3, p { margin: 0; }
  h3 { font-size: var(--text-body); font-weight: 620; }
  p { margin-top: var(--space-1); color: var(--text-secondary); line-height: 1.5; }
  .actions { display: flex; align-items: center; gap: var(--space-2); flex: none; }
  dl { display: grid; grid-template-columns: minmax(120px, 180px) minmax(0, 1fr); margin: 0; }
  dt, dd { min-height: 46px; display: flex; align-items: center; padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  dt { color: var(--text-secondary); }
  dd { margin: 0; }
  .path, code { overflow-wrap: anywhere; font: 12px/1.5 ui-monospace, "Cascadia Code", Consolas, monospace; }
  .instructions { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: var(--space-3); }
  .warning { color: var(--status-warning-text); }
  @media (max-width: 700px) {
    .status-row { align-items: stretch; flex-direction: column; }
    .actions { flex-wrap: wrap; }
    dl { grid-template-columns: 1fr; }
    dt { min-height: 30px; padding-bottom: 0; border-bottom: 0; }
  }
</style>
