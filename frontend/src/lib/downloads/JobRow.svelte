<script lang="ts">
  import ContextMenu from "../components/ContextMenu.svelte";
  import Icon from "../components/Icon.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import type { Job } from "../api/types";
  import { jobsStore } from "../stores/jobs.svelte";
  import {
    formatBytes,
    formatEta,
    formatPercent,
    formatRelativeTime,
    formatSpeed,
    jobDisplayName,
  } from "../util/format";
  import { permittedActions, presentStatus } from "./jobPresentation";
  import { buildJobMenuItems, type JobRowActions } from "./jobActions";

  let {
    job,
    selected,
    focused,
    actions,
    onSelect,
    onOpenDetails,
  }: {
    job: Job;
    selected: boolean;
    focused: boolean;
    actions: JobRowActions;
    onSelect: (job: Job, event: MouseEvent) => void;
    onOpenDetails: (job: Job) => void;
  } = $props();

  const status = $derived(presentStatus(job.status));
  const live = $derived(jobsStore.liveProgress.get(job.id));
  const downloaded = $derived(live?.downloadedBytes ?? job.downloaded_bytes);
  const total = $derived(live?.totalBytes ?? job.total_bytes);
  const speed = $derived(live?.bytesPerSecond ?? 0);
  const permitted = $derived(permittedActions(job.status, job.kind));
  const menuItems = $derived(buildJobMenuItems(job, permitted, actions));
  const name = $derived(jobDisplayName(job.source, job.filename));
</script>

<ContextMenu items={menuItems}>
  <div
    id="job-row-{job.id}"
    class="row"
    class:selected
    class:focused
    role="option"
    aria-selected={selected}
    tabindex="-1"
    onclick={(event) => onSelect(job, event)}
    ondblclick={() => onOpenDetails(job)}
    onkeydown={(event) => {
      if (event.key === "Enter") onOpenDetails(job);
    }}
  >
    <div class="cell name" title={name}>
      <Icon name={status.spinning ? "spinner" : status.icon} size={15} />
      <span class="text">{name}</span>
    </div>
    <div class="cell status">
      <StatusBadge label={status.label} severity={status.severity} />
    </div>
    <div class="cell progress">
      <div class="bar" role="progressbar" aria-valuenow={total ? Math.round((downloaded / total) * 100) : undefined} aria-valuemin={0} aria-valuemax={100}>
        <div class="fill" style="width:{total ? Math.min(100, (downloaded / total) * 100) : 0}%"></div>
      </div>
      <span class="pct">{formatPercent(downloaded, total)}</span>
    </div>
    <div class="cell speed">{job.status === "downloading" ? formatSpeed(speed) : "—"}</div>
    <div class="cell eta">{job.status === "downloading" ? formatEta(downloaded, total, speed) : "—"}</div>
    <div class="cell size">{formatBytes(total)}</div>
    <div class="cell added">{formatRelativeTime(job.created_at)}</div>
  </div>
</ContextMenu>

<style>
  .row {
    display: grid;
    grid-template-columns: 2.2fr 1fr 1.3fr 0.8fr 0.8fr 0.7fr 0.9fr;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 100%;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--stroke-divider);
    cursor: default;
  }
  .row:hover {
    background: var(--bg-subtle-hover);
  }
  .row.selected {
    background: var(--accent-subtle);
  }
  .row:focus-visible,
  .row.focused {
    outline: 2px solid var(--stroke-focus);
    outline-offset: -2px;
  }
  .cell {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-body);
  }
  .cell.name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .cell.name .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cell.progress {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .bar {
    flex: 1;
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--bg-subtle);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent-default);
  }
  .pct {
    flex: none;
    color: var(--text-secondary);
    font-size: var(--text-caption);
    width: 3ch;
  }
  .speed,
  .eta,
  .size,
  .added {
    color: var(--text-secondary);
  }
</style>
