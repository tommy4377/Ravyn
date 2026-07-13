<script lang="ts">
  import { describeError } from "../api/errors";
  import type { BulkJobAction, Job, JobKind, JobStatus } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import InlineError from "../components/InlineError.svelte";
  import type { MenuItem } from "../components/Menu.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import { JobsService } from "../services/jobs";
  import CommandBar, { type Command } from "../shell/CommandBar.svelte";
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
  let removeIds = $state<string[] | null>(null);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);
  let scrollToIndex = $state<number | null>(null);
  let statusFilter = $state("");
  let kindFilter = $state("");
  let sortKey = $state("added");
  let sortDir = $state<"asc" | "desc">("desc");

  const VIEW_TABS: { id: JobView; label: string }[] = [
    { id: "all", label: "All" },
    { id: "active", label: "Active" },
    { id: "queued", label: "Queued" },
    { id: "completed", label: "Completed" },
    { id: "failed", label: "Failed" },
  ];

  const STATUS_OPTIONS: DropdownOption[] = [
    { value: "", label: "Any status" },
    { value: "queued", label: "Queued" },
    { value: "probing", label: "Probing" },
    { value: "downloading", label: "Downloading" },
    { value: "paused", label: "Paused" },
    { value: "verifying", label: "Verifying" },
    { value: "post_processing", label: "Post-processing" },
    { value: "seeding", label: "Seeding" },
    { value: "completed", label: "Completed" },
    { value: "partial", label: "Partially completed" },
    { value: "failed", label: "Failed" },
    { value: "cancelled", label: "Cancelled" },
  ];
  const KIND_OPTIONS: DropdownOption[] = [
    { value: "", label: "Any kind" },
    { value: "http", label: "Direct download" },
    { value: "media", label: "Media" },
    { value: "torrent", label: "Torrent" },
  ];
  const SORT_OPTIONS: DropdownOption[] = [
    { value: "added", label: "Sort by date added" },
    { value: "name", label: "Sort by name" },
    { value: "size", label: "Sort by size" },
    { value: "status", label: "Sort by status" },
  ];

  const baseJobs = $derived(jobsStore.jobsFor(navigation.downloadsView));
  const visibleJobs = $derived(
    [...baseJobs].sort((a, b) => {
      let cmp = 0;
      if (sortKey === "added") cmp = a.created_at.localeCompare(b.created_at);
      else if (sortKey === "name") cmp = (a.filename ?? a.source).localeCompare(b.filename ?? b.source);
      else if (sortKey === "size") cmp = (a.total_bytes ?? 0) - (b.total_bytes ?? 0);
      else cmp = a.status.localeCompare(b.status);
      return sortDir === "asc" ? cmp : -cmp;
    }),
  );
  const visibleOrder = $derived(visibleJobs.map((job) => job.id));
  const hasActiveFilter = $derived(
    !!jobsStore.searchTerm || !!statusFilter || !!kindFilter || navigation.downloadsView !== "all",
  );

  $effect(() => {
    selection.reconcile(new Set(jobsStore.list.map((job) => job.id)));
  });

  // Single debounced trigger for search/status/kind changes. The very first
  // run (component mount) fires immediately so the list appears without an
  // artificial delay; subsequent filter edits are debounced.
  let firstLoad = true;
  $effect(() => {
    const search = searchInput;
    const status = statusFilter;
    const kind = kindFilter;
    const handle = setTimeout(
      () => {
        firstLoad = false;
        void jobsStore.loadInitial({
          search: search || undefined,
          status: (status || undefined) as JobStatus | undefined,
          kind: (kind || undefined) as JobKind | undefined,
        });
      },
      firstLoad ? 0 : 300,
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
    if (event.key === "Delete" && selection.size > 0) {
      requestRemove([...selection.ids]);
    }
  }

  async function runBulk(action: BulkJobAction, ids: string[]): Promise<void> {
    if (!service || ids.length === 0) return;
    try {
      const results = await service.bulkAction(action, ids);
      const failed = results.filter((result) => !result.success);
      if (failed.length > 0) {
        notifications.error(
          `${failed.length} of ${ids.length} ${actionVerb(action)} failed`,
          failed[0]?.error ?? undefined,
        );
      }
    } catch (error) {
      notifications.error(`Couldn't ${actionVerb(action)} the selection`, describeError(error));
    }
  }

  function actionVerb(action: BulkJobAction): string {
    switch (action) {
      case "pause":
        return "pause";
      case "resume":
        return "resume";
      case "cancel":
        return "cancel";
      case "retry":
        return "retry";
      default:
        return "remove";
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
      for (const result of results) {
        if (result.success) jobsStore.removeLocal(result.id);
      }
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
    addDialogOpen = true;
  }

  async function pasteAndAdd(): Promise<void> {
    try {
      addDialogSource = await navigator.clipboard.readText();
    } catch {
      addDialogSource = "";
      notifications.info("Couldn't read the clipboard automatically — paste the URL manually.");
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

  const selectedJobs = $derived(
    [...selection.ids].map((id) => jobsStore.byId.get(id)).filter((job): job is Job => job !== undefined),
  );
  const canPause = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).pause));
  const canResume = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).resume));
  const canRetry = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).retry));
  const canCancel = $derived(selectedJobs.some((job) => permittedActions(job.status, job.kind).cancel));
  const canRemove = $derived(selectedJobs.length > 0);

  const commands = $derived<Command[]>([
    { id: "add", label: "Add", icon: "add", accent: true, onSelect: openAddDialog },
    { id: "paste", label: "Paste", icon: "paste", onSelect: () => void pasteAndAdd() },
    { id: "pause", label: "Pause", icon: "pause", disabled: !canPause, onSelect: () => void runBulk("pause", [...selection.ids]) },
    { id: "resume", label: "Resume", icon: "play", disabled: !canResume, onSelect: () => void runBulk("resume", [...selection.ids]) },
    { id: "retry", label: "Retry", icon: "refresh", disabled: !canRetry, onSelect: () => void runBulk("retry", [...selection.ids]) },
    { id: "cancel", label: "Cancel", icon: "cancel", disabled: !canCancel, onSelect: () => void runBulk("cancel", [...selection.ids]) },
    { id: "remove", label: "Remove", icon: "trash", disabled: !canRemove, onSelect: () => requestRemove([...selection.ids]) },
  ]);

  const overflow = $derived<MenuItem[]>([
    { id: "refresh", label: "Refresh", icon: "refresh", onSelect: () => jobsStore.refreshAll() },
    { id: "select-all", label: "Select all", icon: "check-circle", onSelect: () => selection.selectAll(visibleOrder) },
    { id: "clear-selection", label: "Clear selection", icon: "close", disabled: selection.size === 0, onSelect: () => selection.clear() },
  ]);
</script>

<div class="downloads">
  <CommandBar {commands} {overflow}>
    {#snippet trailing()}
      <SearchBox bind:value={searchInput} label="Search downloads" placeholder="Search downloads" />
    {/snippet}
  </CommandBar>

  <div class="filters">
    <div class="tabs">
      {#each VIEW_TABS as viewTab (viewTab.id)}
        <button
          type="button"
          class="tab"
          aria-current={navigation.downloadsView === viewTab.id ? "page" : undefined}
          onclick={() => (navigation.downloadsView = viewTab.id)}
        >
          {viewTab.label}
        </button>
      {/each}
    </div>
    <div class="controls">
      <Dropdown options={STATUS_OPTIONS} label="Filter by status" bind:value={statusFilter} />
      <Dropdown options={KIND_OPTIONS} label="Filter by kind" bind:value={kindFilter} />
      <Dropdown options={SORT_OPTIONS} label="Sort" bind:value={sortKey} />
      <button
        type="button"
        class="sort-dir"
        aria-label={sortDir === "asc" ? "Sort ascending" : "Sort descending"}
        onclick={() => (sortDir = sortDir === "asc" ? "desc" : "asc")}
      >
        {sortDir === "asc" ? "↑" : "↓"}
      </button>
    </div>
  </div>

  <div class="list">
    {#if jobsStore.status === "error"}
      <InlineError title="Couldn't load downloads" message={jobsStore.errorMessage ?? ""} retry={() => jobsStore.refreshAll()} />
    {:else if jobsStore.status === "loading" && !jobsStore.hasLoadedOnce}
      <div class="skeletons">
        {#each Array(8) as _}
          <Skeleton height="var(--row-height)" />
        {/each}
      </div>
    {:else if visibleJobs.length === 0}
      <EmptyState
        icon="download"
        title={hasActiveFilter ? "No downloads match these filters" : "No downloads yet"}
        message={hasActiveFilter
          ? "Try a different search term, or clear the search/status/kind filters and view."
          : "Add a URL to start your first download."}
      >
        {#snippet children()}
          {#if !hasActiveFilter}
            <Button variant="accent" onclick={openAddDialog}>Add download</Button>
          {/if}
        {/snippet}
      </EmptyState>
    {:else}
      <VirtualList
        items={visibleJobs}
        itemHeight={navigation.density === "compact" ? 28 : 36}
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
        <div class="load-more">
          <Button variant="standard" disabled={jobsStore.loadingMore} onclick={() => jobsStore.loadMore()}>
            {jobsStore.loadingMore ? "Loading…" : "Load more"}
          </Button>
        </div>
      {/if}
    {/if}
  </div>
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
  .downloads {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
  }
  .filters {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--stroke-divider);
    flex: none;
    flex-wrap: wrap;
  }
  .tabs {
    display: flex;
    gap: 2px;
  }
  .tab {
    height: 28px;
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: var(--text-caption);
    cursor: default;
  }
  .tab:hover {
    background: var(--bg-subtle-hover);
  }
  .tab[aria-current="page"] {
    background: var(--accent-subtle);
    color: var(--accent-text);
    font-weight: 600;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .sort-dir {
    display: grid;
    place-items: center;
    width: var(--control-default);
    height: var(--control-default);
    border-radius: var(--radius-medium);
    border: 1px solid var(--stroke-control);
    background: var(--bg-control);
    color: var(--text-primary);
  }
  .list {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .skeletons {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
  }
  .load-more {
    display: flex;
    justify-content: center;
    padding: var(--space-3);
    flex: none;
  }
</style>
