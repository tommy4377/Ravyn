<script lang="ts">
  import { describeError } from "../api/errors";
  import type { Job, MediaArchiveRecord, MediaItemOutputRecord, MediaItemRecord, MediaItemSummary } from "../api/types";
  import Button from "../components/Button.svelte";
  import CompactSummary, { type SummaryItem } from "../components/CompactSummary.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import DetailsPane from "../components/DetailsPane.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import ListDetailsLayout from "../components/ListDetailsLayout.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import type { MenuItem } from "../components/Menu.svelte";
  import PageCommandBar from "../components/PageCommandBar.svelte";
  import PageScaffold from "../components/PageScaffold.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import Tabs from "../components/Tabs.svelte";
  import { openNativePath, revealNativePath } from "../native/tauri";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes, jobDisplayName } from "../util/format";
  import { mediaActivity, mediaProgress, uniqueProducedFiles, type MediaDetailTab } from "./mediaPresentation";

  type MediaTab = "downloads" | "archive";

  let activeTab = $state<MediaTab>("downloads");
  let detailTab = $state<MediaDetailTab>("overview");
  let jobs = $state<Job[]>([]);
  let archive = $state<MediaArchiveRecord[]>([]);
  let search = $state("");
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selectedJobId = $state<string | null>(null);
  let summary = $state<MediaItemSummary | null>(null);
  let items = $state<MediaItemRecord[]>([]);
  let detailsLoading = $state(false);
  let detailsError = $state<string | null>(null);
  let retryBusy = $state<string | null>(null);
  let retryAllBusy = $state(false);

  let producedFiles = $state<MediaItemOutputRecord[]>([]);
  let outputsLoading = $state(false);
  let outputsError = $state<string | null>(null);
  let outputsForJob = $state<string | null>(null);

  let archiveTarget = $state<MediaArchiveRecord | null>(null);
  let archiveBusy = $state(false);
  let archiveError = $state<string | null>(null);

  // jobs holds the fetch identity/order; displayJobs merges in live
  // status+progress from jobsStore (kept current by AppShell's global SSE
  // subscription — see load() below) so status badges and progress bars
  // don't freeze at whatever they were when the page loaded or a job was
  // last selected.
  const displayJobs = $derived(
    jobs.map((job) => {
      const live = jobsStore.byId.get(job.id);
      if (!live) return job;
      const liveProgress = jobsStore.liveProgress.get(job.id);
      return {
        ...live,
        downloaded_bytes: liveProgress?.downloadedBytes ?? live.downloaded_bytes,
        total_bytes: liveProgress?.totalBytes ?? live.total_bytes,
      };
    }),
  );
  const selectedJob = $derived(displayJobs.find((job) => job.id === selectedJobId) ?? null);
  const visibleJobs = $derived(search.trim()
    ? displayJobs.filter((job) => `${job.filename ?? ""} ${job.source} ${job.status}`.toLowerCase().includes(search.toLowerCase()))
    : displayJobs);
  const visibleArchive = $derived(search.trim()
    ? archive.filter((entry) => `${entry.extractor} ${entry.media_id} ${entry.webpage_url ?? ""}`.toLowerCase().includes(search.toLowerCase()))
    : archive);
  const completedJobs = $derived(displayJobs.filter((job) => job.status === "completed").length);
  const activeJobs = $derived(displayJobs.filter((job) => ["queued", "probing", "downloading", "post_processing"].includes(job.status)).length);
  const failedJobs = $derived(displayJobs.filter((job) => job.status === "failed").length);
  const failedItems = $derived(items.filter((item) => item.state === "failed").length);
  const activity = $derived(mediaActivity(items));
  const summaryItems = $derived<SummaryItem[]>([
    { label: "media jobs", value: jobs.length.toLocaleString() },
    { label: "active", value: activeJobs.toLocaleString(), tone: activeJobs ? "success" : "default" },
    { label: "completed", value: completedJobs.toLocaleString() },
    { label: "failed", value: failedJobs.toLocaleString(), tone: failedJobs ? "error" : "default" },
    { label: "archive entries", value: archive.length.toLocaleString() },
  ]);

  const viewTabs = [
    { id: "downloads", label: "Downloads" },
    { id: "archive", label: "Archive" },
  ];
  const detailTabs = [
    { id: "overview", label: "Overview" },
    { id: "items", label: "Items" },
    { id: "files", label: "Produced files" },
    { id: "activity", label: "Activity" },
  ];

  function statusSeverity(status: string): "neutral" | "info" | "success" | "warning" | "error" {
    if (status === "completed") return "success";
    if (status === "failed") return "error";
    if (status === "partial" || status === "paused" || status === "skipped") return "warning";
    if (["queued", "probing", "downloading", "post_processing", "planned"].includes(status)) return "info";
    return "neutral";
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const [jobPage, archivePage] = await Promise.all([
        connection.client.listJobs({ kind: "media", limit: 250 }),
        connection.client.listMediaArchive({ limit: 250 }),
      ]);
      jobs = jobPage.items;
      for (const job of jobPage.items) jobsStore.upsert(job);
      archive = archivePage.items;
      if (selectedJobId && !jobs.some((job) => job.id === selectedJobId)) selectedJobId = null;
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  async function loadDetails(jobId: string): Promise<void> {
    if (!connection.client) return;
    detailsLoading = true;
    detailsError = null;
    try {
      const [nextSummary, page] = await Promise.all([
        connection.client.getMediaSummary(jobId),
        connection.client.listMediaItems(jobId, { limit: 500 }),
      ]);
      if (selectedJobId !== jobId) return;
      summary = nextSummary;
      items = page.items;
    } catch (cause) {
      if (selectedJobId === jobId) detailsError = describeError(cause);
    } finally {
      if (selectedJobId === jobId) detailsLoading = false;
    }
  }

  async function loadProducedFiles(jobId: string): Promise<void> {
    if (!connection.client || outputsLoading) return;
    outputsLoading = true;
    outputsError = null;
    const records: MediaItemOutputRecord[] = [];
    try {
      const currentItems = [...items];
      for (let offset = 0; offset < currentItems.length; offset += 8) {
        const chunk = currentItems.slice(offset, offset + 8);
        const chunkOutputs = await Promise.all(
          chunk.map((item) => connection.client!.listMediaItemOutputs(jobId, item.id).catch(() => [])),
        );
        if (selectedJobId !== jobId) return;
        records.push(...chunkOutputs.flat());
      }
      producedFiles = uniqueProducedFiles(records);
      outputsForJob = jobId;
    } catch (cause) {
      if (selectedJobId === jobId) outputsError = describeError(cause);
    } finally {
      if (selectedJobId === jobId) outputsLoading = false;
    }
  }

  $effect(() => {
    void load();
    // Top-level job status/progress comes live via jobsStore (see load()
    // above), but the per-item playlist detail (`items`, fetched once by
    // loadDetails on selection) has no SSE channel and would otherwise
    // freeze mid-playlist-download. Poll while a job is selected.
    const timer = window.setInterval(() => void load(), 5_000);
    return () => window.clearInterval(timer);
  });

  $effect(() => {
    const jobId = selectedJobId;
    if (!jobId) return;
    const active = ["queued", "probing", "downloading", "post_processing"].includes(
      selectedJob?.status ?? "",
    );
    if (!active) return;
    const timer = window.setInterval(() => void loadDetails(jobId), 5_000);
    return () => window.clearInterval(timer);
  });

  $effect(() => {
    producedFiles = [];
    outputsForJob = null;
    outputsError = null;
    detailTab = "overview";
    if (!selectedJobId) {
      summary = null;
      items = [];
      detailsError = null;
      return;
    }
    void loadDetails(selectedJobId);
  });

  $effect(() => {
    detailTab;
    items.length;
    if (detailTab === "files" && selectedJobId && outputsForJob !== selectedJobId && !detailsLoading) {
      void loadProducedFiles(selectedJobId);
    }
  });

  $effect(() => {
    activeTab;
    if (activeTab === "archive") selectedJobId = null;
  });

  async function runNativePathAction(path: string, action: "open" | "reveal"): Promise<void> {
    try {
      if (action === "open") await openNativePath(path);
      else await revealNativePath(path);
    } catch (cause) {
      notifications.error(action === "open" ? "Couldn't open this file" : "Couldn't show this file in Explorer", describeError(cause));
    }
  }

  async function retryItem(item: MediaItemRecord): Promise<void> {
    if (!connection.client || retryBusy) return;
    retryBusy = item.id;
    try {
      await connection.client.retryMediaItem(item.job_id, item.id);
      notifications.success("Media item queued again", item.title ?? item.item_key);
      await Promise.all([load(), loadDetails(item.job_id)]);
    } catch (cause) {
      notifications.error("Couldn't retry media item", describeError(cause));
    } finally {
      retryBusy = null;
    }
  }

  async function retryFailed(): Promise<void> {
    if (!connection.client || !selectedJobId || retryAllBusy) return;
    retryAllBusy = true;
    try {
      const result = await connection.client.retryFailedMediaItems(selectedJobId, 500);
      if (result.failed > 0) notifications.error(`${result.failed} media items could not be retried`);
      else notifications.success(`${result.accepted} media item${result.accepted === 1 ? "" : "s"} queued again`);
      await Promise.all([load(), loadDetails(selectedJobId)]);
    } catch (cause) {
      notifications.error("Couldn't retry failed media items", describeError(cause));
    } finally {
      retryAllBusy = false;
    }
  }

  async function removeArchiveEntry(): Promise<void> {
    if (!connection.client || !archiveTarget) return;
    archiveBusy = true;
    archiveError = null;
    try {
      await connection.client.removeMediaArchive(archiveTarget.extractor, archiveTarget.media_id);
      notifications.info("Archive entry removed", "The downloaded file was not deleted.");
      archiveTarget = null;
      await load();
    } catch (cause) {
      archiveError = describeError(cause);
    } finally {
      archiveBusy = false;
    }
  }

  function moreItems(): MenuItem[] {
    return [
      { id: "refresh", label: "Refresh", icon: "refresh", onSelect: () => void load() },
    ];
  }
</script>

<PageScaffold title="Media" summary="Video, audio, playlists, produced files, and duplicate prevention.">
  {#snippet actions()}
    <Button variant="accent" onclick={() => navigation.requestAdd("media")}><Icon name="add" size={16} /> Add media</Button>
  {/snippet}

  {#snippet commandBar()}
    <PageCommandBar ariaLabel="Media commands">
      {#snippet leading()}
        <Tabs tabs={viewTabs} bind:selected={activeTab} />
      {/snippet}
      {#snippet actions()}
        <SearchBox bind:value={search} label="Search media" placeholder={activeTab === "downloads" ? "Search media downloads" : "Search archive"} />
        <MenuButton label="More" icon="more" items={moreItems()} variant="subtle" />
      {/snippet}
    </PageCommandBar>
  {/snippet}

  {#snippet status()}
    <div class="status-strip"><CompactSummary items={summaryItems} ariaLabel="Media summary" /></div>
  {/snippet}

  <div class="workspace">
    <ListDetailsLayout detailsOpen={activeTab === "downloads" && !!selectedJob} detailsLabel="Media download details" detailsWidth="450px">
      {#snippet list()}
        <Surface padding="none" class="media-list">
          {#if error}
            <div class="state"><InlineError title="Couldn't load media" message={error} retry={() => void load()} /></div>
          {:else if loading}
            <div class="state muted">Loading media history…</div>
          {:else if activeTab === "downloads"}
            {#if visibleJobs.length === 0}
              <EmptyState icon="video" title="No media downloads" message={search ? "No media jobs match the current search." : "Add a video, audio item, or playlist to begin."}>
                {#if !search}<Button variant="accent" onclick={() => navigation.requestAdd("media")}>Add media</Button>{/if}
              </EmptyState>
            {:else}
              <div class="list-header" aria-hidden="true"><span>Name</span><span>Progress</span><span>Size</span><span>Status</span></div>
              <div class="rows" role="listbox" aria-label="Media downloads">
                {#each visibleJobs as job (job.id)}
                  {@const progress = mediaProgress(job)}
                  <button type="button" class="job-row" class:selected={selectedJobId === job.id} role="option" aria-selected={selectedJobId === job.id} onclick={() => (selectedJobId = job.id)}>
                    <span class="name-cell"><span class="media-icon"><Icon name="video" size={18} /></span><span><strong>{jobDisplayName(job.source, job.filename)}</strong><small>{job.source}</small></span></span>
                    <span class="progress-cell"><span class="progress-track"><span style={`width:${progress}%`}></span></span><small>{progress.toFixed(0)}% · {formatBytes(job.downloaded_bytes)}</small></span>
                    <span>{formatBytes(job.total_bytes)}</span>
                    <span><StatusBadge label={job.status.replaceAll("_", " ")} severity={statusSeverity(job.status)} /></span>
                  </button>
                {/each}
              </div>
            {/if}
          {:else if visibleArchive.length === 0}
            <EmptyState icon="archive" title="Media archive is empty" message={search ? "No archive records match the current search." : "Completed media downloads are recorded here to prevent accidental duplicates."} />
          {:else}
            <div class="archive-header" aria-hidden="true"><span>Media</span><span>Extractor</span><span>Downloaded</span><span></span></div>
            <div class="rows" aria-label="Media archive">
              {#each visibleArchive as entry (`${entry.extractor}:${entry.media_id}`)}
                <div class="archive-row">
                  <span><strong>{entry.media_id}</strong><small>{entry.webpage_url ?? "No source URL stored"}</small></span>
                  <span>{entry.extractor}</span>
                  <span>{formatAbsoluteTime(entry.downloaded_at)}</span>
                  <IconButton icon="trash" label="Remove archive entry" variant="subtle" onclick={() => { archiveTarget = entry; archiveError = null; }} />
                </div>
              {/each}
            </div>
          {/if}
        </Surface>
      {/snippet}

      {#snippet details()}
        {#if selectedJob}
          <DetailsPane
            title={jobDisplayName(selectedJob.source, selectedJob.filename)}
            subtitle={summary?.playlist_title ?? selectedJob.status.replaceAll("_", " ")}
            icon="video"
            tabs={detailTabs}
            bind:selectedTab={detailTab}
            onClose={() => (selectedJobId = null)}
          >
            {#if detailsError}
              <InlineError title="Couldn't load media details" message={detailsError} retry={() => selectedJobId && void loadDetails(selectedJobId)} />
            {:else if detailsLoading}
              <p class="muted">Loading media details…</p>
            {:else if detailTab === "overview"}
              <div class="detail-stack">
                <div class="detail-summary">
                  <span><strong>{summary?.completed ?? 0}</strong> completed</span>
                  <span><strong>{summary?.downloading ?? 0}</strong> downloading</span>
                  <span class:has-error={(summary?.failed ?? 0) > 0}><strong>{summary?.failed ?? 0}</strong> failed</span>
                  <span><strong>{summary?.skipped ?? 0}</strong> skipped</span>
                </div>
                {#if (summary?.failed ?? 0) > 0}
                  <Button variant="accent" disabled={retryAllBusy} onclick={() => void retryFailed()}><Icon name="refresh" size={16} /> {retryAllBusy ? "Retrying…" : "Retry all failed"}</Button>
                {/if}
                <dl>
                  <dt>Status</dt><dd>{selectedJob.status.replaceAll("_", " ")}</dd>
                  <dt>Source</dt><dd class="wrap">{selectedJob.source}</dd>
                  <dt>Destination</dt><dd class="wrap mono">{selectedJob.destination}</dd>
                  <dt>Downloaded</dt><dd>{formatBytes(selectedJob.downloaded_bytes)}</dd>
                  <dt>Total size</dt><dd>{formatBytes(selectedJob.total_bytes)}</dd>
                  <dt>Created</dt><dd>{formatAbsoluteTime(selectedJob.created_at)}</dd>
                  {#if selectedJob.completed_at}<dt>Completed</dt><dd>{formatAbsoluteTime(selectedJob.completed_at)}</dd>{/if}
                  {#if summary?.playlist_title}<dt>Playlist</dt><dd>{summary.playlist_title}</dd>{/if}
                  {#if summary?.declared_playlist_count}<dt>Declared items</dt><dd>{summary.declared_playlist_count}</dd>{/if}
                </dl>
              </div>
            {:else if detailTab === "items"}
              {#if items.length === 0}
                <EmptyState icon="list" title="No media item records" message="Single-file downloads may not expose an item list until processing begins." />
              {:else}
                <div class="item-list">
                  {#each items as item (item.id)}
                    <div class="item-row">
                      <span class="item-state" data-tone={statusSeverity(item.state)}><Icon name={item.state === "completed" ? "check-circle" : item.state === "failed" ? "alert-circle" : "video"} size={16} /></span>
                      <span class="item-copy"><strong>{item.title ?? item.item_key}</strong><small>{item.playlist_index ? `${item.playlist_index}${item.playlist_count ? ` of ${item.playlist_count}` : ""}` : item.output_path ?? item.webpage_url ?? item.state}</small>{#if item.error}<em>{item.error}</em>{/if}</span>
                      <StatusBadge label={item.state} severity={statusSeverity(item.state)} />
                      {#if item.state === "failed"}<IconButton icon="refresh" label="Retry item" variant="subtle" disabled={retryBusy === item.id} onclick={() => void retryItem(item)} />{/if}
                    </div>
                  {/each}
                </div>
              {/if}
            {:else if detailTab === "files"}
              {#if outputsError}
                <InlineError title="Couldn't load produced files" message={outputsError} retry={() => selectedJobId && void loadProducedFiles(selectedJobId)} />
              {:else if outputsLoading || outputsForJob !== selectedJob.id}
                <p class="muted">Collecting produced files…</p>
              {:else if producedFiles.length === 0}
                <EmptyState icon="file" title="No produced files recorded" message="Files will appear here after media items finish processing." />
              {:else}
                <div class="output-list">
                  {#each producedFiles as record (record.output.id)}
                    <div class="output-row">
                      <Icon name={record.output.output_type === "audio" ? "music" : record.output.output_type === "thumbnail" ? "image" : "file"} size={17} />
                      <span><strong>{record.output.relative_path}</strong><small>{record.role} · {formatBytes(record.output.size_bytes)}</small></span>
                      <IconButton icon="external-link" label="Open file" variant="subtle" onclick={() => void runNativePathAction(record.output.current_path, "open")} />
                      <IconButton icon="folder-open" label="Show in Explorer" variant="subtle" onclick={() => void runNativePathAction(record.output.current_path, "reveal")} />
                    </div>
                  {/each}
                </div>
              {/if}
            {:else}
              {#if activity.length === 0}
                <EmptyState icon="clock" title="No activity recorded" message="Media item state changes will appear here." />
              {:else}
                <div class="timeline">
                  {#each activity as item (item.id)}
                    <div class="timeline-entry" data-tone={statusSeverity(item.state)}>
                      <span class="timeline-dot"></span>
                      <div><strong>{item.title ?? item.item_key}</strong><small>{item.state.replaceAll("_", " ")} · {formatAbsoluteTime(item.updated_at)}</small>{#if item.error}<p>{item.error}</p>{/if}</div>
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}
          </DetailsPane>
        {/if}
      {/snippet}
    </ListDetailsLayout>
  </div>
</PageScaffold>

<ConfirmDialog
  open={!!archiveTarget}
  title="Remove archive entry?"
  message="Ravyn may allow this media item to be downloaded again. No downloaded file is deleted."
  confirmLabel="Remove entry"
  destructive
  busy={archiveBusy}
  error={archiveError}
  onConfirm={() => void removeArchiveEntry()}
  onClose={() => !archiveBusy && (archiveTarget = null)}
/>

<style>
  .workspace { height: 100%; min-height: 0; padding: 0 var(--page-padding) var(--page-padding); }
  .status-strip { min-height: 38px; display: flex; align-items: center; padding: 0 var(--page-padding); border-bottom: 1px solid var(--stroke-divider); }
  :global(.media-list) { height: 100%; min-height: 0; display: flex; flex-direction: column; border-radius: 0; border-color: var(--stroke-divider); background: var(--surface-content); }
  .state { padding: var(--space-6); }
  .muted { color: var(--text-secondary); }
  .list-header, .job-row { display: grid; grid-template-columns: minmax(250px, 1.8fr) minmax(170px, 1fr) 100px 130px; align-items: center; gap: var(--space-3); }
  .archive-header, .archive-row { display: grid; grid-template-columns: minmax(250px, 1.8fr) 120px 180px 36px; align-items: center; gap: var(--space-3); }
  .list-header, .archive-header { min-height: 36px; padding: 0 var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .rows { flex: 1; min-height: 0; overflow: auto; }
  .job-row { width: 100%; min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); color: var(--text-primary); background: transparent; text-align: left; }
  .job-row:hover, .archive-row:hover { background: var(--bg-subtle-hover); }
  .job-row.selected { background: color-mix(in srgb, var(--accent-subtle) 52%, transparent); box-shadow: inset 2px 0 var(--accent-default); }
  .name-cell { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .media-icon { width: 30px; height: 30px; flex: none; display: grid; place-items: center; color: var(--text-secondary); }
  .name-cell > span:last-child, .archive-row > span:first-child { min-width: 0; display: flex; flex-direction: column; }
  .name-cell strong, .name-cell small, .archive-row strong, .archive-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name-cell small, .archive-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .progress-cell { display: flex; flex-direction: column; gap: 4px; }
  .progress-cell small { color: var(--text-tertiary); }
  .progress-track { height: 4px; overflow: hidden; border-radius: var(--radius-pill); background: var(--bg-subtle); }
  .progress-track span { display: block; height: 100%; border-radius: inherit; background: var(--accent-default); }
  .archive-row { min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .detail-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .detail-summary { display: flex; flex-wrap: wrap; gap: var(--space-2) var(--space-4); padding-bottom: var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-secondary); font-size: var(--text-caption); }
  .detail-summary span { display: inline-flex; gap: 4px; }
  .detail-summary strong { color: var(--text-primary); }
  .detail-summary .has-error strong { color: var(--status-error); }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-4); margin: 0; }
  dt { color: var(--text-secondary); }
  dd { min-width: 0; margin: 0; }
  .wrap { word-break: break-word; }
  .mono { font: 12px/18px Consolas, ui-monospace, monospace; }
  .item-list, .output-list, .timeline { display: flex; flex-direction: column; }
  .item-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); min-height: 56px; padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .item-copy, .output-row > span { min-width: 0; display: flex; flex-direction: column; }
  .item-copy strong, .item-copy small, .output-row strong, .output-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item-copy small, .output-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .item-copy em { color: var(--status-error); font-size: var(--text-caption); font-style: normal; white-space: normal; }
  .item-state { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-tertiary); }
  .item-state[data-tone="success"] { color: var(--status-success); }
  .item-state[data-tone="error"] { color: var(--status-error); }
  .item-state[data-tone="warning"] { color: var(--status-warning); }
  .output-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-2); min-height: 52px; border-bottom: 1px solid var(--stroke-divider); }
  .timeline-entry { position: relative; display: grid; grid-template-columns: 18px minmax(0, 1fr); gap: var(--space-2); padding: 0 0 var(--space-4); }
  .timeline-entry::before { content: ""; position: absolute; left: 5px; top: 12px; bottom: -2px; width: 1px; background: var(--stroke-divider); }
  .timeline-entry:last-child::before { display: none; }
  .timeline-dot { width: 11px; height: 11px; margin-top: 4px; border: 2px solid var(--surface-content); border-radius: 50%; background: var(--text-tertiary); box-shadow: 0 0 0 1px var(--stroke-control); }
  .timeline-entry[data-tone="success"] .timeline-dot { background: var(--status-success); }
  .timeline-entry[data-tone="error"] .timeline-dot { background: var(--status-error); }
  .timeline-entry[data-tone="warning"] .timeline-dot { background: var(--status-warning); }
  .timeline-entry div { min-width: 0; display: flex; flex-direction: column; }
  .timeline-entry small { color: var(--text-tertiary); }
  .timeline-entry p { margin: var(--space-1) 0 0; color: var(--status-error); font-size: var(--text-caption); }
  @media (max-width: 1120px) {
    .list-header, .job-row { grid-template-columns: minmax(220px, 1.5fr) minmax(150px, 1fr) 120px; }
    .list-header span:nth-child(3), .job-row > span:nth-child(3) { display: none; }
  }
  @media (max-width: 720px) {
    .archive-header, .archive-row { grid-template-columns: minmax(0, 1fr) 36px; }
    .archive-header span:nth-child(2), .archive-header span:nth-child(3), .archive-row > span:nth-child(2), .archive-row > span:nth-child(3) { display: none; }
    .list-header { display: none; }
    .job-row { grid-template-columns: minmax(0, 1fr) auto; }
    .job-row > span:nth-child(2), .job-row > span:nth-child(3) { display: none; }
    .item-row { grid-template-columns: auto minmax(0, 1fr) auto; }
    .item-row > :global(button) { grid-column: 3; }
  }
</style>
