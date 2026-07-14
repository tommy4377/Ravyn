<script lang="ts">
  import { describeError } from "../api/errors";
  import type { BulkJobAction, Job, JobKind } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore, type JobView } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { SelectionStore } from "../stores/selection.svelte";
  import AddDownloadDialog from "./AddDownloadDialog.svelte";
  import { permittedActions } from "./jobPresentation";
  import JobRow from "./JobRow.svelte";

  const selection = new SelectionStore();
  const service = $derived(connection.client ? new JobsService(connection.client) : null);

  let searchInput = $state("");
  let addDialogOpen = $state(false);
  let addDialogSource = $state("");
  let addDialogKind = $state<JobKind>("http");
  let removeIds = $state<string[] | null>(null);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);
  let scrollToIndex = $state<number | null>(null);
  let kindFilter = $state("");
  let sortKey = $state("added");
  let sortDir = $state<"asc" | "desc">("desc");

  const VIEW_TABS: { id: JobView; label: string }[] = [
    { id: "all", label: "All" },
    { id: "active", label: "In progress" },
    { id: "queued", label: "Queued" },
    { id: "completed", label: "Completed" },
    { id: "failed", label: "Needs attention" },
  ];
  const KIND_OPTIONS: DropdownOption[] = [
    { value: "", label: "All types" },
    { value: "http", label: "Direct downloads" },
    { value: "media", label: "Media" },
    { value: "torrent", label: "Torrents" },
  ];
  const SORT_OPTIONS: DropdownOption[] = [
    { value: "added", label: "Date added" },
    { value: "name", label: "Name" },
    { value: "size", label: "Size" },
    { value: "status", label: "Status" },
  ];

  const baseJobs = $derived(jobsStore.jobsFor(navigation.downloadsView));
  const visibleJobs = $derived(
    [...baseJobs].sort((a, b) => {
      let comparison = 0;
      if (sortKey === "added") comparison = a.created_at.localeCompare(b.created_at);
      else if (sortKey === "name") comparison = (a.filename ?? a.source).localeCompare(b.filename ?? b.source);
      else if (sortKey === "size") comparison = (a.total_bytes ?? 0) - (b.total_bytes ?? 0);
      else comparison = a.status.localeCompare(b.status);
      return sortDir === "asc" ? comparison : -comparison;
    }),
  );
  const visibleOrder = $derived(visibleJobs.map((job) => job.id));
  const selectedJobs = $derived(
    [...selection.ids].map((id) => jobsStore.byId.get(id)).filter((job): job is Job => job !== undefined),
  );
  const canPause = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).pause));
  const canResume = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).resume));
  const canRetry = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).retry));
  const canCancel = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).cancel));
  const hasActiveFilter = $derived(!!jobsStore.searchTerm || !!kindFilter || navigation.downloadsView !== "all");
  const activeCount = $derived(jobsStore.jobsFor("active").length);
  const queuedCount = $derived(jobsStore.jobsFor("queued").length);
  const completedCount = $derived(jobsStore.jobsFor("completed").length);
  const pageDescription = $derived(
    `${activeCount} active · ${queuedCount} queued · ${completedCount} completed`,
  );

  $effect(() => {
    selection.reconcile(new Set(jobsStore.list.map((job) => job.id)));
  });

  $effect(() => {
    const requestedKind = navigation.pendingAddKind;
    if (!requestedKind) return;
    addDialogKind = navigation.consumeAddRequest() ?? "http";
    addDialogSource = "";
    addDialogOpen = true;
  });

  let firstLoad = true;
  $effect(() => {
    const search = searchInput;
    const kind = kindFilter;
    const handle = setTimeout(
      () => {
        firstLoad = false;
        void jobsStore.loadInitial({
          search: search || undefined,
          kind: (kind || undefined) as JobKind | undefined,
        });
      },
      firstLoad ? 0 : 260,
    );
    return () => clearTimeout(handle);
  });

  function handleSelect(job: Job, event: MouseEvent): void {
    if (event.shiftKey) selection.selectRange(job.id, visibleOrder);
    else if (event.ctrlKey || event.metaKey) selection.toggle(job.id);
    else selection.selectOnly(job.id);
  }

  function openDetails(job: Job): void {
    navigation.selectJob(job.id);
  }

  function onListKeydown(event: KeyboardEvent): void {
    if (visibleOrder.length === 0) return;
    if ((event.key === "a" || event.key === "A") && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      selection.selectAll(visibleOrder);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const currentId = selection.focusedId ?? visibleOrder[0]!;
      const currentIndex = Math.max(0, visibleOrder.indexOf(currentId));
      const nextIndex = Math.min(
        visibleOrder.length - 1,
        Math.max(0, currentIndex + (event.key === "ArrowDown" ? 1 : -1)),
      );
      const nextId = visibleOrder[nextIndex]!;
      if (event.shiftKey) selection.selectRange(nextId, visibleOrder);
      else selection.selectOnly(nextId);
      scrollToIndex = nextIndex;
      return;
    }
    if (event.key === "Enter" && selection.focusedId) {
      const job = jobsStore.byId.get(selection.focusedId);
      if (job) openDetails(job);
      return;
    }
    if (event.key === "Delete" && selection.size > 0) requestRemove([...selection.ids]);
    if (event.key === "Escape") selection.clear();
  }

  async function runBulk(action: BulkJobAction, ids: string[]): Promise<void> {
    if (!service || ids.length === 0) return;
    try {
      const results = await service.bulkAction(action, ids);
      const failed = results.filter((result) => !result.success);
      if (failed.length > 0) {
        notifications.error(`${failed.length} of ${ids.length} actions failed`, failed[0]?.error ?? undefined);
      }
    } catch (error) {
      notifications.error(`Couldn't ${action} the selection`, describeError(error));
    }
  }

  function requestRemove(ids: string[]): void {
    removeError = null;
    removeIds = ids;
  }

  async function confirmRemove(): Promise<void> {
    if (!removeIds || !service) return;
    removeBusy = true;
    removeError = null;
    try {
      const results = await service.bulkAction("delete", removeIds);
      for (const result of results) if (result.success) jobsStore.removeLocal(result.id);
      const failed = results.filter((result) => !result.success);
      if (failed.length > 0) {
        removeError = `${failed.length} of ${removeIds.length} item(s) could not be removed.`;
      } else {
        notifications.info(removeIds.length === 1 ? "Download removed" : `${removeIds.length} downloads removed`);
        removeIds = null;
        selection.clear();
      }
    } catch (error) {
      removeError = describeError(error);
    } finally {
      removeBusy = false;
    }
  }

  function openAddDialog(): void {
    addDialogSource = "";
    addDialogKind = "http";
    addDialogOpen = true;
  }

  async function pasteAndAdd(): Promise<void> {
    try {
      addDialogSource = await navigator.clipboard.readText();
      const normalized = addDialogSource.trim().toLowerCase();
      addDialogKind = normalized.startsWith("magnet:") || normalized.endsWith(".torrent") ? "torrent" : "http";
    } catch {
      addDialogSource = "";
      addDialogKind = "http";
      notifications.info("Paste the URL manually in the add dialog.");
    }
    addDialogOpen = true;
  }

  const rowActions = {
    onOpenDetails: openDetails,
    onPause: (job: Job) => void runBulk("pause", [job.id]),
    onResume: (job: Job) => void runBulk("resume", [job.id]),
    onRetry: (job: Job) => void runBulk("retry", [job.id]),
    onCancel: (job: Job) => void runBulk("cancel", [job.id]),
    onRemove: (job: Job) => requestRemove([job.id]),
  };
</script>

<div class="downloads">
  <PageHeader eyebrow="Transfers" title="Downloads" description={pageDescription}>
    {#snippet actions()}
      <Button variant="standard" onclick={() => void pasteAndAdd()}><Icon name="paste" size={15} /> Paste</Button>
      <Button variant="accent" onclick={openAddDialog}><Icon name="add" size={15} /> Add download</Button>
    {/snippet}
  </PageHeader>

  <section class="workspace" aria-label="Download manager">
    <div class="toolbar" class:selection-mode={selection.size > 0}>
      {#if selection.size > 0}
        <div class="selection-summary">
          <strong>{selection.size} selected</strong>
          <button type="button" onclick={() => selection.clear()}>Clear selection</button>
        </div>
        <div class="selection-actions">
          {#if canPause}<Button variant="subtle" onclick={() => void runBulk("pause", [...selection.ids])}><Icon name="pause" size={15} /> Pause</Button>{/if}
          {#if canResume}<Button variant="subtle" onclick={() => void runBulk("resume", [...selection.ids])}><Icon name="play" size={15} /> Resume</Button>{/if}
          {#if canRetry}<Button variant="subtle" onclick={() => void runBulk("retry", [...selection.ids])}><Icon name="refresh" size={15} /> Retry</Button>{/if}
          {#if canCancel}<Button variant="subtle" onclick={() => void runBulk("cancel", [...selection.ids])}><Icon name="cancel" size={15} /> Cancel</Button>{/if}
          <Button variant="subtle" onclick={() => requestRemove([...selection.ids])}><Icon name="trash" size={15} /> Remove</Button>
        </div>
      {:else}
        <div class="view-tabs" aria-label="Download view">
          {#each VIEW_TABS as viewTab (viewTab.id)}
            <button
              type="button"
              class="view-tab"
              aria-current={navigation.downloadsView === viewTab.id ? "page" : undefined}
              onclick={() => (navigation.downloadsView = viewTab.id)}
            >
              {viewTab.label}
            </button>
          {/each}
        </div>
        <div class="toolbar-controls">
          <SearchBox bind:value={searchInput} label="Search downloads" placeholder="Search downloads" />
          <Dropdown options={KIND_OPTIONS} label="Filter by type" bind:value={kindFilter} />
          <Dropdown options={SORT_OPTIONS} label="Sort downloads" bind:value={sortKey} />
          <IconButton
            icon={sortDir === "asc" ? "chevron-up" : "chevron-down"}
            label={sortDir === "asc" ? "Sort ascending" : "Sort descending"}
            variant="standard"
            onclick={() => (sortDir = sortDir === "asc" ? "desc" : "asc")}
          />
          <IconButton icon="refresh" label="Refresh downloads" variant="subtle" onclick={() => jobsStore.refreshAll()} />
        </div>
      {/if}
    </div>

    <div class="column-header" aria-hidden="true">
      <span>Name</span><span>Status and progress</span><span>Transfer</span><span>Size</span><span>Added</span>
    </div>

    <div class="list">
      {#if jobsStore.status === "error"}
        <InlineError title="Couldn't load downloads" message={jobsStore.errorMessage ?? ""} retry={() => jobsStore.refreshAll()} />
      {:else if jobsStore.status === "loading" && !jobsStore.hasLoadedOnce}
        <div class="skeletons">
          {#each Array(8) as _}<Skeleton height={navigation.density === "compact" ? "48px" : "60px"} />{/each}
        </div>
      {:else if visibleJobs.length === 0}
        <EmptyState
          icon="download"
          title={hasActiveFilter ? "No downloads match this view" : "No downloads yet"}
          message={hasActiveFilter ? "Change the view, search term or download type." : "Add a URL to start your first download."}
        >
          {#snippet children()}
            {#if !hasActiveFilter}<Button variant="accent" onclick={openAddDialog}>Add download</Button>{/if}
          {/snippet}
        </EmptyState>
      {:else}
        <VirtualList
          items={visibleJobs}
          itemHeight={navigation.density === "compact" ? 48 : 60}
          getKey={(job) => job.id}
          ariaLabel="Downloads"
          ariaMultiselectable
          bind:scrollToIndex
          onkeydown={onListKeydown}
          activeDescendant={selection.focusedId ? `job-row-${selection.focusedId}` : undefined}
        >
          {#snippet row(job, _index)}
            <JobRow
              {job}
              selected={selection.isSelected(job.id)}
              focused={selection.focusedId === job.id}
              actions={rowActions}
              onSelect={handleSelect}
              onOpenDetails={openDetails}
            />
          {/snippet}
        </VirtualList>
        {#if jobsStore.nextCursor}
          <div class="load-more"><Button variant="standard" disabled={jobsStore.loadingMore} onclick={() => jobsStore.loadMore()}>{jobsStore.loadingMore ? "Loading…" : "Load more"}</Button></div>
        {/if}
      {/if}
    </div>
  </section>
</div>

<AddDownloadDialog open={addDialogOpen} initialSource={addDialogSource} onClose={() => (addDialogOpen = false)} />
<ConfirmDialog
  open={removeIds !== null}
  title={removeIds && removeIds.length > 1 ? `Remove ${removeIds.length} downloads?` : "Remove this download?"}
  message="This removes the entry from Ravyn's list. Files already downloaded are not deleted."
  confirmLabel="Remove from list"
  destructive
  busy={removeBusy}
  error={removeError}
  onConfirm={confirmRemove}
  onClose={() => (removeIds = null)}
/>

<style>
  .downloads { height: 100%; min-width: 0; display: flex; flex-direction: column; }
  .workspace { flex: 1; min-height: 0; margin: 0 var(--page-padding) var(--page-padding); display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: color-mix(in srgb, var(--surface-card) 76%, transparent); }
  .toolbar { min-height: 52px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--stroke-divider); background: transparent; }
  .toolbar.selection-mode { background: color-mix(in srgb, var(--accent-subtle) 62%, var(--bg-layer-alt)); }
  .view-tabs { display: flex; align-items: center; gap: 2px; min-width: 0; }
  .view-tab { min-height: 32px; padding: 0 var(--space-3); border: 0; border-radius: var(--radius-control); background: transparent; color: var(--text-secondary); font-size: var(--text-caption); cursor: default; white-space: nowrap; }
  .view-tab:hover { background: var(--bg-subtle-hover); color: var(--text-primary); }
  .view-tab[aria-current="page"] { background: var(--bg-subtle-hover); color: var(--text-primary); box-shadow: inset 0 -2px var(--accent-default); font-weight: 600; }
  .toolbar-controls, .selection-actions { display: flex; align-items: center; gap: var(--space-2); min-width: 0; }
  .selection-summary { display: flex; align-items: center; gap: var(--space-3); }
  .selection-summary button { border: 0; background: transparent; color: var(--accent-text); font-size: var(--text-caption); cursor: default; }
  .column-header { display: grid; grid-template-columns: minmax(220px, 2fr) minmax(180px, 1.25fr) minmax(115px, .72fr) minmax(82px, .55fr) minmax(96px, .64fr); gap: var(--space-3); min-height: 36px; align-items: center; padding: 0 var(--space-4); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .list { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .skeletons { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-3); }
  .load-more { display: flex; justify-content: center; padding: var(--space-3); border-top: 1px solid var(--stroke-divider); }
  @media (max-width: 1180px) {
    .toolbar { align-items: flex-start; flex-direction: column; }
    .toolbar-controls { width: 100%; }
    .toolbar-controls :global(.search-box) { flex: 1; }
  }
  @media (max-width: 980px) {
    .column-header { grid-template-columns: minmax(210px, 2fr) minmax(175px, 1.25fr) minmax(105px, .72fr) minmax(78px, .55fr); }
    .column-header span:last-child { display: none; }
  }
  @media (max-width: 760px) {
    .view-tabs { max-width: 100%; overflow-x: auto; }
    .toolbar-controls { flex-wrap: wrap; }
    .toolbar-controls :global(.search-box) { width: 100%; flex-basis: 100%; }
    .selection-actions { flex-wrap: wrap; }
    .column-header { display: none; }
  }
</style>
