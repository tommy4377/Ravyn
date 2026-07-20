<script lang="ts">
  import { describeError } from "../api/errors";
  import type { BulkJobAction, Job, JobKind } from "../api/types";
  import Button from "../components/Button.svelte";
  import CompactSummary, { type SummaryItem } from "../components/CompactSummary.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import FilterFlyout from "../components/FilterFlyout.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import type { MenuItem } from "../components/Menu.svelte";
  import PageCommandBar from "../components/PageCommandBar.svelte";
  import PageScaffold from "../components/PageScaffold.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import Skeleton from "../components/Skeleton.svelte";
  import VirtualList from "../components/VirtualList.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore, type JobView } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { SelectionStore } from "../stores/selection.svelte";
  import { formatSpeed } from "../util/format";
  import AddDownloadDialog from "./AddDownloadDialog.svelte";
  import BatchImportDialog from "./BatchImportDialog.svelte";
  import MetalinkImportDialog from "./MetalinkImportDialog.svelte";
  import { permittedActions } from "./jobPresentation";
  import JobRow from "./JobRow.svelte";

  type SortKey = "added" | "name" | "size" | "status";

  const selection = new SelectionStore();
  const service = $derived(connection.client ? new JobsService(connection.client) : null);

  let searchInput = $state("");
  let addDialogOpen = $state(false);
  let addDialogSource = $state("");
  let addDialogKind = $state<JobKind>("http");
  let metalinkDialogOpen = $state(false);
  let metalinkInitialDocument = $state("");
  let batchImportOpen = $state(false);
  let batchInitialText = $state("");
  let removeIds = $state<string[] | null>(null);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);
  let scrollToIndex = $state<number | null>(null);
  let kindFilter = $state("");
  let sortKey = $state<SortKey>(loadSortKey());
  let sortDir = $state<"asc" | "desc">(loadSortDirection());
  let dragDepth = 0;
  let dragActive = $state(false);

  const KIND_OPTIONS: DropdownOption[] = [
    { value: "", label: "All download types" },
    { value: "http", label: "Direct downloads" },
    { value: "media", label: "Media" },
    { value: "torrent", label: "Torrents" },
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
  const filterCount = $derived(kindFilter ? 1 : 0);
  const hasActiveFilter = $derived(searchInput.trim().length > 0 || filterCount > 0 || navigation.downloadsView !== "all");
  const activeCount = $derived(jobsStore.summary?.active ?? jobsStore.jobsFor("active").length);
  const queuedCount = $derived(jobsStore.summary?.queued ?? jobsStore.jobsFor("queued").length);
  const completedCount = $derived(jobsStore.summary?.completed ?? jobsStore.jobsFor("completed").length);
  const failedCount = $derived(jobsStore.summary?.failed ?? jobsStore.jobsFor("failed").length);
  const totalCount = $derived(jobsStore.summary?.total ?? jobsStore.list.length);
  const totalSpeed = $derived(
    jobsStore.summary?.speed_bps ??
      [...jobsStore.liveProgress.values()].reduce((sum, progress) => sum + Math.max(0, progress.bytesPerSecond), 0),
  );
  const summaryItems = $derived<SummaryItem[]>([
    { label: "active", value: String(activeCount) },
    { label: "queued", value: String(queuedCount) },
    { label: "down", value: formatSpeed(totalSpeed) },
    { label: "need attention", value: String(failedCount), tone: failedCount > 0 ? "error" : "default" },
  ]);
  const viewTabs = $derived([
    { id: "all" as JobView, label: "All", count: totalCount },
    { id: "active" as JobView, label: "In progress", count: activeCount },
    { id: "queued" as JobView, label: "Queued", count: queuedCount },
    { id: "completed" as JobView, label: "Completed", count: completedCount },
    { id: "failed" as JobView, label: "Needs attention", count: failedCount },
  ]);
  const sortMenuItems = $derived<MenuItem[]>([
    { id: "sort-added", label: "Date added", icon: sortKey === "added" ? "check-circle" : "clock", onSelect: () => setSort("added") },
    { id: "sort-name", label: "Name", icon: sortKey === "name" ? "check-circle" : "file", onSelect: () => setSort("name") },
    { id: "sort-size", label: "Size", icon: sortKey === "size" ? "check-circle" : "hard-drive", onSelect: () => setSort("size") },
    { id: "sort-status", label: "Status", icon: sortKey === "status" ? "check-circle" : "info", onSelect: () => setSort("status") },
    {
      id: "sort-direction",
      label: sortDir === "asc" ? "Ascending" : "Descending",
      icon: sortDir === "asc" ? "chevron-up" : "chevron-down",
      separatorBefore: true,
      onSelect: () => (sortDir = sortDir === "asc" ? "desc" : "asc"),
    },
  ]);
  const addMenuItems = $derived<MenuItem[]>([
    { id: "new-download", label: "New download", icon: "download", onSelect: openAddDialog },
    { id: "paste-add", label: "Paste and add", icon: "paste", onSelect: () => void pasteAndAdd() },
    { id: "import-metalink", label: "Import Metalink", icon: "document", separatorBefore: true, onSelect: () => { metalinkInitialDocument = ""; metalinkDialogOpen = true; } },
    { id: "import-batch", label: "Import batch file", icon: "basket", onSelect: () => { batchInitialText = ""; batchImportOpen = true; } },
    { id: "batch-queue", label: "Open batch queue", icon: "basket", separatorBefore: true, onSelect: () => navigation.openBasket() },
  ]);
  const moreMenuItems = $derived<MenuItem[]>([
    { id: "paste", label: "Paste and add", icon: "paste", onSelect: () => void pasteAndAdd() },
    { id: "batch", label: "Open batch queue", icon: "basket", onSelect: () => navigation.openBasket() },
    { id: "refresh", label: "Refresh downloads", icon: "refresh", separatorBefore: true, onSelect: () => jobsStore.refreshAll() },
  ]);
  const selectionMoreItems = $derived<MenuItem[]>([
    ...(canCancel
      ? [{ id: "cancel", label: "Cancel downloads", icon: "cancel" as const, onSelect: () => void runBulk("cancel", [...selection.ids]) }]
      : []),
    {
      id: "remove",
      label: "Remove from list",
      icon: "trash",
      danger: true,
      separatorBefore: canCancel,
      onSelect: () => requestRemove([...selection.ids]),
    },
  ]);

  $effect(() => {
    selection.reconcile(new Set(jobsStore.list.map((job) => job.id)));
  });

  $effect(() => {
    localStorage.setItem("ravyn.downloadsSortKey", sortKey);
    localStorage.setItem("ravyn.downloadsSortDirection", sortDir);
  });

  $effect(() => {
    const requestedKind = navigation.pendingAddKind;
    if (!requestedKind) return;
    addDialogKind = navigation.consumeAddRequest() ?? "http";
    addDialogSource = navigation.consumeAddSource();
    addDialogOpen = true;
  });

  $effect(() => {
    const onPasteAdd = (): void => { void pasteAndAdd(); };
    window.addEventListener("ravyn:paste-add", onPasteAdd);
    return () => window.removeEventListener("ravyn:paste-add", onPasteAdd);
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

  function loadSortKey(): SortKey {
    const value = localStorage.getItem("ravyn.downloadsSortKey");
    return value === "name" || value === "size" || value === "status" ? value : "added";
  }

  function loadSortDirection(): "asc" | "desc" {
    return localStorage.getItem("ravyn.downloadsSortDirection") === "asc" ? "asc" : "desc";
  }

  function setSort(next: SortKey): void {
    if (sortKey === next) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
      return;
    }
    sortKey = next;
    sortDir = next === "name" ? "asc" : "desc";
  }

  function ariaSort(key: SortKey): "ascending" | "descending" | "none" {
    return sortKey === key ? (sortDir === "asc" ? "ascending" : "descending") : "none";
  }

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
    if (event.key === " " && selection.size > 0) {
      event.preventDefault();
      if (canPause) void runBulk("pause", [...selection.ids]);
      else if (canResume) void runBulk("resume", [...selection.ids]);
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
      const clipboard = await navigator.clipboard.readText();
      const meaningful = clipboard.split(/\r?\n/).map((line) => line.trim()).filter((line) => line && !line.startsWith("#") && !line.startsWith("//"));
      if (meaningful.length > 1 || clipboard.trim().startsWith("[")) {
        batchInitialText = clipboard;
        batchImportOpen = true;
        return;
      }
      if (/^\s*<\?xml|<metalink(?:\s|>)/i.test(clipboard)) {
        metalinkInitialDocument = clipboard;
        metalinkDialogOpen = true;
        return;
      }
      addDialogSource = clipboard;
      const normalized = addDialogSource.trim().toLowerCase();
      addDialogKind = normalized.startsWith("magnet:") || normalized.endsWith(".torrent") ? "torrent" : "http";
      addDialogOpen = true;
    } catch {
      addDialogSource = "";
      addDialogKind = "http";
      notifications.info("Paste the source manually in the add dialog.");
      addDialogOpen = true;
    }
  }

  function handleDragEnter(event: DragEvent): void {
    event.preventDefault();
    dragDepth += 1;
    dragActive = true;
  }

  function handleDragLeave(event: DragEvent): void {
    event.preventDefault();
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) dragActive = false;
  }

  function handleDragOver(event: DragEvent): void {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  }

  async function handleDrop(event: DragEvent): Promise<void> {
    event.preventDefault();
    dragDepth = 0;
    dragActive = false;
    const transfer = event.dataTransfer;
    if (!transfer) return;

    const files = [...transfer.files];
    if (files.length === 1 && /\.(meta4|metalink)$/i.test(files[0]!.name)) {
      try {
        metalinkInitialDocument = await files[0]!.text();
        metalinkDialogOpen = true;
      } catch {
        notifications.error("Couldn't read the dropped Metalink document");
      }
      return;
    }
    const filePaths = files
      .map((file) => (file as File & { path?: string }).path ?? file.name)
      .filter((path) => path.trim().length > 0);
    const uriList = transfer.getData("text/uri-list")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#"));
    const plainText = transfer.getData("text/plain").trim();
    const sources = filePaths.length > 0 ? filePaths : uriList.length > 0 ? uriList : plainText ? [plainText] : [];
    if (sources.length === 0) {
      notifications.warning("The dropped content does not contain a supported source.");
      return;
    }

    if (sources.length > 1) {
      batchInitialText = sources.join("\n");
      batchImportOpen = true;
      return;
    }
    addDialogSource = sources[0] ?? "";
    const normalized = addDialogSource.trim().toLowerCase();
    addDialogKind = normalized.startsWith("magnet:") || normalized.endsWith(".torrent") ? "torrent" : "http";
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

<PageScaffold title="Downloads" summary="Manage transfers and review completed files">
  {#snippet actions()}
    <MenuButton label="Add" icon="add" items={addMenuItems} variant="accent" />
  {/snippet}

  {#snippet commandBar()}
    <PageCommandBar selectedCount={selection.size}>
      {#snippet leading()}
        <div class="view-tabs" aria-label="Download view">
          {#each viewTabs as viewTab (viewTab.id)}
            <button
              type="button"
              class="view-tab"
              aria-current={navigation.downloadsView === viewTab.id ? "page" : undefined}
              onclick={() => (navigation.downloadsView = viewTab.id)}
            >
              <span>{viewTab.label}</span>
              {#if viewTab.count > 0}<span class="tab-count">{viewTab.count}</span>{/if}
            </button>
          {/each}
        </div>
      {/snippet}

      {#snippet actions()}
        <SearchBox inputId="downloads-search" bind:value={searchInput} label="Search downloads" placeholder="Search downloads" />
        <FilterFlyout count={filterCount} onClear={() => (kindFilter = "")}>
          <div class="filter-field">
            <label for="download-kind-filter">Download type</label>
            <Dropdown id="download-kind-filter" options={KIND_OPTIONS} label="Filter by download type" bind:value={kindFilter} />
          </div>
        </FilterFlyout>
        <MenuButton label="Sort" icon="sort" items={sortMenuItems} />
        <MenuButton label="More" icon="more" items={moreMenuItems} variant="subtle" iconOnly />
      {/snippet}

      {#snippet selectionContent()}
        <div class="selection-summary">
          <strong>{selection.size} selected</strong>
          <button type="button" onclick={() => selection.clear()}>Clear selection</button>
        </div>
        <div class="selection-actions">
          {#if canPause}<Button variant="subtle" onclick={() => void runBulk("pause", [...selection.ids])}><Icon name="pause" size={15} /> Pause</Button>{/if}
          {#if canResume}<Button variant="subtle" onclick={() => void runBulk("resume", [...selection.ids])}><Icon name="play" size={15} /> Resume</Button>{/if}
          {#if canRetry}<Button variant="subtle" onclick={() => void runBulk("retry", [...selection.ids])}><Icon name="refresh" size={15} /> Retry</Button>{/if}
          <MenuButton label="More selection actions" icon="more" items={selectionMoreItems} variant="subtle" iconOnly />
        </div>
      {/snippet}
    </PageCommandBar>
  {/snippet}

  {#snippet status()}
    <div class="summary-strip"><CompactSummary items={summaryItems} ariaLabel="Download activity summary" /></div>
  {/snippet}

  <section
    class="workspace"
    aria-label="Download manager"
    ondragenter={handleDragEnter}
    ondragleave={handleDragLeave}
    ondragover={handleDragOver}
    ondrop={(event) => void handleDrop(event)}
  >
    {#if dragActive}
      <div class="drop-overlay" aria-live="polite">
        <Icon name="download" size={28} />
        <strong>Drop to add</strong>
        <span>Links, magnets, torrent files, Metalink documents and local files are supported.</span>
      </div>
    {/if}
    <div class="column-header">
      <button type="button" aria-pressed={sortKey === "name"} aria-label={`Sort by name, ${ariaSort("name")}`} onclick={() => setSort("name")}>Name <Icon name={sortKey === "name" ? (sortDir === "asc" ? "chevron-up" : "chevron-down") : "sort"} size={12} /></button>
      <button type="button" aria-pressed={sortKey === "status"} aria-label={`Sort by status, ${ariaSort("status")}`} onclick={() => setSort("status")}>Status and progress <Icon name={sortKey === "status" ? (sortDir === "asc" ? "chevron-up" : "chevron-down") : "sort"} size={12} /></button>
      <span>Transfer</span>
      <button type="button" aria-pressed={sortKey === "size"} aria-label={`Sort by size, ${ariaSort("size")}`} onclick={() => setSort("size")}>Size <Icon name={sortKey === "size" ? (sortDir === "asc" ? "chevron-up" : "chevron-down") : "sort"} size={12} /></button>
      <button type="button" aria-pressed={sortKey === "added"} aria-label={`Sort by date added, ${ariaSort("added")}`} onclick={() => setSort("added")}>Added <Icon name={sortKey === "added" ? (sortDir === "asc" ? "chevron-up" : "chevron-down") : "sort"} size={12} /></button>
      <span aria-hidden="true"></span>
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
          message={hasActiveFilter ? "Change the view, search term or download type." : "Add a link, magnet or local file to start."}
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
</PageScaffold>

<AddDownloadDialog open={addDialogOpen} initialSource={addDialogSource} initialKind={addDialogKind} onClose={() => (addDialogOpen = false)} />
<MetalinkImportDialog open={metalinkDialogOpen} initialDocument={metalinkInitialDocument} onClose={() => { metalinkDialogOpen = false; metalinkInitialDocument = ""; }} />
<BatchImportDialog open={batchImportOpen} initialText={batchInitialText} onClose={() => { batchImportOpen = false; batchInitialText = ""; }} />
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
  .summary-strip { min-height: 34px; display: flex; align-items: center; padding: 0 var(--page-padding); border-bottom: 1px solid var(--stroke-divider); background: var(--surface-content); }
  .workspace { position: relative; height: 100%; min-height: 0; margin: 0 var(--page-padding) var(--page-padding); display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--surface-card); }
  .drop-overlay { position: absolute; z-index: 80; inset: var(--space-3); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-2); border: 2px dashed var(--accent-default); border-radius: var(--radius-layer); background: color-mix(in srgb, var(--surface-overlay) 90%, var(--accent-subtle)); color: var(--text-primary); text-align: center; pointer-events: none; backdrop-filter: blur(12px); }
  .drop-overlay :global(svg) { color: var(--accent-text); }
  .drop-overlay strong { font-size: var(--text-subtitle); }
  .drop-overlay span { max-width: 420px; color: var(--text-secondary); font-size: var(--text-caption); }
  .view-tabs { display: flex; align-items: center; gap: 2px; min-width: 0; }
  .view-tab { min-height: 32px; display: inline-flex; align-items: center; gap: 6px; padding: 0 var(--space-3); border: 0; border-radius: var(--radius-control); background: transparent; color: var(--text-secondary); font-size: var(--text-caption); cursor: default; white-space: nowrap; }
  .view-tab:hover { background: var(--bg-subtle-hover); color: var(--text-primary); }
  .view-tab[aria-current="page"] { background: var(--bg-subtle-hover); color: var(--text-primary); box-shadow: inset 0 -2px var(--accent-default); font-weight: 600; }
  .tab-count { color: var(--text-tertiary); font-size: 11px; font-variant-numeric: tabular-nums; }
  .selection-summary, .selection-actions { display: flex; align-items: center; gap: var(--space-2); }
  .selection-summary { gap: var(--space-3); }
  .selection-summary button { border: 0; background: transparent; color: var(--accent-text); font: inherit; font-size: var(--text-caption); cursor: default; }
  .filter-field { display: flex; flex-direction: column; align-items: stretch; gap: var(--space-2); }
  .filter-field label { color: var(--text-secondary); font-size: var(--text-caption); font-weight: 600; }
  .filter-field :global(.dropdown), .filter-field :global(select) { width: 100%; }
  /* scrollbar-gutter mirrors the VirtualList viewport so header and row
     columns stay aligned whether or not the list currently scrolls. */
  .column-header { display: grid; grid-template-columns: minmax(220px, 2fr) minmax(180px, 1.25fr) minmax(115px, .72fr) minmax(82px, .55fr) minmax(96px, .64fr) 32px; gap: var(--space-3); min-height: 36px; align-items: center; padding: 0 var(--space-3) 0 var(--space-4); overflow-y: hidden; scrollbar-gutter: stable; border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .column-header button { min-width: 0; display: inline-flex; align-items: center; gap: 4px; justify-self: start; padding: 4px 0; border: 0; background: transparent; color: inherit; font: inherit; cursor: default; }
  .column-header button:hover { color: var(--text-primary); }
  .column-header button :global(svg) { opacity: .55; }
  .column-header button:hover :global(svg), .column-header button[aria-pressed="true"] :global(svg) { opacity: 1; }
  .list { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .skeletons { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-3); }
  .load-more { display: flex; justify-content: center; padding: var(--space-3); border-top: 1px solid var(--stroke-divider); }
  @media (max-width: 1040px) {
    .column-header { grid-template-columns: minmax(210px, 2fr) minmax(175px, 1.25fr) minmax(105px, .72fr) minmax(78px, .55fr) 32px; }
    .column-header > :nth-child(5) { display: none; }
  }
  @media (max-width: 760px) {
    .view-tabs { max-width: 100%; overflow-x: auto; }
    .column-header { display: none; }
    .workspace { margin-inline: 0; border-inline: 0; border-radius: 0; }
    .summary-strip { overflow-x: auto; }
  }
</style>
