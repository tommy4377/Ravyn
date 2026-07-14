<script lang="ts">
  import { describeError } from "../api/errors";
  import type { Job, MediaArchiveRecord, MediaItemRecord, MediaItemSummary } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import MetricCard from "../components/MetricCard.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import { connection } from "../stores/connection.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes, formatPercent, jobDisplayName } from "../util/format";

  type MediaTab = "downloads" | "archive";

  let activeTab = $state<MediaTab>("downloads");
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
  let archiveTarget = $state<MediaArchiveRecord | null>(null);
  let archiveBusy = $state(false);
  let archiveError = $state<string | null>(null);

  const visibleJobs = $derived(search.trim()
    ? jobs.filter((job) => `${job.filename ?? ""} ${job.source} ${job.status}`.toLowerCase().includes(search.toLowerCase()))
    : jobs);
  const visibleArchive = $derived(search.trim()
    ? archive.filter((entry) => `${entry.extractor} ${entry.media_id} ${entry.webpage_url ?? ""}`.toLowerCase().includes(search.toLowerCase()))
    : archive);
  const selectedJob = $derived(jobs.find((job) => job.id === selectedJobId) ?? null);
  const completedJobs = $derived(jobs.filter((job) => job.status === "completed").length);
  const failedItems = $derived(items.filter((item) => item.state === "failed").length);

  function statusSeverity(status: string): "neutral" | "info" | "success" | "warning" | "error" {
    if (status === "completed") return "success";
    if (status === "failed") return "error";
    if (status === "partial" || status === "paused") return "warning";
    if (["queued", "probing", "downloading", "post_processing"].includes(status)) return "info";
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

  $effect(() => { void load(); });
  $effect(() => {
    if (!selectedJobId) {
      summary = null;
      items = [];
      detailsError = null;
      return;
    }
    void loadDetails(selectedJobId);
  });

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
      notifications.info("Archive entry removed");
      archiveTarget = null;
      await load();
    } catch (cause) {
      archiveError = describeError(cause);
    } finally {
      archiveBusy = false;
    }
  }
</script>

<div class="page">
  <PageHeader eyebrow="Downloads" title="Media" description="Video, audio, playlists, item history, and the anti-duplicate archive.">
    {#snippet actions()}
      <Button onclick={() => void load()}><Icon name="refresh" size={16} /> Refresh</Button>
      <Button variant="accent" onclick={() => navigation.requestAdd("media")}><Icon name="add" size={16} /> Add media</Button>
    {/snippet}
  </PageHeader>

  <div class="metrics">
    <MetricCard label="Media downloads" value={jobs.length.toLocaleString()} detail={`${completedJobs} completed`} icon="video" />
    <MetricCard label="Archive entries" value={archive.length.toLocaleString()} detail="Used to prevent duplicate downloads" icon="archive" />
    <MetricCard label="Selected items" value={(summary?.total ?? 0).toLocaleString()} detail={selectedJob ? jobDisplayName(selectedJob.source, selectedJob.filename) : "Select a media job"} icon="list" />
    <MetricCard label="Failed items" value={failedItems.toLocaleString()} detail={failedItems > 0 ? "Can be retried individually or together" : "No failed selected items"} icon="warning" />
  </div>

  <div class="command-row">
    <div class="tabs" aria-label="Media view">
      <button type="button" class:active={activeTab === "downloads"} onclick={() => (activeTab = "downloads")}>Downloads</button>
      <button type="button" class:active={activeTab === "archive"} onclick={() => { activeTab = "archive"; selectedJobId = null; }}>Archive</button>
    </div>
    <SearchBox bind:value={search} label="Search media" placeholder={activeTab === "downloads" ? "Search media downloads" : "Search archive"} />
  </div>

  <div class="workspace" class:with-details={activeTab === "downloads" && !!selectedJob}>
    <Surface padding="none" class="main-list">
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
          <div class="rows">
            {#each visibleJobs as job (job.id)}
              <button type="button" class="job-row" class:selected={selectedJobId === job.id} onclick={() => (selectedJobId = job.id)}>
                <span class="name-cell"><span class="media-icon"><Icon name="video" size={18} /></span><span><strong>{jobDisplayName(job.source, job.filename)}</strong><small>{job.source}</small></span></span>
                <span class="progress-cell"><span class="progress-track"><span style={`width:${job.total_bytes ? Math.min(100, job.downloaded_bytes / job.total_bytes * 100) : 0}%`}></span></span><small>{formatPercent(job.downloaded_bytes, job.total_bytes)}</small></span>
                <span>{formatBytes(job.total_bytes)}</span>
                <span><StatusBadge label={job.status.replaceAll("_", " ")} severity={statusSeverity(job.status)} /></span>
              </button>
            {/each}
          </div>
        {/if}
      {:else if visibleArchive.length === 0}
        <EmptyState icon="archive" title="Media archive is empty" message={search ? "No archive records match the current search." : "Completed media downloads are recorded here to prevent accidental duplicates."} />
      {:else}
        <div class="archive-header" aria-hidden="true"><span>Media ID</span><span>Extractor</span><span>Downloaded</span><span></span></div>
        <div class="rows">
          {#each visibleArchive as entry (`${entry.extractor}:${entry.media_id}`)}
            <div class="archive-row"><span><strong>{entry.media_id}</strong><small>{entry.webpage_url ?? "No source URL stored"}</small></span><span>{entry.extractor}</span><span>{formatAbsoluteTime(entry.downloaded_at)}</span><IconButton icon="trash" label="Remove archive entry" variant="subtle" onclick={() => { archiveTarget = entry; archiveError = null; }} /></div>
          {/each}
        </div>
      {/if}
    </Surface>

    {#if activeTab === "downloads" && selectedJob}
      <aside class="details">
        <header><div><span class="media-icon large"><Icon name="video" size={21} /></span><span><h2>{jobDisplayName(selectedJob.source, selectedJob.filename)}</h2><small>{summary?.playlist_title ?? selectedJob.status.replaceAll("_", " ")}</small></span></div><IconButton icon="close" label="Close details" variant="subtle" onclick={() => (selectedJobId = null)} /></header>
        <div class="details-body">
          {#if detailsError}
            <InlineError title="Couldn't load media items" message={detailsError} retry={() => selectedJobId && void loadDetails(selectedJobId)} />
          {:else if detailsLoading}
            <p class="muted">Loading media items…</p>
          {:else}
            <div class="summary-grid">
              <div><strong>{summary?.completed ?? 0}</strong><small>Completed</small></div>
              <div><strong>{summary?.downloading ?? 0}</strong><small>Downloading</small></div>
              <div><strong>{summary?.failed ?? 0}</strong><small>Failed</small></div>
              <div><strong>{summary?.skipped ?? 0}</strong><small>Skipped</small></div>
            </div>
            {#if (summary?.failed ?? 0) > 0}<Button variant="accent" disabled={retryAllBusy} onclick={() => void retryFailed()}><Icon name="refresh" size={16} /> {retryAllBusy ? "Retrying…" : "Retry all failed"}</Button>{/if}
            <div class="item-list">
              {#each items as item (item.id)}
                <div class="item-row"><span class="item-state {statusSeverity(item.state)}"><Icon name={item.state === "completed" ? "check-circle" : item.state === "failed" ? "alert-circle" : "video"} size={16} /></span><span><strong>{item.title ?? item.item_key}</strong><small>{item.playlist_index ? `${item.playlist_index}${item.playlist_count ? ` of ${item.playlist_count}` : ""}` : item.output_path ?? item.webpage_url ?? item.state}</small>{#if item.error}<em>{item.error}</em>{/if}</span><StatusBadge label={item.state} severity={statusSeverity(item.state)} />{#if item.state === "failed"}<IconButton icon="refresh" label="Retry item" variant="subtle" disabled={retryBusy === item.id} onclick={() => void retryItem(item)} />{/if}</div>
              {/each}
              {#if items.length === 0}<EmptyState icon="list" title="No media item records" message="Single-file downloads may not expose a playlist item list until processing begins." />{/if}
            </div>
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</div>

<ConfirmDialog open={!!archiveTarget} title="Remove archive entry?" message="Ravyn may allow this media item to be downloaded again after the archive record is removed. No downloaded file is deleted." confirmLabel="Remove entry" destructive busy={archiveBusy} error={archiveError} onConfirm={() => void removeArchiveEntry()} onClose={() => !archiveBusy && (archiveTarget = null)} />

<style>
  .page { height: 100%; display: flex; flex-direction: column; min-width: 0; }
  .metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-3); padding: 0 var(--page-padding) var(--space-4); }
  .command-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: 0 var(--page-padding) var(--space-4); }
  .tabs { display: inline-flex; padding: 3px; border: 1px solid var(--stroke-control); border-radius: var(--radius-medium); background: var(--bg-control); }
  .tabs button { min-height: 30px; padding: 0 var(--space-3); border: 0; border-radius: calc(var(--radius-medium) - 2px); color: var(--text-secondary); background: transparent; }
  .tabs button.active { color: var(--text-primary); background: var(--surface-card); box-shadow: var(--shadow-control); font-weight: 600; }
  .command-row :global(.search-box) { width: min(440px, 48vw); }
  .workspace { position: relative; display: grid; grid-template-columns: minmax(0, 1fr); flex: 1; min-height: 0; gap: var(--space-3); padding: 0 var(--page-padding) var(--page-padding); }
  .workspace.with-details { grid-template-columns: minmax(0, 1fr) minmax(360px, 420px); }
  :global(.main-list) { display: flex; flex-direction: column; min-height: 0; }
  .state { padding: var(--space-6); }
  .muted { color: var(--text-secondary); }
  .list-header, .job-row { display: grid; grid-template-columns: minmax(240px, 1.7fr) minmax(150px, 1fr) 100px 120px; align-items: center; gap: var(--space-3); }
  .archive-header, .archive-row { display: grid; grid-template-columns: minmax(240px, 1.7fr) 120px 180px 36px; align-items: center; gap: var(--space-3); }
  .list-header, .archive-header { min-height: 36px; padding: 0 var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .rows { flex: 1; min-height: 0; overflow: auto; }
  .job-row { width: 100%; min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); color: var(--text-primary); background: transparent; text-align: left; }
  .job-row:hover, .archive-row:hover { background: var(--bg-subtle-hover); }
  .job-row.selected { background: var(--accent-subtle); box-shadow: inset 3px 0 var(--accent-default); }
  .name-cell { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .name-cell > span:last-child, .archive-row > span:first-child { display: flex; min-width: 0; flex-direction: column; }
  .name-cell strong, .name-cell small, .archive-row strong, .archive-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name-cell small, .archive-row small, .details header small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .media-icon { display: grid; place-items: center; width: 34px; height: 34px; flex: none; border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  .media-icon.large { width: 40px; height: 40px; }
  .progress-cell { display: flex; flex-direction: column; gap: 4px; }
  .progress-cell small { color: var(--text-tertiary); }
  .progress-track { height: 4px; overflow: hidden; border-radius: var(--radius-pill); background: var(--bg-subtle); }
  .progress-track span { display: block; height: 100%; border-radius: inherit; background: var(--accent-default); }
  .archive-row { min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .details { min-width: 0; overflow: hidden; display: flex; flex-direction: column; border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--surface-card); }
  .details > header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .details > header > div, .details > header > div > span:last-child { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .details > header > div > span:last-child { align-items: flex-start; flex-direction: column; gap: 0; }
  .details h2 { max-width: 280px; margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-body-strong); }
  .details-body { flex: 1; min-height: 0; overflow: auto; padding: var(--space-4); }
  .summary-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-2); margin-bottom: var(--space-4); }
  .summary-grid div { display: flex; flex-direction: column; padding: var(--space-3); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .summary-grid strong { font-size: var(--text-subtitle); }
  .summary-grid small { color: var(--text-tertiary); }
  .item-list { display: flex; flex-direction: column; margin-top: var(--space-4); }
  .item-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); min-height: 54px; padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .item-row > span:nth-child(2) { display: flex; min-width: 0; flex-direction: column; }
  .item-row strong, .item-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item-row small { color: var(--text-tertiary); }
  .item-row em { color: var(--status-error); font-size: var(--text-caption); font-style: normal; }
  .item-state { display: grid; place-items: center; width: 28px; height: 28px; border-radius: var(--radius-medium); color: var(--text-tertiary); background: var(--bg-subtle); }
  .item-state.success { color: var(--status-success); background: var(--status-success-bg); }
  .item-state.error { color: var(--status-error); background: var(--status-error-bg); }
  @media (max-width: 1120px) { .metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); } .list-header, .job-row { grid-template-columns: minmax(220px, 1.5fr) minmax(140px, 1fr) 110px; } .list-header span:nth-child(3), .job-row > span:nth-child(3) { display: none; } }
  @media (max-width: 900px) { .workspace.with-details { grid-template-columns: minmax(0, 1fr); } .details { position: absolute; inset: 0 var(--page-padding) var(--page-padding); z-index: 20; background: var(--surface-overlay); backdrop-filter: blur(30px); } }
  @media (max-width: 720px) { .command-row { align-items: stretch; flex-direction: column; } .command-row :global(.search-box) { width: 100%; } .archive-header, .archive-row { grid-template-columns: minmax(0, 1fr) 36px; } .archive-header span:nth-child(2), .archive-header span:nth-child(3), .archive-row > span:nth-child(2), .archive-row > span:nth-child(3) { display: none; } }
  @media (max-width: 620px) { .metrics { grid-template-columns: 1fr; } .list-header, .job-row { grid-template-columns: minmax(0, 1fr) 100px; } .list-header span:nth-child(2), .job-row > span:nth-child(2) { display: none; } .summary-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
