<script lang="ts">
  /**
   * Minimal always-on-top progress view for the compact window. It owns a
   * lightweight independent connection (no shared shell state) so it stays
   * usable even if the main window's store never mounts, and closes itself
   * once nothing is left transferring.
   */
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { RavynClient } from "../api/client";
  import { RavynEventClient } from "../api/events.svelte";
  import type { Job, ProgressEvent, RavynEvent } from "../api/types";
  import Icon from "../components/Icon.svelte";
  import { backendInfo, focusMainWindow } from "../native/tauri";
  import { formatPercent, formatSpeed, jobDisplayName } from "../util/format";

  const ACTIVE_STATUSES = new Set(["probing", "downloading", "paused", "verifying", "post_processing", "seeding"]);

  let jobs = $state<Job[]>([]);
  let liveProgress = $state<Map<string, { downloaded: number; total: number | null; speed: number }>>(new Map());
  let client: RavynClient | null = null;
  let events: RavynEventClient | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  const activeJobs = $derived(jobs.filter((job) => ACTIVE_STATUSES.has(job.status)));

  function scheduleAutoClose(): void {
    if (closeTimer) return;
    // Give a just-finished job a moment on screen before the window closes.
    closeTimer = setTimeout(() => void getCurrentWindow().close(), 2500);
  }

  function cancelAutoClose(): void {
    if (closeTimer) {
      clearTimeout(closeTimer);
      closeTimer = undefined;
    }
  }

  async function refresh(): Promise<void> {
    if (!client) return;
    const page = await client.listJobs({ limit: 50 });
    jobs = page.items;
    if (activeJobs.length === 0) scheduleAutoClose();
    else cancelAutoClose();
  }

  function applyEvent(event: RavynEvent): void {
    if (event.type === "progress") {
      const e = event as ProgressEvent;
      const next = new Map(liveProgress);
      next.set(e.job_id, { downloaded: e.downloaded_bytes, total: e.total_bytes, speed: e.bytes_per_second });
      liveProgress = next;
      return;
    }
    if (event.type === "job_status" || event.type === "queue_changed" || event.type === "resync_required") {
      void refresh();
    }
  }

  $effect(() => {
    let cancelled = false;
    void (async () => {
      const backend = await backendInfo();
      if (cancelled) return;
      client = new RavynClient(backend.base_url, backend.api_token);
      events = new RavynEventClient(backend.base_url, backend.api_token);
      events.subscribe(applyEvent);
      events.connect();
      await refresh();
    })();
    return () => {
      cancelled = true;
      events?.close();
      cancelAutoClose();
    };
  });

  function progressFor(job: Job): { downloaded: number; total: number | null; speed: number } {
    return liveProgress.get(job.id) ?? { downloaded: job.downloaded_bytes, total: job.total_bytes, speed: 0 };
  }
</script>

<div class="compact" role="main">
  <div class="titlebar" data-tauri-drag-region>
    <span class="title">Ravyn downloads</span>
    <div class="titlebar-actions">
      <button class="icon-button" title="Open Ravyn" onclick={() => void focusMainWindow()}>
        <Icon name="folder-open" size={13} />
      </button>
      <button class="icon-button" title="Close" onclick={() => void getCurrentWindow().close()}>
        <Icon name="close" size={13} />
      </button>
    </div>
  </div>

  <div class="body">
    {#if activeJobs.length === 0}
      <p class="empty">No active downloads</p>
    {:else}
      {#each activeJobs as job (job.id)}
        {@const progress = progressFor(job)}
        {@const percent = progress.total ? Math.min(100, (progress.downloaded / progress.total) * 100) : 0}
        <div class="row">
          <div class="row-header">
            <span class="name" title={jobDisplayName(job.source, job.filename)}>{jobDisplayName(job.source, job.filename)}</span>
            <span class="speed mono">{formatSpeed(progress.speed)}</span>
          </div>
          <div class="bar"><div class="fill" style:width={`${percent}%`}></div></div>
          <span class="percent mono">{formatPercent(progress.downloaded, progress.total)}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .compact {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary, #1f1f1f);
    color: var(--text-primary, #fff);
    border: 1px solid var(--stroke-divider, rgba(255, 255, 255, 0.12));
    border-radius: var(--radius-medium, 8px);
    overflow: hidden;
  }
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 6px 12px;
    font-size: 12px;
    font-weight: 600;
    flex: none;
    -webkit-app-region: drag;
  }
  .titlebar-actions {
    display: flex;
    gap: 2px;
    -webkit-app-region: no-drag;
  }
  .icon-button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 0;
    background: transparent;
    color: inherit;
    border-radius: var(--radius-small, 4px);
    cursor: default;
  }
  .icon-button:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 12px 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .empty {
    margin: auto;
    color: var(--text-tertiary, rgba(255, 255, 255, 0.5));
    font-size: 12px;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .speed {
    flex: none;
    color: var(--text-secondary, rgba(255, 255, 255, 0.7));
  }
  .bar {
    height: 3px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.15);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent-default, #4cc2ff);
    transition: width 160ms linear;
  }
  .percent {
    font-size: 11px;
    color: var(--text-tertiary, rgba(255, 255, 255, 0.55));
  }
  .mono {
    font-family: var(--font-family-mono, monospace);
    font-variant-numeric: tabular-nums;
  }
</style>
