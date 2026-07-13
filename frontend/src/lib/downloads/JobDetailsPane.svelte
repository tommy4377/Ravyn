<script lang="ts">
  import { describeError } from "../api/errors";
  import type { Job, JobActionRecord, JobLogRecord, JobOutput } from "../api/types";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Tabs, { type TabItem } from "../components/Tabs.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes, formatEta, formatPercent, formatSpeed } from "../util/format";
  import { presentStatus } from "./jobPresentation";

  let { jobId, onClose }: { jobId: string; onClose: () => void } = $props();

  const job = $derived(jobsStore.byId.get(jobId));
  const live = $derived(jobsStore.liveProgress.get(jobId));
  const status = $derived(job ? presentStatus(job.status) : null);

  let tab = $state("overview");
  const tabs: TabItem[] = [
    { id: "overview", label: "Overview" },
    { id: "outputs", label: "Outputs" },
    { id: "activity", label: "Activity" },
    { id: "advanced", label: "Advanced" },
  ];

  let outputs = $state<JobOutput[] | null>(null);
  let outputsError = $state<string | null>(null);
  let actions = $state<JobActionRecord[] | null>(null);
  let logs = $state<JobLogRecord[] | null>(null);
  let activityError = $state<string | null>(null);
  let segmentSummary = $state<string | null>(null);

  const service = $derived(connection.client ? new JobsService(connection.client) : null);

  // A single effect (rather than two) so a job-id change resets cached tab
  // data and decides what to fetch in the same synchronous pass — avoiding
  // a cross-effect ordering race that could fetch the previous job's tab
  // for an instant before the reset effect runs.
  let loadedForJobId: string | null = null;
  $effect(() => {
    if (jobId !== loadedForJobId) {
      outputs = null;
      outputsError = null;
      actions = null;
      logs = null;
      activityError = null;
      segmentSummary = null;
      tab = "overview";
      loadedForJobId = jobId;
    }

    if (!service || !jobId) return;
    if (tab === "outputs" && outputs === null) {
      service
        .outputs(jobId)
        .then((page) => (outputs = page.items))
        .catch((error) => (outputsError = describeError(error)));
    } else if (tab === "activity" && actions === null) {
      Promise.all([service.actions(jobId), service.logs(jobId, { limit: 50 })])
        .then(([actionsPage, logsPage]) => {
          actions = actionsPage.items;
          logs = logsPage.items;
        })
        .catch((error) => (activityError = describeError(error)));
    } else if (tab === "advanced" && segmentSummary === null) {
      service
        .segments(jobId)
        .then((page) => {
          const byState = new Map<string, number>();
          for (const segment of page.items) {
            byState.set(segment.state, (byState.get(segment.state) ?? 0) + 1);
          }
          segmentSummary =
            page.items.length === 0
              ? "No segment data for this job."
              : [...byState.entries()].map(([state, count]) => `${count} ${state}`).join(", ");
        })
        .catch(() => (segmentSummary = "Segment data unavailable."));
    }
  });

  async function copyPath(path: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(path);
      notifications.info("Path copied");
    } catch {
      notifications.warning("Couldn't copy to the clipboard");
    }
  }
</script>

<aside class="pane" aria-label="Download details">
  <header class="header">
    <h2>Details</h2>
    <IconButton icon="close" label="Close details" variant="subtle" onclick={onClose} />
  </header>

  {#if !job}
    <div class="loading"><Skeleton height="120px" /></div>
  {:else}
    <Tabs {tabs} bind:selected={tab} />

    <div class="content">
      {#if tab === "overview" && status}
        <section class="overview">
          <div class="row">
            <StatusBadge label={status.label} severity={status.severity} icon={status.icon} spinning={status.spinning} />
          </div>
          {#if job.error}
            <InlineError title="Last error" message={job.error} />
          {/if}
          {#if job.status === "downloading"}
            <dl>
              <dt>Progress</dt>
              <dd>{formatPercent(live?.downloadedBytes ?? job.downloaded_bytes, live?.totalBytes ?? job.total_bytes)}</dd>
              <dt>Speed</dt>
              <dd>{formatSpeed(live?.bytesPerSecond ?? 0)}</dd>
              <dt>ETA</dt>
              <dd>{formatEta(live?.downloadedBytes ?? job.downloaded_bytes, live?.totalBytes ?? job.total_bytes, live?.bytesPerSecond ?? 0)}</dd>
            </dl>
          {/if}
          <dl>
            <dt>Source</dt>
            <dd class="wrap">{job.source}</dd>
            <dt>Destination</dt>
            <dd class="wrap">{job.destination}</dd>
            {#if job.filename}
              <dt>File name</dt>
              <dd class="wrap">{job.filename}</dd>
            {/if}
            <dt>Size</dt>
            <dd>{formatBytes(live?.totalBytes ?? job.total_bytes)}</dd>
            <dt>Kind</dt>
            <dd>{job.kind}</dd>
            <dt>Priority</dt>
            <dd>{job.priority}</dd>
            {#if job.expected_sha256}
              <dt>Expected SHA-256</dt>
              <dd class="wrap mono">{job.expected_sha256}</dd>
            {/if}
            {#if job.options_json.tags?.length}
              <dt>Tags</dt>
              <dd>{job.options_json.tags.join(", ")}</dd>
            {/if}
            <dt>Added</dt>
            <dd>{formatAbsoluteTime(job.created_at)}</dd>
            {#if job.started_at}
              <dt>Started</dt>
              <dd>{formatAbsoluteTime(job.started_at)}</dd>
            {/if}
            {#if job.completed_at}
              <dt>Completed</dt>
              <dd>{formatAbsoluteTime(job.completed_at)}</dd>
            {/if}
          </dl>
        </section>
      {:else if tab === "outputs"}
        {#if outputsError}
          <InlineError title="Couldn't load outputs" message={outputsError} />
        {:else if outputs === null}
          <Skeleton height="80px" />
        {:else if outputs.length === 0}
          <p class="muted">No output files yet.</p>
        {:else}
          <ul class="outputs">
            {#each outputs as output (output.id)}
              <li>
                <div class="output-row">
                  <span class="path" title={output.current_path}>{output.relative_path}</span>
                  <span class="size">{formatBytes(output.size_bytes)}</span>
                  <IconButton icon="paste" label="Copy path" variant="subtle" onclick={() => copyPath(output.current_path)} />
                </div>
                <span class="output-meta">{output.output_type} · {output.state}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if tab === "activity"}
        {#if activityError}
          <InlineError title="Couldn't load activity" message={activityError} />
        {:else if actions === null}
          <Skeleton height="80px" />
        {:else}
          {#if actions.length > 0}
            <h3 class="subheading">Post-processing</h3>
            <ul class="actions">
              {#each actions as action (action.id)}
                <li>
                  <span>{action.action.type}</span>
                  <span class="muted">{action.state}{action.error ? ` — ${action.error}` : ""}</span>
                </li>
              {/each}
            </ul>
          {/if}
          <h3 class="subheading">Recent log entries</h3>
          {#if !logs || logs.length === 0}
            <p class="muted">No log entries.</p>
          {:else}
            <ul class="logs">
              {#each logs as entry (entry.id)}
                <li class="log-entry {entry.severity}">
                  <span class="log-time">{formatAbsoluteTime(entry.timestamp)}</span>
                  <span class="log-message">{entry.message}</span>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      {:else if tab === "advanced"}
        <h3 class="subheading">Segments</h3>
        <p class="muted">{segmentSummary ?? "Loading…"}</p>
        <h3 class="subheading">Raw options</h3>
        <pre class="json">{JSON.stringify(job.options_json, null, 2)}</pre>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    width: 340px;
    flex: none;
    border-left: 1px solid var(--stroke-divider);
    background: var(--bg-layer);
    overflow: hidden;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--stroke-divider);
  }
  .header h2 {
    margin: 0;
    font-size: var(--text-body-strong);
    font-weight: 600;
  }
  .loading {
    padding: var(--space-4);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4);
  }
  .row {
    margin-bottom: var(--space-3);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-body);
    margin: 0 0 var(--space-4);
  }
  dt {
    color: var(--text-secondary);
  }
  dd {
    margin: 0;
  }
  dd.wrap {
    word-break: break-all;
  }
  dd.mono {
    font-family: "Consolas", ui-monospace, monospace;
    font-size: var(--text-caption);
  }
  .muted {
    color: var(--text-secondary);
  }
  .subheading {
    font-size: var(--text-caption);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-tertiary);
    margin: var(--space-4) 0 var(--space-2);
  }
  .subheading:first-child {
    margin-top: 0;
  }
  .outputs,
  .actions,
  .logs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .output-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-caption);
    font-family: "Consolas", ui-monospace, monospace;
  }
  .size {
    color: var(--text-secondary);
    font-size: var(--text-caption);
  }
  .output-meta {
    display: block;
    color: var(--text-tertiary);
    font-size: var(--text-caption);
  }
  .actions li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    font-size: var(--text-caption);
  }
  .log-entry {
    display: flex;
    gap: var(--space-2);
    font-size: var(--text-caption);
  }
  .log-entry.error {
    color: var(--status-error);
  }
  .log-entry.warn,
  .log-entry.warning {
    color: var(--status-warning);
  }
  .log-time {
    flex: none;
    color: var(--text-tertiary);
  }
  .json {
    font-size: var(--text-caption);
    background: var(--bg-subtle);
    border-radius: var(--radius-control);
    padding: var(--space-3);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
