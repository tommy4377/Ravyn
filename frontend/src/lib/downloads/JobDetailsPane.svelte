<script lang="ts">
  import { describeError } from "../api/errors";
  import type { BulkJobAction, Job, JobActionRecord, JobLogRecord, JobOutput, TrustReport } from "../api/types";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Tabs, { type TabItem } from "../components/Tabs.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { openNativePath, revealNativePath } from "../native/tauri";
  import { formatAbsoluteTime, formatBytes, formatEta, formatPercent, formatSpeed } from "../util/format";
  import { permittedActions, presentStatus } from "./jobPresentation";

  let { jobId, onClose }: { jobId: string; onClose: () => void } = $props();

  const job = $derived(jobsStore.byId.get(jobId));
  const live = $derived(jobsStore.liveProgress.get(jobId));
  const status = $derived(job ? presentStatus(job.status) : null);
  const permitted = $derived(job ? permittedActions(job.status, job.kind) : null);

  let tab = $state("overview");
  const tabs: TabItem[] = [
    { id: "overview", label: "Overview" },
    { id: "outputs", label: "Outputs" },
    { id: "activity", label: "Activity" },
    { id: "security", label: "Security" },
    { id: "advanced", label: "Advanced" },
  ];

  let outputs = $state<JobOutput[] | null>(null);
  let outputsError = $state<string | null>(null);
  let actions = $state<JobActionRecord[] | null>(null);
  let logs = $state<JobLogRecord[] | null>(null);
  let activityError = $state<string | null>(null);
  let segmentSummary = $state<string | null>(null);
  let actionBusy = $state(false);
  let trust = $state<TrustReport | null>(null);
  let trustError = $state<string | null>(null);
  let tags = $state<string[] | null>(null);
  let tagsDraft = $state("");
  let tagsEditing = $state(false);
  let tagsBusy = $state(false);

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
      trust = null;
      trustError = null;
      tags = null;
      tagsDraft = "";
      tagsEditing = false;
      tab = "overview";
      loadedForJobId = jobId;
    }

    if (!service || !jobId) return;
    if (tab === "overview" && tags === null) {
      service
        .tags(jobId)
        .then((names) => (tags = names))
        .catch(() => (tags = []));
    } else if (tab === "security" && trust === null && trustError === null) {
      service
        .trust(jobId)
        .then((report) => (trust = report))
        .catch((error) => (trustError = describeError(error)));
    } else if (tab === "outputs" && outputs === null) {
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

  async function runNativePathAction(path: string, action: "open" | "reveal"): Promise<void> {
    try {
      if (action === "open") await openNativePath(path);
      else await revealNativePath(path);
    } catch (cause) {
      notifications.error(
        action === "open" ? "Couldn't open this path" : "Couldn't reveal this path",
        describeError(cause),
      );
    }
  }

  async function runJobAction(action: Exclude<BulkJobAction, "delete">): Promise<void> {
    if (!service || !job || actionBusy) return;
    actionBusy = true;
    try {
      const [result] = await service.bulkAction(action, [job.id]);
      if (!result?.success) throw new Error(result?.error ?? `The ${action} action failed.`);
      notifications.info(`${action.charAt(0).toUpperCase()}${action.slice(1)} requested`);
      jobsStore.refreshAll();
    } catch (error) {
      notifications.error(`Couldn't ${action} this download`, describeError(error));
    } finally {
      actionBusy = false;
    }
  }

  function startTagEditing(): void {
    tagsDraft = (tags ?? []).join(", ");
    tagsEditing = true;
  }

  async function saveTags(): Promise<void> {
    if (!service || tagsBusy) return;
    tagsBusy = true;
    try {
      const next = tagsDraft
        .split(",")
        .map((tag) => tag.trim())
        .filter((tag) => tag.length > 0);
      tags = await service.replaceTags(jobId, next);
      tagsEditing = false;
      notifications.info("Tags updated");
    } catch (error) {
      notifications.error("Couldn't update tags", describeError(error));
    } finally {
      tagsBusy = false;
    }
  }

  function retryTrust(): void {
    trustError = null;
    trust = null;
  }

  function retryOutputs(): void {
    outputsError = null;
    outputs = null;
  }

  function retryActivity(): void {
    activityError = null;
    actions = null;
    logs = null;
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
          <div class="summary-header">
            <div class="row">
              <StatusBadge label={status.label} severity={status.severity} icon={status.icon} spinning={status.spinning} />
            </div>
            <div class="job-actions" aria-label="Download actions">
              <Button variant="subtle" onclick={() => void runNativePathAction(job.destination, "open")}><Icon name="folder-open" size={14} /> Open folder</Button>
              {#if permitted}
                {#if permitted.pause}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("pause")}><Icon name="pause" size={14} /> Pause</Button>{/if}
                {#if permitted.resume}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("resume")}><Icon name="play" size={14} /> Resume</Button>{/if}
                {#if permitted.retry}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("retry")}><Icon name="refresh" size={14} /> Retry</Button>{/if}
                {#if permitted.cancel}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("cancel")}><Icon name="cancel" size={14} /> Cancel</Button>{/if}
              {/if}
            </div>
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
            <dt>Tags</dt>
            <dd>
              {#if tagsEditing}
                <div class="tag-editor">
                  <input
                    class="tag-input"
                    type="text"
                    bind:value={tagsDraft}
                    placeholder="tag-one, tag-two"
                    aria-label="Tags, comma separated"
                    disabled={tagsBusy}
                    onkeydown={(event) => {
                      if (event.key === "Enter") void saveTags();
                      else if (event.key === "Escape") tagsEditing = false;
                    }}
                  />
                  <Button variant="subtle" disabled={tagsBusy} onclick={() => void saveTags()}>Save</Button>
                  <Button variant="subtle" disabled={tagsBusy} onclick={() => (tagsEditing = false)}>Cancel</Button>
                </div>
              {:else}
                <span class="tag-row">
                  <span>{tags === null ? "Loading…" : tags.length > 0 ? tags.join(", ") : "None"}</span>
                  {#if tags !== null}
                    <IconButton icon="edit" label="Edit tags" variant="subtle" onclick={startTagEditing} />
                  {/if}
                </span>
              {/if}
            </dd>
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
          <InlineError title="Couldn't load outputs" message={outputsError} retry={retryOutputs} />
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
                  <IconButton icon="external-link" label="Open file" variant="subtle" onclick={() => void runNativePathAction(output.current_path, "open")} />
                  <IconButton icon="folder-open" label="Show in Explorer" variant="subtle" onclick={() => void runNativePathAction(output.current_path, "reveal")} />
                  <IconButton icon="copy" label="Copy path" variant="subtle" onclick={() => copyPath(output.current_path)} />
                </div>
                <span class="output-meta">{output.output_type} · {output.state}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if tab === "activity"}
        {#if activityError}
          <InlineError title="Couldn't load activity" message={activityError} retry={retryActivity} />
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
      {:else if tab === "security"}
        {#if trustError}
          <InlineError title="Couldn't load the trust report" message={trustError} retry={retryTrust} />
        {:else if trust === null}
          <Skeleton height="120px" />
        {:else}
          <div class="trust-summary trust-{trust.level}">
            <Icon name="shield" size={20} />
            <div>
              <strong>Trust score: {trust.score}/100</strong>
              <span class="trust-level">{trust.level}</span>
            </div>
          </div>
          <p class="muted trust-note">
            Advisory evaluation of the download source. It informs, it does not block.
          </p>
          <ul class="trust-factors">
            {#each trust.factors as factor (factor.code)}
              <li class="trust-factor" class:satisfied={factor.satisfied}>
                <span class="factor-points" class:positive={factor.points > 0 && factor.satisfied}>
                  {factor.satisfied ? (factor.points > 0 ? `+${factor.points}` : factor.points) : "—"}
                </span>
                <div>
                  <span class="factor-label">{factor.label}</span>
                  <span class="factor-explanation">{factor.explanation}</span>
                </div>
              </li>
            {/each}
          </ul>
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
    width: 100%;
    height: 100%;
    min-width: 0;
    background: transparent;
    overflow: hidden;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 54px;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--stroke-divider);
    background: var(--bg-layer-alt);
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
  .summary-header {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
    padding-bottom: var(--space-4);
    border-bottom: 1px solid var(--stroke-divider);
  }
  .row { margin: 0; }
  .job-actions { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .job-actions :global(.button) { min-height: 28px; padding-inline: var(--space-2); font-size: var(--text-caption); }
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
  .tag-row {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .tag-editor {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .tag-input {
    flex: 1;
    min-width: 0;
    min-height: 26px;
    padding: 0 var(--space-2);
    font: inherit;
    font-size: var(--text-caption);
    color: var(--text-primary);
    background: var(--bg-subtle);
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-control);
  }
  .trust-summary {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-control);
    background: var(--bg-subtle);
  }
  .trust-summary strong {
    display: block;
  }
  .trust-level {
    color: var(--text-secondary);
    font-size: var(--text-caption);
    text-transform: capitalize;
  }
  .trust-note {
    font-size: var(--text-caption);
    margin: var(--space-2) 0 var(--space-3);
  }
  .trust-factors {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .trust-factor {
    display: flex;
    gap: var(--space-2);
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
  .trust-factor.satisfied {
    color: var(--text-primary);
  }
  .factor-points {
    flex: none;
    width: 32px;
    text-align: right;
    font-family: "Consolas", ui-monospace, monospace;
    color: var(--text-tertiary);
  }
  .factor-points.positive {
    color: var(--status-success, #2e7d32);
  }
  .factor-label {
    display: block;
    font-weight: 600;
  }
  .factor-explanation {
    display: block;
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
