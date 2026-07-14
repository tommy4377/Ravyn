<script lang="ts">
  import type { Job } from "../api/types";
  import ContextMenu from "../components/ContextMenu.svelte";
  import Icon from "../components/Icon.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { formatBytes, formatEta, formatPercent, formatRelativeTime, formatSpeed, jobDisplayName } from "../util/format";
  import { buildJobMenuItems, type JobRowActions } from "./jobActions";
  import { permittedActions, presentStatus } from "./jobPresentation";

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
  const progress = $derived(total ? Math.min(100, (downloaded / total) * 100) : 0);
  const kindLabel = $derived(job.kind === "http" ? "Direct" : job.kind === "media" ? "Media" : "Torrent");
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
    onkeydown={(event) => { if (event.key === "Enter") onOpenDetails(job); }}
  >
    <div class="name-cell" title={name}>
      <div class="file-icon"><Icon name={job.kind === "torrent" ? "torrent" : job.kind === "media" ? "video" : "download"} size={17} /></div>
      <div class="name-copy"><strong>{name}</strong><span>{kindLabel} · {job.destination}</span></div>
    </div>

    <div class="status-cell">
      <div class="status-line">
        <StatusBadge label={status.label} severity={status.severity} />
        <span>{formatPercent(downloaded, total)}</span>
      </div>
      <div class="bar" role="progressbar" aria-valuenow={total ? Math.round(progress) : undefined} aria-valuemin={0} aria-valuemax={100}>
        <div class="fill" style:width={`${progress}%`}></div>
      </div>
    </div>

    <div class="transfer-cell">
      <strong>{job.status === "downloading" ? formatSpeed(speed) : "—"}</strong>
      <span>{job.status === "downloading" ? formatEta(downloaded, total, speed) : status.label}</span>
    </div>
    <div class="size-cell">{formatBytes(total)}</div>
    <div class="added-cell">{formatRelativeTime(job.created_at)}</div>
  </div>
</ContextMenu>

<style>
  .row { display: grid; grid-template-columns: minmax(220px, 2fr) minmax(180px, 1.25fr) minmax(115px, .72fr) minmax(82px, .55fr) minmax(96px, .64fr); align-items: center; gap: var(--space-3); width: 100%; height: 100%; padding: 0 var(--space-4); border-bottom: 1px solid var(--stroke-divider); cursor: default; transition: background var(--motion-fast) var(--motion-easing), box-shadow var(--motion-fast) var(--motion-easing); }
  .row:hover { background: color-mix(in srgb, var(--bg-subtle-hover) 74%, transparent); }
  .row.selected { background: color-mix(in srgb, var(--accent-subtle) 54%, transparent); box-shadow: inset 2px 0 var(--accent-default); }
  .row.focused { outline: 2px solid var(--stroke-focus); outline-offset: -2px; }
  .name-cell { min-width: 0; display: flex; align-items: center; gap: var(--space-3); }
  .file-icon { display: grid; place-items: center; width: 32px; height: 32px; flex: none; border-radius: var(--radius-medium); color: var(--text-secondary); background: var(--bg-subtle); border: 1px solid var(--stroke-divider); }
  .name-copy, .transfer-cell { min-width: 0; display: flex; flex-direction: column; }
  .name-copy strong, .name-copy span, .transfer-cell strong, .transfer-cell span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name-copy strong, .transfer-cell strong { font-size: var(--text-body); font-weight: 600; }
  .name-copy span, .transfer-cell span { color: var(--text-tertiary); font-size: var(--text-caption); }
  .status-cell { min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .status-line { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); min-width: 0; }
  .status-line > span { flex: none; color: var(--text-secondary); font-size: var(--text-caption); }
  .bar { height: 3px; overflow: hidden; border-radius: var(--radius-pill); background: var(--bg-subtle); }
  .fill { height: 100%; min-width: 0; border-radius: inherit; background: var(--accent-default); transition: width 160ms linear; }
  .size-cell, .added-cell { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-secondary); font-size: var(--text-caption); }
  @media (max-width: 980px) {
    .row { grid-template-columns: minmax(210px, 2fr) minmax(175px, 1.25fr) minmax(105px, .72fr) minmax(78px, .55fr); }
    .added-cell { display: none; }
  }
  @media (max-width: 760px) {
    .row { grid-template-columns: minmax(0, 1fr) auto; grid-template-rows: 32px 18px; gap: 2px var(--space-3); padding-block: 4px; }
    .name-cell { grid-column: 1; grid-row: 1; }
    .status-cell { grid-column: 1 / -1; grid-row: 2; flex-direction: row; align-items: center; gap: var(--space-2); padding-left: 44px; }
    .status-line { flex: none; }
    .status-line :global(.badge) { display: none; }
    .bar { flex: 1; }
    .transfer-cell { grid-column: 2; grid-row: 1; text-align: right; }
    .size-cell, .added-cell { display: none; }
  }
</style>
