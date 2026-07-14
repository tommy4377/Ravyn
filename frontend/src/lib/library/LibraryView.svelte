<script lang="ts">
  import { describeError } from "../api/errors";
  import type { LibraryCategory, LibraryEntry, LibraryEntryState, LibraryImportStatus } from "../api/types";
  import Button from "../components/Button.svelte";
  import CompactSummary, { type SummaryItem } from "../components/CompactSummary.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import DetailsPane from "../components/DetailsPane.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import ListDetailsLayout from "../components/ListDetailsLayout.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import type { MenuItem } from "../components/Menu.svelte";
  import PageCommandBar from "../components/PageCommandBar.svelte";
  import PageScaffold from "../components/PageScaffold.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import Tabs from "../components/Tabs.svelte";
  import TextField from "../components/TextField.svelte";
  import { openNativePath, revealNativePath } from "../native/tauri";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes } from "../util/format";
  import LibraryRelocationDialog from "./LibraryRelocationDialog.svelte";
  import LibraryStatisticsDialog from "./LibraryStatisticsDialog.svelte";
  import {
    groupLibraryDuplicates,
    libraryTypeLabel,
    sortLibraryEntries,
    type LibraryMode,
    type LibrarySortKey,
    type SortDirection,
  } from "./libraryPresentation";

  const SORT_STORAGE_KEY = "ravyn.library.sort";

  let entries = $state<LibraryEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let search = $state("");
  let category = $state("");
  let mode = $state<LibraryMode>("files");
  let selectedId = $state<string | null>(null);
  let sortKey = $state<LibrarySortKey>("modified");
  let sortDirection = $state<SortDirection>("desc");

  let importOpen = $state(false);
  let importPath = $state("");
  let importTags = $state("");
  let importMaxEntries = $state("100000");
  let importMaxDepth = $state("64");
  let importBusy = $state(false);
  let importCancelBusy = $state(false);
  let importStatus = $state<LibraryImportStatus | null>(null);
  let relocationOpen = $state(false);
  let statisticsOpen = $state(false);

  let removeEntry = $state<LibraryEntry | null>(null);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);

  const selected = $derived(entries.find((entry) => entry.id === selectedId) ?? null);
  const totalSize = $derived(entries.reduce((sum, entry) => sum + (entry.size_bytes ?? 0), 0));
  const missingCount = $derived(entries.filter((entry) => entry.state === "missing").length);
  const duplicateGroups = $derived(groupLibraryDuplicates(entries));
  const duplicateEntryCount = $derived(duplicateGroups.reduce((sum, group) => sum + group.entries.length, 0));
  const sortedEntries = $derived(sortLibraryEntries(entries, sortKey, sortDirection));
  const summaryItems = $derived<SummaryItem[]>([
    { label: mode === "trash" ? "in trash" : "visible files", value: entries.length.toLocaleString() },
    { label: "indexed", value: formatBytes(totalSize) },
    { label: "missing", value: missingCount.toLocaleString(), tone: missingCount ? "warning" : "default" },
    { label: "duplicate copies", value: duplicateEntryCount.toLocaleString(), tone: duplicateEntryCount ? "warning" : "default" },
  ]);

  const viewTabs = [
    { id: "files", label: "Files" },
    { id: "trash", label: "Trash" },
    { id: "duplicates", label: "Duplicates" },
  ];

  const categoryOptions: DropdownOption[] = [
    { value: "", label: "All types" },
    { value: "downloads", label: "Downloads" },
    { value: "videos", label: "Videos" },
    { value: "music", label: "Music" },
    { value: "documents", label: "Documents" },
    { value: "images", label: "Images" },
    { value: "archives", label: "Archives" },
    { value: "torrents", label: "Torrents" },
    { value: "playlists", label: "Playlists" },
    { value: "other", label: "Other" },
  ];

  function iconFor(entry: LibraryEntry): IconName {
    if (entry.category === "videos") return "video";
    if (entry.category === "music") return "music";
    if (entry.category === "documents") return "document";
    if (entry.category === "images") return "image";
    if (entry.category === "archives") return "archive";
    if (entry.category === "torrents") return "torrent";
    return "file";
  }

  function sourceLabel(source: string): string {
    if (!source) return "Imported file";
    try {
      return new URL(source).hostname || source;
    } catch {
      return source;
    }
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const request = {
        q: search || undefined,
        category: (category || undefined) as LibraryCategory | undefined,
        limit: 250,
      };
      if (mode === "trash") {
        entries = (await connection.client.listLibrary({ ...request, state: "trashed" })).items;
      } else {
        const [active, missing] = await Promise.all([
          connection.client.listLibrary({ ...request, state: "active" }),
          connection.client.listLibrary({ ...request, state: "missing" }),
        ]);
        entries = [...active.items, ...missing.items];
      }
      if (selectedId && !entries.some((entry) => entry.id === selectedId)) selectedId = null;
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  let firstLoad = true;
  $effect(() => {
    search;
    category;
    mode;
    const timer = setTimeout(() => {
      firstLoad = false;
      void load();
    }, firstLoad ? 0 : 220);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    if (typeof localStorage === "undefined") return;
    const stored = localStorage.getItem(SORT_STORAGE_KEY);
    if (stored) {
      try {
        const value = JSON.parse(stored) as { key?: LibrarySortKey; direction?: SortDirection };
        if (value.key) sortKey = value.key;
        if (value.direction) sortDirection = value.direction;
      } catch {
        localStorage.removeItem(SORT_STORAGE_KEY);
      }
    }
  });

  $effect(() => {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(SORT_STORAGE_KEY, JSON.stringify({ key: sortKey, direction: sortDirection }));
  });

  $effect(() => {
    const client = connection.client;
    if (!client) return;
    void client.getLibraryImportStatus().then((status) => {
      importStatus = status.run_id ? status : null;
    }).catch(() => {
      // Import status is secondary to the main Library list.
    });
  });

  $effect(() => {
    if (!importStatus?.running || !connection.client) return;
    const timer = setInterval(async () => {
      try {
        const previousRun = importStatus?.run_id;
        importStatus = await connection.client!.getLibraryImportStatus();
        if (!importStatus.running && importStatus.run_id === previousRun) {
          if (importStatus.cancelled) {
            notifications.info("Library import cancelled", `${importStatus.imported} item(s) were added before cancellation.`);
          } else if (importStatus.truncated) {
            notifications.warning("Library import reached its scan limit", `${importStatus.imported} item(s) added. Increase the scan limit to continue.`);
          } else if (importStatus.errors.length > 0) {
            notifications.warning("Library import completed with warnings", `${importStatus.imported} item(s) added · ${importStatus.errors.length} warning(s)`);
          } else {
            notifications.info(`Library import completed: ${importStatus.imported} item(s) added`);
          }
          void load();
        }
      } catch {
        // The backend import continues; a later poll can recover.
      }
    }, 1500);
    return () => clearInterval(timer);
  });

  async function startImport(): Promise<void> {
    if (!connection.client || !importPath.trim()) return;
    importBusy = true;
    try {
      const maxEntries = Number.parseInt(importMaxEntries, 10);
      const maxDepth = Number.parseInt(importMaxDepth, 10);
      importStatus = await connection.client.startLibraryImport({
        path: importPath.trim(),
        tags: importTags.split(",").map((tag) => tag.trim()).filter(Boolean),
        max_entries: Number.isFinite(maxEntries) ? maxEntries : 100_000,
        max_depth: Number.isFinite(maxDepth) ? maxDepth : 64,
      });
      importOpen = false;
      notifications.info("Library import started");
    } catch (cause) {
      notifications.error("Couldn't start the library import", describeError(cause));
    } finally {
      importBusy = false;
    }
  }

  async function cancelImport(): Promise<void> {
    if (!connection.client || !importStatus?.running || importCancelBusy) return;
    importCancelBusy = true;
    try {
      importStatus = await connection.client.cancelLibraryImport();
      notifications.info("Cancelling Library import", "The current file finishes safely before the scan stops.");
    } catch (cause) {
      notifications.error("Couldn't cancel the Library import", describeError(cause));
    } finally {
      importCancelBusy = false;
    }
  }

  async function verifyLibrary(): Promise<void> {
    if (!connection.client) return;
    try {
      const report = await connection.client.verifyLibrary();
      notifications.info(
        `Verified ${report.checked} item(s)`,
        report.missing ? `${report.missing} missing file(s) found` : "No missing files found",
      );
      await load();
    } catch (cause) {
      notifications.error("Couldn't verify the library", describeError(cause));
    }
  }

  async function restore(entry: LibraryEntry): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.restoreLibraryEntry(entry.id);
      notifications.info("Item restored");
      selectedId = null;
      await load();
    } catch (cause) {
      notifications.error("Couldn't restore the item", describeError(cause));
    }
  }

  async function undoTrash(entry: LibraryEntry): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.restoreLibraryEntry(entry.id);
      notifications.success("Item restored", entry.filename);
      await load();
    } catch (cause) {
      notifications.error("Couldn't restore the item", describeError(cause));
    }
  }

  async function confirmRemove(): Promise<void> {
    if (!connection.client || !removeEntry) return;
    removeBusy = true;
    removeError = null;
    const target = removeEntry;
    try {
      await connection.client.deleteLibraryEntry(target.id, target.state === "trashed" ? "purge" : "trash");
      removeEntry = null;
      selectedId = null;
      if (target.state === "trashed") {
        notifications.info("Item permanently deleted", target.filename);
      } else {
        notifications.push({
          severity: "info",
          title: "Item moved to trash",
          message: target.filename,
          actionLabel: "Undo",
          onAction: () => void undoTrash(target),
        });
      }
      await load();
    } catch (cause) {
      removeError = describeError(cause);
    } finally {
      removeBusy = false;
    }
  }

  async function runPathAction(path: string, action: "open" | "reveal" | "copy"): Promise<void> {
    try {
      if (action === "open") await openNativePath(path);
      else if (action === "reveal") await revealNativePath(path);
      else await navigator.clipboard.writeText(path);
      if (action === "copy") notifications.info("Path copied");
    } catch (cause) {
      notifications.error(
        action === "open" ? "Couldn't open this file" : action === "reveal" ? "Couldn't show this file in Explorer" : "Couldn't copy the path",
        describeError(cause),
      );
    }
  }

  function changeSort(next: LibrarySortKey): void {
    if (sortKey === next) sortDirection = sortDirection === "asc" ? "desc" : "asc";
    else {
      sortKey = next;
      sortDirection = next === "modified" || next === "size" ? "desc" : "asc";
    }
  }

  function sortLabel(key: LibrarySortKey): "none" | "ascending" | "descending" {
    return sortKey === key ? (sortDirection === "asc" ? "ascending" : "descending") : "none";
  }

  function moreItems(): MenuItem[] {
    return [
      { id: "verify", label: "Verify library", icon: "verify", onSelect: () => void verifyLibrary() },
      { id: "relocate", label: "Find moved files", icon: "restore", onSelect: () => (relocationOpen = true) },
      { id: "statistics", label: "Library statistics", icon: "speed", onSelect: () => (statisticsOpen = true) },
      { id: "refresh", label: "Refresh", icon: "refresh", separatorBefore: true, onSelect: () => void load() },
    ];
  }

</script>

<PageScaffold title="Library" summary="Files managed by Ravyn, imported folders, trash, and duplicate groups.">
  {#snippet actions()}
    <Button variant="accent" onclick={() => (importOpen = true)}><Icon name="upload" size={16} /> Import folder</Button>
  {/snippet}

  {#snippet commandBar()}
    <PageCommandBar ariaLabel="Library commands">
      {#snippet leading()}
        <Tabs tabs={viewTabs} bind:selected={mode} />
      {/snippet}
      {#snippet actions()}
        <SearchBox bind:value={search} label="Search library" placeholder="Search files, paths, or sources" />
        <Dropdown options={categoryOptions} bind:value={category} label="Filter by file type" />
        <MenuButton label="More" icon="more" items={moreItems()} variant="subtle" />
      {/snippet}
    </PageCommandBar>
  {/snippet}

  {#snippet status()}
    <div class="status-strip">
      <CompactSummary items={summaryItems} ariaLabel="Library summary" />
      {#if importStatus?.running}
        <span class="import-status" role="status">
          <Icon name="spinner" size={14} />
          {importStatus.cancel_requested ? "Stopping import" : "Importing"} · {importStatus.scanned} scanned · {importStatus.imported} added
        </span>
        <Button variant="subtle" disabled={importCancelBusy || importStatus.cancel_requested} onclick={() => void cancelImport()}>
          <Icon name="cancel" size={14} /> {importCancelBusy || importStatus.cancel_requested ? "Stopping…" : "Cancel import"}
        </Button>
      {/if}
    </div>
  {/snippet}

  <div class="workspace">
    <ListDetailsLayout detailsOpen={!!selected} detailsLabel="Library item details" detailsWidth="390px">
      {#snippet list()}
        <Surface padding="none" class="library-list">
          {#if error}
            <div class="state"><InlineError title="Couldn't load the library" message={error} retry={() => void load()} /></div>
          {:else if loading}
            <div class="state muted">Loading library…</div>
          {:else if mode === "duplicates"}
            {#if duplicateGroups.length === 0}
              <EmptyState icon="copy" title="No duplicate groups" message={search || category ? "No duplicate groups match the current filters." : "Ravyn did not find repeated checksums or matching file names and sizes in the loaded library."} />
            {:else}
              <div class="duplicate-groups">
                {#each duplicateGroups as group (group.key)}
                  <section class="duplicate-group">
                    <header>
                      <div><strong>{group.entries[0]?.filename}</strong><small>{group.entries.length} copies · matched by {group.reason}</small></div>
                      <span>{formatBytes(group.totalBytes)}</span>
                    </header>
                    {#each group.entries as entry (entry.id)}
                      <button type="button" class="duplicate-copy" class:selected={selectedId === entry.id} onclick={() => (selectedId = entry.id)}>
                        <Icon name={iconFor(entry)} size={17} />
                        <span><strong>{entry.path}</strong><small>{formatBytes(entry.size_bytes)} · {formatAbsoluteTime(entry.updated_at)}</small></span>
                        {#if entry.state === "missing"}<StatusBadge label="Missing" severity="warning" icon="warning" />{/if}
                      </button>
                    {/each}
                  </section>
                {/each}
              </div>
            {/if}
          {:else if sortedEntries.length === 0}
            <EmptyState
              icon={mode === "trash" ? "trash" : "library"}
              title={mode === "trash" ? "Trash is empty" : "No files found"}
              message={search || category ? "No library items match the current filters." : mode === "trash" ? "Items moved to trash will appear here until they are restored or permanently deleted." : "Completed and imported files will appear here."}
            >
              {#if mode === "files" && !search && !category}<Button variant="accent" onclick={() => (importOpen = true)}>Import a folder</Button>{/if}
            </EmptyState>
          {:else}
            <div class="table-header" role="row">
              <span role="columnheader" aria-sort={sortLabel("name")}><button type="button" onclick={() => changeSort("name")}>Name <Icon name="sort" size={12} /></button></span>
              <span role="columnheader" aria-sort={sortLabel("type")}><button type="button" onclick={() => changeSort("type")}>Type <Icon name="sort" size={12} /></button></span>
              <span role="columnheader" aria-sort={sortLabel("size")}><button type="button" onclick={() => changeSort("size")}>Size <Icon name="sort" size={12} /></button></span>
              <span role="columnheader" aria-sort={sortLabel("modified")}><button type="button" onclick={() => changeSort("modified")}>Modified <Icon name="sort" size={12} /></button></span>
              <span role="columnheader" aria-sort={sortLabel("source")}><button type="button" onclick={() => changeSort("source")}>Source <Icon name="sort" size={12} /></button></span>
              <span role="columnheader"></span>
            </div>
            <div class="rows" role="listbox" aria-label="Library files">
              {#each sortedEntries as entry (entry.id)}
                <button
                  type="button"
                  class="library-row"
                  class:selected={selectedId === entry.id}
                  role="option"
                  aria-selected={selectedId === entry.id}
                  ondblclick={() => entry.state !== "missing" && void runPathAction(entry.path, "open")}
                  onclick={() => (selectedId = entry.id)}
                >
                  <span class="name-cell">
                    <span class="file-icon"><Icon name={iconFor(entry)} size={18} /></span>
                    <span class="file-copy"><strong>{entry.filename}</strong><small>{entry.path}</small></span>
                  </span>
                  <span class="type-cell">{libraryTypeLabel(entry)}</span>
                  <span>{formatBytes(entry.size_bytes)}</span>
                  <span>{formatAbsoluteTime(entry.updated_at)}</span>
                  <span class="source-cell">{sourceLabel(entry.source_url)}</span>
                  <span class="row-status">
                    {#if entry.state === "missing"}<StatusBadge label="Missing" severity="warning" icon="warning" />
                    {:else if entry.state === "trashed"}<StatusBadge label="Trash" severity="neutral" icon="trash" />{/if}
                  </span>
                </button>
              {/each}
            </div>
          {/if}
        </Surface>
      {/snippet}

      {#snippet details()}
        {#if selected}
          <DetailsPane
            title={selected.filename}
            subtitle={selected.path}
            icon={iconFor(selected)}
            onClose={() => (selectedId = null)}
          >
            <div class="details-stack">
              <div class="details-actions">
                <Button variant="accent" disabled={selected.state === "missing" || selected.state === "trashed"} onclick={() => void runPathAction(selected.path, "open")}><Icon name="external-link" size={16} /> Open</Button>
                <Button disabled={selected.state === "missing"} onclick={() => void runPathAction(selected.path, "reveal")}><Icon name="folder-open" size={16} /> Show in Explorer</Button>
              </div>

              {#if selected.state === "missing"}
                <div class="notice warning"><Icon name="warning" size={17} /><span><strong>File not found</strong><small>Use Find moved files to reconnect this record by checksum.</small></span></div>
              {/if}

              <dl>
                <dt>State</dt><dd>{selected.state === "active" ? "Available" : selected.state === "trashed" ? "In trash" : "Missing"}</dd>
                <dt>Type</dt><dd>{libraryTypeLabel(selected)}</dd>
                <dt>Category</dt><dd>{selected.category}</dd>
                <dt>Size</dt><dd>{formatBytes(selected.size_bytes)}</dd>
                <dt>Modified</dt><dd>{formatAbsoluteTime(selected.updated_at)}</dd>
                <dt>Downloaded</dt><dd>{formatAbsoluteTime(selected.downloaded_at)}</dd>
                <dt>Source</dt><dd class="wrap">{selected.source_url || "Imported file"}</dd>
                <dt>Path</dt><dd class="wrap mono">{selected.path}</dd>
                {#if selected.tags.length}<dt>Tags</dt><dd>{selected.tags.join(", ")}</dd>{/if}
                {#if selected.sha256}<dt>SHA-256</dt><dd class="wrap mono">{selected.sha256}</dd>{/if}
              </dl>

              <div class="secondary-actions">
                <Button variant="subtle" onclick={() => void runPathAction(selected.path, "copy")}><Icon name="copy" size={15} /> Copy path</Button>
                <Button variant="subtle" onclick={() => void verifyLibrary()}><Icon name="verify" size={15} /> Verify library</Button>
                {#if selected.state === "trashed"}
                  <Button variant="subtle" onclick={() => void restore(selected)}><Icon name="restore" size={15} /> Restore</Button>
                {/if}
                <Button variant="subtle" onclick={() => (removeEntry = selected)}><Icon name="trash" size={15} /> {selected.state === "trashed" ? "Delete permanently" : "Move to trash"}</Button>
              </div>
            </div>
          </DetailsPane>
        {/if}
      {/snippet}
    </ListDetailsLayout>
  </div>
</PageScaffold>

<Dialog open={importOpen} title="Import a folder" onClose={() => !importBusy && (importOpen = false)} preventClose={importBusy}>
  <div class="dialog-stack">
    <p>Ravyn scans the folder, classifies supported files, and adds them to the library without moving the originals.</p>
    <PathPicker bind:value={importPath} label="Folder" placeholder="Choose a folder to scan" />
    <TextField bind:value={importTags} label="Tags" placeholder="imported, archive" hint="Optional comma-separated labels applied to every imported file." />
    <details class="advanced">
      <summary>Advanced scan limits</summary>
      <div class="two-column">
        <TextField bind:value={importMaxEntries} label="Maximum entries" inputmode="numeric" hint="Stops safely when the limit is reached." />
        <TextField bind:value={importMaxDepth} label="Maximum folder depth" inputmode="numeric" hint="Symlinks are never followed." />
      </div>
    </details>
  </div>
  {#snippet footer()}
    <Button disabled={importBusy} onclick={() => (importOpen = false)}>Cancel</Button>
    <Button variant="accent" disabled={importBusy || !importPath.trim()} onclick={() => void startImport()}>{importBusy ? "Starting…" : "Start import"}</Button>
  {/snippet}
</Dialog>

<LibraryStatisticsDialog open={statisticsOpen} onClose={() => (statisticsOpen = false)} />

<LibraryRelocationDialog
  open={relocationOpen}
  {missingCount}
  entryCount={entries.length}
  {totalSize}
  onClose={() => (relocationOpen = false)}
  onCompleted={() => void load()}
/>

<ConfirmDialog
  open={!!removeEntry}
  title={removeEntry?.state === "trashed" ? "Delete permanently?" : "Move to trash?"}
  message={removeEntry?.state === "trashed" ? "This deletes the file and its library record. This action cannot be undone." : "The file is moved to Library trash and can be restored with Undo or from the Trash view."}
  confirmLabel={removeEntry?.state === "trashed" ? "Delete permanently" : "Move to trash"}
  destructive
  busy={removeBusy}
  error={removeError}
  onConfirm={() => void confirmRemove()}
  onClose={() => !removeBusy && (removeEntry = null)}
/>

<style>
  .workspace { height: 100%; min-height: 0; padding: 0 var(--page-padding) var(--page-padding); }
  .status-strip { min-height: 38px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: 0 var(--page-padding); border-bottom: 1px solid var(--stroke-divider); }
  .import-status { display: inline-flex; align-items: center; gap: var(--space-2); color: var(--accent-text); font-size: var(--text-caption); }
  :global(.library-list) { height: 100%; min-height: 0; display: flex; flex-direction: column; border-radius: 0; border-color: var(--stroke-divider); background: var(--surface-content); }
  .state { padding: var(--space-6); }
  .muted { color: var(--text-secondary); }
  .table-header, .library-row { display: grid; grid-template-columns: minmax(250px, 2fr) minmax(110px, .8fr) 90px 150px minmax(120px, .8fr) auto; align-items: center; column-gap: var(--space-3); }
  .table-header { min-height: 36px; padding: 0 var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .table-header button { display: inline-flex; align-items: center; gap: 4px; min-width: 0; padding: 0; border: 0; color: inherit; background: transparent; font: inherit; text-align: left; }
  .rows { flex: 1; min-height: 0; overflow: auto; }
  .library-row { width: 100%; min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); color: var(--text-primary); background: transparent; text-align: left; cursor: default; }
  .library-row:hover, .duplicate-copy:hover { background: var(--bg-subtle-hover); }
  .library-row.selected, .duplicate-copy.selected { background: color-mix(in srgb, var(--accent-subtle) 52%, transparent); box-shadow: inset 2px 0 var(--accent-default); }
  .name-cell { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .file-icon { width: 30px; height: 30px; flex: none; display: grid; place-items: center; color: var(--text-secondary); }
  .file-copy { display: flex; flex-direction: column; min-width: 0; }
  .file-copy strong, .file-copy small, .source-cell, .type-cell { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-copy strong { font-weight: 500; }
  .file-copy small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .type-cell, .source-cell { color: var(--text-secondary); }
  .row-status { justify-self: end; }
  .duplicate-groups { flex: 1; min-height: 0; overflow: auto; padding: var(--space-3); }
  .duplicate-group { margin-bottom: var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .duplicate-group > header { min-height: 52px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: 0 var(--space-2); }
  .duplicate-group > header div, .duplicate-copy span { min-width: 0; display: flex; flex-direction: column; }
  .duplicate-group small, .duplicate-copy small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .duplicate-copy { width: 100%; min-height: 50px; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-2); border: 0; border-top: 1px solid var(--stroke-divider); color: var(--text-primary); background: transparent; text-align: left; }
  .duplicate-copy strong, .duplicate-copy small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .details-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .details-actions, .secondary-actions { display: flex; flex-wrap: wrap; gap: var(--space-2); }
  .secondary-actions { padding-top: var(--space-3); border-top: 1px solid var(--stroke-divider); }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-4); margin: 0; }
  dt { color: var(--text-secondary); }
  dd { min-width: 0; margin: 0; }
  .wrap { word-break: break-word; }
  .mono { font: 12px/18px Consolas, ui-monospace, monospace; }
  .notice { display: flex; align-items: flex-start; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); }
  .notice span { display: flex; flex-direction: column; }
  .notice small { color: inherit; opacity: .85; }
  .notice.warning { color: var(--status-warning); background: var(--status-warning-bg); }
  .dialog-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .dialog-stack p { margin: 0; color: var(--text-secondary); }
  .advanced summary { cursor: default; font-weight: 600; }
  .two-column { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); padding-top: var(--space-3); }
  @media (max-width: 1180px) {
    .table-header, .library-row { grid-template-columns: minmax(230px, 2fr) 110px 90px 140px auto; }
    .table-header button:nth-child(5), .library-row > span:nth-child(5) { display: none; }
  }
  @media (max-width: 820px) {
    .two-column { grid-template-columns: 1fr; }
    .status-strip { align-items: flex-start; flex-direction: column; justify-content: center; padding-block: var(--space-2); }
    .table-header { display: none; }
    .library-row { grid-template-columns: minmax(0, 1fr) auto; }
    .library-row > span:nth-child(2), .library-row > span:nth-child(3), .library-row > span:nth-child(4), .library-row > span:nth-child(5) { display: none; }
  }
</style>
