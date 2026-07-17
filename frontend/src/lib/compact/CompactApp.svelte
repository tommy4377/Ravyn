<script lang="ts">
  /**
   * Minimal always-on-top progress view for the compact window. It owns a
   * lightweight independent connection (no shared shell state) so it stays
   * usable even if the main window's store never mounts, and closes itself
   * once nothing is left transferring.
   */
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { RavynClient } from "../api/client";
  import { RavynEventClient } from "../api/events.svelte";
  import type { Job, ProgressEvent, RavynEvent } from "../api/types";
  import Icon from "../components/Icon.svelte";
  import type { IconName } from "../components/Icon.svelte";
  import { permittedActions } from "../downloads/jobPresentation";
  import { backendInfo, focusMainWindow } from "../native/tauri";
  import { formatBytes, formatEta, formatPercent, formatSpeed, jobDisplayName } from "../util/format";

  const ACTIVE_STATUSES = new Set(["probing", "downloading", "paused", "verifying", "post_processing", "seeding"]);

  const WINDOW_WIDTH = 400;
  const HEADER_HEIGHT = 40;
  const EMPTY_BODY_HEIGHT = 76;
  const ROW_HEIGHT = 68;
  const ROW_GAP = 8;
  const BODY_PADDING = 20;
  const MAX_VISIBLE_ROWS = 4;

  const KIND_ICON: Record<Job["kind"], IconName> = {
    http: "file",
    media: "video",
    torrent: "torrent",
  };

  let jobs = $state<Job[]>([]);
  let liveProgress = $state<Map<string, { downloaded: number; total: number | null; speed: number }>>(new Map());
  let pending = $state<Set<string>>(new Set());
  let client: RavynClient | null = null;
  let events: RavynEventClient | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  const activeJobs = $derived(jobs.filter((job) => ACTIVE_STATUSES.has(job.status)));
  const totalSpeed = $derived(
    activeJobs.reduce((sum, job) => sum + progressFor(job).speed, 0),
  );

  $effect(() => {
    const rows = Math.min(activeJobs.length, MAX_VISIBLE_ROWS);
    const bodyHeight = activeJobs.length === 0 ? EMPTY_BODY_HEIGHT : rows * ROW_HEIGHT + (rows - 1) * ROW_GAP + BODY_PADDING;
    void getCurrentWindow().setSize(new LogicalSize(WINDOW_WIDTH, HEADER_HEIGHT + bodyHeight));
  });

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

  async function runAction(id: string, action: (id: string) => Promise<void>): Promise<void> {
    if (pending.has(id)) return;
    pending = new Set(pending).add(id);
    try {
      await action(id);
      await refresh();
    } finally {
      const next = new Set(pending);
      next.delete(id);
      pending = next;
    }
  }

  function pause(id: string): void {
    if (client) void runAction(id, (jobId) => client!.pauseJob(jobId));
  }

  function resume(id: string): void {
    if (client) void runAction(id, (jobId) => client!.resumeJob(jobId));
  }

  function cancel(id: string): void {
    if (client) void runAction(id, (jobId) => client!.cancelJob(jobId));
  }
</script>

<div class="compact" role="main">
  <div class="titlebar" data-tauri-drag-region>
    <div class="brand">
      <span class="mark"><Icon name="download" size={12} /></span>
      <span class="title">
        {#if activeJobs.length > 1}
          {activeJobs.length} downloads · {formatSpeed(totalSpeed)}
        {:else}
          Ravyn downloads
        {/if}
      </span>
    </div>
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
      <div class="empty">
        <Icon name="check-circle" size={22} />
        <p>No active downloads</p>
      </div>
    {:else}
      {#each activeJobs as job (job.id)}
        {@const progress = progressFor(job)}
        {@const percent = progress.total ? Math.min(100, (progress.downloaded / progress.total) * 100) : 0}
        {@const actions = permittedActions(job.status, job.kind)}
        {@const busy = pending.has(job.id)}
        <div class="row">
          <span class="kind-icon" data-status={job.status}>
            <Icon name={KIND_ICON[job.kind]} size={15} />
          </span>
          <div class="row-main">
            <div class="row-header">
              <span class="name" title={jobDisplayName(job.source, job.filename)}>{jobDisplayName(job.source, job.filename)}</span>
              <div class="row-controls">
                {#if actions.pause}
                  <button class="control" title="Pause" disabled={busy} onclick={() => pause(job.id)}>
                    <Icon name="pause" size={12} />
                  </button>
                {:else if actions.resume}
                  <button class="control" title="Resume" disabled={busy} onclick={() => resume(job.id)}>
                    <Icon name="play" size={12} />
                  </button>
                {/if}
                {#if actions.cancel}
                  <button class="control" title="Cancel" disabled={busy} onclick={() => cancel(job.id)}>
                    <Icon name="cancel" size={12} />
                  </button>
                {/if}
              </div>
            </div>
            <div class="bar"><div class="fill" data-paused={job.status === "paused"} style:width={`${percent}%`}></div></div>
            <div class="row-footer mono">
              <span>{formatBytes(progress.downloaded)} / {progress.total ? formatBytes(progress.total) : "—"}</span>
              <span class="dot">·</span>
              <span>{formatPercent(progress.downloaded, progress.total)}</span>
              {#if job.status === "downloading"}
                <span class="dot">·</span>
                <span>{formatSpeed(progress.speed)}</span>
                <span class="dot">·</span>
                <span>{formatEta(progress.downloaded, progress.total, progress.speed)} left</span>
              {:else}
                <span class="dot">·</span>
                <span class="state">{job.status}</span>
              {/if}
            </div>
          </div>
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
    padding: 6px 8px 6px 10px;
    font-size: 12px;
    font-weight: 600;
    flex: none;
    -webkit-app-region: drag;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 5px;
    background: var(--accent-default, #4cc2ff);
    color: #08111a;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .titlebar-actions {
    display: flex;
    gap: 2px;
    flex: none;
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
    padding: 4px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    color: var(--text-tertiary, rgba(255, 255, 255, 0.5));
    font-size: 12px;
  }
  .empty p {
    margin: 0;
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .kind-icon {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    flex: none;
    margin-top: 2px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-secondary, rgba(255, 255, 255, 0.75));
  }
  .row-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
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
  .row-controls {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .control {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    background: transparent;
    color: var(--text-secondary, rgba(255, 255, 255, 0.7));
    border-radius: var(--radius-small, 4px);
    cursor: default;
  }
  .control:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    color: var(--text-primary, #fff);
  }
  .control:disabled {
    opacity: 0.4;
  }
  .bar {
    height: 4px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.15);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent-default, #4cc2ff);
    transition: width 160ms linear;
  }
  .fill[data-paused="true"] {
    background: var(--text-tertiary, rgba(255, 255, 255, 0.4));
  }
  .row-footer {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--text-tertiary, rgba(255, 255, 255, 0.55));
    overflow: hidden;
    white-space: nowrap;
  }
  .row-footer .state {
    text-transform: capitalize;
  }
  .dot {
    flex: none;
    opacity: 0.6;
  }
  .mono {
    font-family: var(--font-family-mono, monospace);
    font-variant-numeric: tabular-nums;
  }
</style>
