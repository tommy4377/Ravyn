<script lang="ts">
  import { describeError } from "../api/errors";
  import type { BulkJobAction, JobActionRecord, JobLogRecord, JobOutput, SegmentRecord, TrustReport } from "../api/types";
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Tabs, { type TabItem } from "../components/Tabs.svelte";
  import { openNativePath, revealNativePath } from "../native/tauri";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes, formatEta, formatPercent, formatSpeed, jobDisplayName } from "../util/format";
  import { permittedActions, presentStatus, presentTrust } from "./jobPresentation";

  let { jobId, onClose }: { jobId: string; onClose: () => void } = $props();

  const job = $derived(jobsStore.byId.get(jobId));
  const live = $derived(jobsStore.liveProgress.get(jobId));
  const status = $derived(job ? presentStatus(job.status) : null);
  const permitted = $derived(job ? permittedActions(job.status, job.kind) : null);
  const title = $derived(job ? jobDisplayName(job.source, job.filename) : "Download details");

  let tab = $state("overview");
  const tabs: TabItem[] = [
    { id: "overview", label: "Overview" },
    { id: "files", label: "Files" },
    { id: "activity", label: "Activity" },
    { id: "advanced", label: "Advanced" },
  ];

  let outputs = $state<JobOutput[] | null>(null);
  let outputsError = $state<string | null>(null);
  let outputsLoading = false;
  let actions = $state<JobActionRecord[] | null>(null);
  let logs = $state<JobLogRecord[] | null>(null);
  let activityError = $state<string | null>(null);
  let activityLoading = false;
  let segments = $state<SegmentRecord[] | null>(null);
  let segmentsError = $state<string | null>(null);
  let segmentsLoading = false;
  const segmentSummaryText = $derived.by(() => {
    if (!segments) return null;
    if (segments.length === 0) return "No segment data for this download.";
    const byState = new Map<string, number>();
    for (const segment of segments) byState.set(segment.state, (byState.get(segment.state) ?? 0) + 1);
    return [...byState.entries()].map(([state, count]) => `${count} ${state}`).join(", ");
  });
  let actionBusy = $state(false);
  let trust = $state<TrustReport | null>(null);
  let trustError = $state<string | null>(null);
  let trustLoading = false;
  let tags = $state<string[] | null>(null);
  let tagsLoading = false;
  let tagsDraft = $state("");
  let tagsEditing = $state(false);
  let tagsBusy = $state(false);
  const trustPresentation = $derived(trust ? presentTrust(trust) : null);

  const service = $derived(connection.client ? new JobsService(connection.client) : null);

  let loadedForJobId: string | null = null;
  $effect(() => {
    if (jobId !== loadedForJobId) {
      outputs = null;
      outputsError = null;
      outputsLoading = false;
      actions = null;
      logs = null;
      activityError = null;
      activityLoading = false;
      segments = null;
      segmentsError = null;
      segmentsLoading = false;
      trust = null;
      trustError = null;
      trustLoading = false;
      tags = null;
      tagsLoading = false;
      tagsDraft = "";
      tagsEditing = false;
      tab = "overview";
      loadedForJobId = jobId;
    }

    if (!service || !jobId) return;

    if (tab === "overview" && tags === null && !tagsLoading) {
      tagsLoading = true;
      service
        .tags(jobId)
        .then((names) => (tags = names))
        .catch(() => (tags = []))
        .finally(() => (tagsLoading = false));
    }

    if (tab === "files" && outputs === null && !outputsLoading) {
      outputsLoading = true;
      service
        .outputs(jobId)
        .then((page) => (outputs = page.items))
        .catch((error) => (outputsError = describeError(error)))
        .finally(() => (outputsLoading = false));
    }

    if (tab === "activity" && actions === null && !activityLoading) {
      activityLoading = true;
      Promise.all([service.actions(jobId), service.logs(jobId, { limit: 50 })])
        .then(([actionsPage, logsPage]) => {
          actions = actionsPage.items;
          logs = logsPage.items;
        })
        .catch((error) => (activityError = describeError(error)))
        .finally(() => (activityLoading = false));
    }

    if (tab === "advanced") {
      if (trust === null && trustError === null && !trustLoading) {
        trustLoading = true;
        service
          .trust(jobId)
          .then((report) => (trust = report))
          .catch((error) => (trustError = describeError(error)))
          .finally(() => (trustLoading = false));
      }
      if (segments === null && segmentsError === null && !segmentsLoading) {
        segmentsLoading = true;
        service
          .segments(jobId)
          .then((page) => (segments = page.items))
          .catch((error) => (segmentsError = describeError(error)))
          .finally(() => (segmentsLoading = false));
      }
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
      const next = tagsDraft.split(",").map((tag) => tag.trim()).filter(Boolean);
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

  function retrySegments(): void {
    segmentsError = null;
    segments = null;
  }

  function segmentProgress(segment: SegmentRecord): number {
    const span = segment.end_byte - segment.start_byte + 1;
    if (span <= 0) return 0;
    return Math.min(100, Math.max(0, (segment.downloaded_bytes / span) * 100));
  }
</script>

<aside class="pane" aria-label="Download details">
  <header class="header">
    <div class="header-copy">
      <h2 title={title}>{title}</h2>
      {#if job}<p>{job.kind === "http" ? "Direct download" : job.kind === "media" ? "Media download" : "Torrent download"}</p>{/if}
    </div>
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
            <StatusBadge label={status.label} severity={status.severity} icon={status.icon} spinning={status.spinning} />
            <div class="job-actions" aria-label="Download actions">
              <Button variant="subtle" onclick={() => void runNativePathAction(job.destination, "open")}><Icon name="folder-open" size={14} /> Open folder</Button>
              {#if permitted?.pause}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("pause")}><Icon name="pause" size={14} /> Pause</Button>{/if}
              {#if permitted?.resume}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("resume")}><Icon name="play" size={14} /> Resume</Button>{/if}
              {#if permitted?.retry}<Button variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("retry")}><Icon name="refresh" size={14} /> Retry</Button>{/if}
              {#if permitted?.cancel}<IconButton icon="cancel" label="Cancel download" variant="subtle" disabled={actionBusy} onclick={() => void runJobAction("cancel")} />{/if}
            </div>
          </div>

          {#if job.error}<InlineError title="Last error" message={job.error} />{/if}

          {#if job.status === "downloading"}
            <div class="transfer-summary" aria-label="Transfer progress">
              <div><span>Progress</span><strong>{formatPercent(live?.downloadedBytes ?? job.downloaded_bytes, live?.totalBytes ?? job.total_bytes)}</strong></div>
              <div><span>Speed</span><strong>{formatSpeed(live?.bytesPerSecond ?? 0)}</strong></div>
              <div><span>ETA</span><strong>{formatEta(live?.downloadedBytes ?? job.downloaded_bytes, live?.totalBytes ?? job.total_bytes, live?.bytesPerSecond ?? 0)}</strong></div>
            </div>
          {/if}

          <dl>
            <dt>Source</dt><dd class="wrap">{job.source}</dd>
            <dt>Destination</dt><dd class="wrap">{job.destination}</dd>
            {#if job.filename}<dt>File name</dt><dd class="wrap">{job.filename}</dd>{/if}
            <dt>Size</dt><dd>{formatBytes(live?.totalBytes ?? job.total_bytes)}</dd>
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
                  {#if tags !== null}<IconButton icon="edit" label="Edit tags" variant="subtle" onclick={startTagEditing} />{/if}
                </span>
              {/if}
            </dd>
            <dt>Added</dt><dd>{formatAbsoluteTime(job.created_at)}</dd>
            {#if job.started_at}<dt>Started</dt><dd>{formatAbsoluteTime(job.started_at)}</dd>{/if}
            {#if job.completed_at}<dt>Completed</dt><dd>{formatAbsoluteTime(job.completed_at)}</dd>{/if}
          </dl>
        </section>
      {:else if tab === "files"}
        {#if outputsError}
          <InlineError title="Couldn't load files" message={outputsError} retry={retryOutputs} />
        {:else if outputs === null}
          <Skeleton height="80px" />
        {:else if outputs.length === 0}
          <p class="muted">No produced files yet.</p>
        {:else}
          <ul class="outputs">
            {#each outputs as output (output.id)}
              <li>
                <div class="output-row">
                  <Icon name={output.output_type === "video" ? "video" : output.output_type === "audio" ? "music" : output.output_type === "thumbnail" ? "image" : "file"} size={16} />
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
                <li><span>{action.action.type}</span><span class="muted">{action.state}{action.error ? ` — ${action.error}` : ""}</span></li>
              {/each}
            </ul>
          {/if}
          <h3 class="subheading">Recent log entries</h3>
          {#if !logs || logs.length === 0}
            <p class="muted">No log entries.</p>
          {:else}
            <ul class="logs">
              {#each logs as entry (entry.id)}
                <li class="log-entry {entry.severity}"><span class="log-time">{formatAbsoluteTime(entry.timestamp)}</span><span class="log-message">{entry.message}</span></li>
              {/each}
            </ul>
          {/if}
        {/if}
      {:else if tab === "advanced"}
        <section class="security-section">
          <h3 class="subheading">Source verification</h3>
          {#if trustError}
            <InlineError title="Couldn't load source verification" message={trustError} retry={retryTrust} />
          {:else if trust === null || trustPresentation === null}
            <Skeleton height="92px" />
          {:else}
            <div class="trust-summary" data-severity={trustPresentation.severity}>
              <Icon name="shield" size={20} />
              <div><strong>{trustPresentation.label}</strong><span>{trustPresentation.description}</span></div>
            </div>
            <ul class="trust-factors">
              {#each trust.factors as factor (factor.code)}
                <li class:satisfied={factor.satisfied}>
                  <Icon name={factor.satisfied ? "check-circle" : "info"} size={15} />
                  <div><span class="factor-label">{factor.label}</span><span class="factor-explanation">{factor.explanation}</span></div>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <AdvancedDisclosure title="Transfer details" description="Technical state and verification metadata">
          <dl class="technical-details">
            <dt>Download type</dt><dd>{job.kind}</dd>
            <dt>Priority</dt><dd>{job.priority}</dd>
            <dt>Transfer mode</dt><dd>{job.transfer_mode}</dd>
            {#if job.speed_limit_bps}<dt>Speed limit</dt><dd>{formatSpeed(job.speed_limit_bps)}</dd>{/if}
            {#if job.expected_sha256}<dt>Expected SHA-256</dt><dd class="wrap mono">{job.expected_sha256}</dd>{/if}
          </dl>

          <div class="segments-block">
            <h3 class="subheading">Segments</h3>
            {#if segmentsError}
              <InlineError title="Couldn't load segment data" message={segmentsError} retry={retrySegments} />
            {:else if segments === null}
              <Skeleton height="24px" />
            {:else if segments.length === 0}
              <p class="muted">No segment data for this download.</p>
            {:else}
              <div class="seg-lane" role="img" aria-label={`Segment progress: ${segmentSummaryText}`}>
                {#each segments as segment (segment.id)}
                  <div class="seg-track" title={`Segment ${segment.index} · ${segment.state} · ${Math.round(segmentProgress(segment))}%`}>
                    <span class="seg-fill" class:seg-done={segmentProgress(segment) >= 100} style:width={`${segmentProgress(segment)}%`}></span>
                  </div>
                {/each}
              </div>
              <p class="muted seg-summary">{segmentSummaryText}</p>
            {/if}
          </div>
        </AdvancedDisclosure>

        <AdvancedDisclosure title="Raw options" description="Backend request options for troubleshooting">
          <pre class="json">{JSON.stringify(job.options_json, null, 2)}</pre>
        </AdvancedDisclosure>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .pane { display: flex; flex-direction: column; width: 100%; height: 100%; min-width: 0; background: transparent; overflow: hidden; }
  .header { min-height: 58px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); background: var(--bg-layer-alt); }
  .header-copy { min-width: 0; }
  .header h2 { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-body-strong); font-weight: 600; }
  .header p { margin: 2px 0 0; color: var(--text-tertiary); font-size: var(--text-caption); }
  .loading, .content { padding: var(--space-4); }
  .content { flex: 1; overflow-y: auto; }
  .summary-header { display: flex; flex-direction: column; gap: var(--space-3); margin-bottom: var(--space-4); padding-bottom: var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .job-actions { display: flex; flex-wrap: wrap; gap: var(--space-1); }
  .job-actions :global(.button) { min-height: 28px; padding-inline: var(--space-2); font-size: var(--text-caption); }
  .transfer-summary { display: grid; grid-template-columns: repeat(3, 1fr); margin-bottom: var(--space-4); border-block: 1px solid var(--stroke-divider); }
  .transfer-summary div { min-width: 0; padding: var(--space-3); }
  .transfer-summary div + div { border-left: 1px solid var(--stroke-divider); }
  .transfer-summary span, .transfer-summary strong { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .transfer-summary span { color: var(--text-tertiary); font-size: var(--text-caption); }
  .transfer-summary strong { margin-top: 2px; font-size: var(--text-body); font-weight: 600; }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-1) var(--space-3); margin: 0 0 var(--space-4); font-size: var(--text-body); }
  dt { color: var(--text-secondary); }
  dd { margin: 0; }
  dd.wrap { word-break: break-all; }
  dd.mono { font-family: "Consolas", ui-monospace, monospace; font-size: var(--text-caption); }
  .muted { color: var(--text-secondary); }
  .subheading { margin: var(--space-4) 0 var(--space-2); color: var(--text-primary); font-size: var(--text-body); font-weight: 600; }
  .subheading:first-child { margin-top: 0; }
  .outputs, .actions, .logs, .trust-factors { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .outputs { gap: 0; }
  .outputs li { padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .output-row { display: flex; align-items: center; gap: var(--space-2); }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-caption); }
  .size, .output-meta { color: var(--text-tertiary); font-size: var(--text-caption); }
  .output-meta { display: block; padding-left: 24px; }
  .actions, .logs { gap: var(--space-2); }
  .actions li { display: flex; justify-content: space-between; gap: var(--space-2); font-size: var(--text-caption); }
  .log-entry { display: grid; grid-template-columns: 104px minmax(0, 1fr); gap: var(--space-2); font-size: var(--text-caption); }
  .log-entry.error { color: var(--status-error); }
  .log-entry.warn, .log-entry.warning { color: var(--status-warning); }
  .log-time { color: var(--text-tertiary); }
  .tag-row { display: inline-flex; align-items: center; gap: var(--space-1); }
  .tag-editor { display: flex; align-items: center; gap: var(--space-1); }
  .tag-input { flex: 1; min-width: 0; min-height: 28px; padding: 0 var(--space-2); border: 1px solid var(--stroke-divider); border-radius: var(--radius-control); background: var(--bg-subtle); color: var(--text-primary); font: inherit; font-size: var(--text-caption); }
  .security-section { margin-bottom: var(--space-3); }
  .trust-summary { display: flex; align-items: flex-start; gap: var(--space-3); padding: var(--space-3) 0; border-block: 1px solid var(--stroke-divider); }
  .trust-summary[data-severity="success"] { color: var(--status-success); }
  .trust-summary[data-severity="warning"] { color: var(--status-warning); }
  .trust-summary[data-severity="error"] { color: var(--status-error); }
  .trust-summary strong, .trust-summary span { display: block; }
  .trust-summary span { margin-top: 2px; color: var(--text-secondary); font-size: var(--text-caption); }
  .trust-factors { gap: var(--space-2); padding: var(--space-3) 0; }
  .trust-factors li { display: flex; align-items: flex-start; gap: var(--space-2); color: var(--text-tertiary); font-size: var(--text-caption); }
  .trust-factors li.satisfied { color: var(--text-primary); }
  .factor-label, .factor-explanation { display: block; }
  .factor-label { font-weight: 600; }
  .factor-explanation { color: var(--text-tertiary); }
  .technical-details { margin: var(--space-2) 0 0; }
  .segments-block { margin-top: var(--space-3); }
  .seg-lane { display: flex; gap: 2px; height: 14px; }
  .seg-track { flex: 1; min-width: 2px; height: 100%; overflow: hidden; border-radius: 2px; background: var(--bg-subtle); }
  .seg-fill { display: block; height: 100%; background: var(--accent-default); }
  .seg-fill.seg-done { background: var(--status-success); }
  .seg-summary { margin: var(--space-2) 0 0; font-family: var(--font-family-mono); font-size: var(--text-caption); }
  .json { margin: var(--space-2) 0 0; padding: var(--space-3); overflow-x: auto; border: 1px solid var(--stroke-divider); border-radius: var(--radius-control); background: var(--bg-subtle); white-space: pre-wrap; word-break: break-word; font-size: var(--text-caption); }
  @media (max-width: 420px) {
    .transfer-summary { grid-template-columns: 1fr; }
    .transfer-summary div + div { border-left: 0; border-top: 1px solid var(--stroke-divider); }
    .log-entry { grid-template-columns: 1fr; }
  }
</style>
