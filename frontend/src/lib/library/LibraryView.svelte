<script lang="ts">
  import { describeError } from "../api/errors";
  import type { CleanupPolicies, CleanupReport, DuplicateCandidate, LibraryCategory, LibraryEntry, LibraryEntryState, LibraryImportStatus } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import TextField from "../components/TextField.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes } from "../util/format";

  let entries = $state<LibraryEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let search = $state("");
  let category = $state("");
  let stateFilter = $state("active");
  let importOpen = $state(false);
  let importPath = $state("");
  let importBusy = $state(false);
  let importStatus = $state<LibraryImportStatus | null>(null);
  let removeEntry = $state<LibraryEntry | null>(null);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let duplicateOpen = $state(false);
  let duplicates = $state<DuplicateCandidate[]>([]);
  let duplicateLoading = $state(false);
  let duplicateError = $state<string | null>(null);
  let cleanupOpen = $state(false);
  let cleanupPolicies = $state<CleanupPolicies>({ temporary_max_age_days: 7, trash_retention_days: 30, log_retention_days: 90, cache_retention_days: 30 });
  let cleanupTemporaryDays = $state("7");
  let cleanupTrashDays = $state("30");
  let cleanupLogDays = $state("90");
  let cleanupCacheDays = $state("30");
  let cleanupBusy = $state(false);
  let cleanupError = $state<string | null>(null);
  let cleanupReport = $state<CleanupReport | null>(null);

  const selected = $derived(entries.find((entry) => entry.id === selectedId) ?? null);
  const totalSize = $derived(entries.reduce((sum, entry) => sum + (entry.size_bytes ?? 0), 0));
  const missingCount = $derived(entries.filter((entry) => entry.state === "missing").length);

  const categoryOptions: DropdownOption[] = [
    { value: "", label: "All categories" },
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
  const stateOptions: DropdownOption[] = [
    { value: "active", label: "Available" },
    { value: "trashed", label: "Trash" },
    { value: "missing", label: "Missing" },
    { value: "", label: "All states" },
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

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const page = await connection.client.listLibrary({
        q: search || undefined,
        category: (category || undefined) as LibraryCategory | undefined,
        state: (stateFilter || undefined) as LibraryEntryState | undefined,
        limit: 250,
      });
      entries = page.items;
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
    stateFilter;
    const timer = setTimeout(() => {
      firstLoad = false;
      void load();
    }, firstLoad ? 0 : 250);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    if (!importStatus?.running || !connection.client) return;
    const timer = setInterval(async () => {
      try {
        importStatus = await connection.client!.getLibraryImportStatus();
        if (!importStatus.running) {
          notifications.info(`Library import completed: ${importStatus.imported} item(s) added`);
          void load();
        }
      } catch {
        // The import keeps running in the backend; the next poll can recover.
      }
    }, 1500);
    return () => clearInterval(timer);
  });

  async function startImport(): Promise<void> {
    if (!connection.client || !importPath.trim()) return;
    importBusy = true;
    try {
      importStatus = await connection.client.startLibraryImport({ path: importPath.trim() });
      importOpen = false;
      notifications.info("Library import started");
    } catch (cause) {
      notifications.error("Couldn't start the library import", describeError(cause));
    } finally {
      importBusy = false;
    }
  }

  async function verifyLibrary(): Promise<void> {
    if (!connection.client) return;
    try {
      const report = await connection.client.verifyLibrary();
      notifications.info(`Verified ${report.checked} item(s)`, report.missing ? `${report.missing} missing file(s) found` : "No missing files found");
      await load();
    } catch (cause) {
      notifications.error("Couldn't verify the library", describeError(cause));
    }
  }

  async function confirmRemove(): Promise<void> {
    if (!connection.client || !removeEntry) return;
    removeBusy = true;
    removeError = null;
    try {
      await connection.client.deleteLibraryEntry(removeEntry.id, removeEntry.state === "trashed" ? "purge" : "trash");
      notifications.info(removeEntry.state === "trashed" ? "Item permanently deleted" : "Item moved to trash");
      removeEntry = null;
      selectedId = null;
      await load();
    } catch (cause) {
      removeError = describeError(cause);
    } finally {
      removeBusy = false;
    }
  }

  async function restore(entry: LibraryEntry): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.restoreLibraryEntry(entry.id);
      notifications.info("Item restored");
      await load();
    } catch (cause) {
      notifications.error("Couldn't restore the item", describeError(cause));
    }
  }

  async function copyPath(path: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(path);
      notifications.info("Path copied");
    } catch {
      notifications.warning("Couldn't copy the path");
    }
  }

  async function findDuplicates(entry: LibraryEntry): Promise<void> {
    if (!connection.client) return;
    duplicateOpen = true;
    duplicateLoading = true;
    duplicateError = null;
    duplicates = [];
    try {
      const candidates = await connection.client.findLibraryDuplicates({
        sha256: entry.sha256 ?? undefined,
        size_bytes: entry.size_bytes ?? undefined,
        filename: entry.filename,
        limit: 50,
      });
      duplicates = candidates.filter((candidate) => candidate.entry.id !== entry.id);
    } catch (cause) {
      duplicateError = describeError(cause);
    } finally {
      duplicateLoading = false;
    }
  }

  async function openCleanup(): Promise<void> {
    if (!connection.client) return;
    cleanupOpen = true;
    cleanupError = null;
    cleanupReport = null;
    try {
      cleanupPolicies = await connection.client.getCleanupPolicies();
      cleanupTemporaryDays = String(cleanupPolicies.temporary_max_age_days);
      cleanupTrashDays = String(cleanupPolicies.trash_retention_days);
      cleanupLogDays = String(cleanupPolicies.log_retention_days);
      cleanupCacheDays = String(cleanupPolicies.cache_retention_days);
    } catch (cause) {
      cleanupError = describeError(cause);
    }
  }

  async function saveAndRunCleanup(): Promise<void> {
    if (!connection.client || cleanupBusy) return;
    cleanupBusy = true;
    cleanupError = null;
    try {
      const normalized: CleanupPolicies = {
        temporary_max_age_days: Math.max(1, Math.round(Number(cleanupTemporaryDays) || 7)),
        trash_retention_days: Math.max(1, Math.round(Number(cleanupTrashDays) || 30)),
        log_retention_days: Math.max(1, Math.round(Number(cleanupLogDays) || 90)),
        cache_retention_days: Math.max(1, Math.round(Number(cleanupCacheDays) || 30)),
      };
      cleanupPolicies = await connection.client.updateCleanupPolicies(normalized);
      cleanupReport = await connection.client.runLibraryCleanup();
      notifications.success("Library cleanup complete", `${cleanupReport.trash_entries_purged} trash item(s) purged · ${formatBytes(cleanupReport.temporary_bytes_removed + cleanupReport.cache_bytes_removed)} freed`);
      await load();
    } catch (cause) {
      cleanupError = describeError(cause);
    } finally {
      cleanupBusy = false;
    }
  }
</script>

<div class="page">
  <PageHeader
    title="Library"
    description={`${entries.length} visible item${entries.length === 1 ? "" : "s"} · ${formatBytes(totalSize)}`}
  >
    {#snippet actions()}
      <Button onclick={() => void openCleanup()}><Icon name="wrench" size={16} /> Clean up</Button>
      <Button onclick={() => void verifyLibrary()}><Icon name="verify" size={16} /> Verify</Button>
      <Button variant="accent" onclick={() => (importOpen = true)}><Icon name="upload" size={16} /> Import folder</Button>
    {/snippet}
  </PageHeader>

  <div class="toolbar">
    <SearchBox bind:value={search} label="Search library" placeholder="Search files, tags, or source" />
    <Dropdown options={categoryOptions} bind:value={category} label="Filter by category" />
    <Dropdown options={stateOptions} bind:value={stateFilter} label="Filter by state" />
    <IconButton icon="refresh" label="Refresh library" onclick={() => void load()} />
  </div>

  {#if importStatus?.running}
    <div class="import-banner">
      <Icon name="spinner" size={16} />
      <span>Importing {importStatus.root ?? "folder"} — {importStatus.scanned} scanned, {importStatus.imported} added</span>
    </div>
  {/if}

  <div class="workspace" class:with-details={!!selected}>
    <Surface padding="none" class="list-surface">
      {#if error}
        <div class="state"><InlineError title="Couldn't load the library" message={error} retry={() => void load()} /></div>
      {:else if loading}
        <div class="state muted">Loading library…</div>
      {:else if entries.length === 0}
        <EmptyState icon="library" title="Nothing here" message={search || category ? "No library items match the current filters." : "Completed and imported files will appear here."}>
          {#if !search && !category}<Button variant="accent" onclick={() => (importOpen = true)}>Import a folder</Button>{/if}
        </EmptyState>
      {:else}
        <div class="table-header" aria-hidden="true">
          <span>Name</span><span>Category</span><span>Size</span><span>Added</span><span></span>
        </div>
        <div class="rows" role="listbox" aria-label="Library items">
          {#each entries as entry (entry.id)}
            <button
              type="button"
              class="library-row"
              class:selected={selectedId === entry.id}
              role="option"
              aria-selected={selectedId === entry.id}
              onclick={() => (selectedId = entry.id)}
            >
              <span class="name-cell">
                <span class="file-icon"><Icon name={iconFor(entry)} size={19} /></span>
                <span class="file-copy"><strong>{entry.filename}</strong><small>{entry.path}</small></span>
              </span>
              <span class="category-cell">{entry.category}</span>
              <span>{formatBytes(entry.size_bytes)}</span>
              <span>{formatAbsoluteTime(entry.downloaded_at)}</span>
              <span class="row-status">
                {#if entry.state === "missing"}<StatusBadge label="Missing" severity="warning" icon="warning" />
                {:else if entry.state === "trashed"}<StatusBadge label="Trash" severity="neutral" icon="trash" />{/if}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </Surface>

    {#if selected}
      <aside class="details">
        <header><div><span class="detail-icon"><Icon name={iconFor(selected)} size={22} /></span><h2>{selected.filename}</h2></div><IconButton icon="close" label="Close details" variant="subtle" onclick={() => (selectedId = null)} /></header>
        <div class="details-body">
          <dl>
            <dt>State</dt><dd>{selected.state}</dd>
            <dt>Category</dt><dd>{selected.category}</dd>
            <dt>Size</dt><dd>{formatBytes(selected.size_bytes)}</dd>
            <dt>Type</dt><dd>{selected.mime_type ?? "Unknown"}</dd>
            <dt>Imported</dt><dd>{selected.imported ? "Yes" : "No"}</dd>
            <dt>Downloaded</dt><dd>{formatAbsoluteTime(selected.downloaded_at)}</dd>
            <dt>Path</dt><dd class="wrap">{selected.path}</dd>
            {#if selected.tags.length}<dt>Tags</dt><dd>{selected.tags.join(", ")}</dd>{/if}
            {#if selected.sha256}<dt>SHA-256</dt><dd class="wrap mono">{selected.sha256}</dd>{/if}
          </dl>
          <div class="detail-actions">
            <Button onclick={() => void copyPath(selected.path)}><Icon name="paste" size={16} /> Copy path</Button>
            <Button onclick={() => void findDuplicates(selected)}><Icon name="copy" size={16} /> Find duplicates</Button>
            {#if selected.state === "trashed"}
              <Button variant="accent" onclick={() => void restore(selected)}><Icon name="restore" size={16} /> Restore</Button>
            {/if}
            <Button onclick={() => (removeEntry = selected)}><Icon name="trash" size={16} /> {selected.state === "trashed" ? "Delete permanently" : "Move to trash"}</Button>
          </div>
        </div>
      </aside>
    {/if}
  </div>
</div>

<Dialog open={importOpen} title="Import a folder" onClose={() => !importBusy && (importOpen = false)} preventClose={importBusy}>
  <div class="dialog-stack">
    <p>Ravyn scans the folder safely, classifies supported files, and adds them to the library without moving the originals.</p>
    <PathPicker bind:value={importPath} label="Folder" placeholder="Choose a folder to scan" />
  </div>
  {#snippet footer()}
    <Button disabled={importBusy} onclick={() => (importOpen = false)}>Cancel</Button>
    <Button variant="accent" disabled={importBusy || !importPath.trim()} onclick={() => void startImport()}>{importBusy ? "Starting…" : "Start import"}</Button>
  {/snippet}
</Dialog>

<Dialog open={duplicateOpen} title="Possible duplicates" size="large" onClose={() => (duplicateOpen = false)}>
  {#if duplicateError}
    <InlineError title="Couldn't find duplicates" message={duplicateError} />
  {:else if duplicateLoading}
    <p class="muted">Comparing library records…</p>
  {:else if duplicates.length === 0}
    <EmptyState icon="copy" title="No other matches" message="No other library entries match the selected file name, size, or checksum." />
  {:else}
    <div class="duplicate-list">
      {#each duplicates as candidate (candidate.entry.id)}
        <div class="duplicate-row"><span class="file-icon"><Icon name={iconFor(candidate.entry)} size={18} /></span><span><strong>{candidate.entry.filename}</strong><small>{candidate.entry.path}</small></span><span>{formatBytes(candidate.entry.size_bytes)}</span><span class="match-list">{candidate.matches.join(", ")}</span></div>
      {/each}
    </div>
  {/if}
  {#snippet footer()}<Button variant="accent" onclick={() => (duplicateOpen = false)}>Done</Button>{/snippet}
</Dialog>

<Dialog open={cleanupOpen} title="Library cleanup" size="medium" preventClose={cleanupBusy} onClose={() => !cleanupBusy && (cleanupOpen = false)}>
  <div class="dialog-stack">
    <p>Configure retention periods and remove expired temporary, cache, trash, and log data. Active library files are never deleted by this operation.</p>
    <div class="cleanup-grid">
      <TextField bind:value={cleanupTemporaryDays} inputmode="numeric" label="Temporary files (days)" />
      <TextField bind:value={cleanupCacheDays} inputmode="numeric" label="Cache files (days)" />
      <TextField bind:value={cleanupTrashDays} inputmode="numeric" label="Trash retention (days)" />
      <TextField bind:value={cleanupLogDays} inputmode="numeric" label="Log retention (days)" />
    </div>
    {#if cleanupReport}<div class="cleanup-result"><Icon name="check-circle" size={18} /><span><strong>Cleanup complete</strong><small>{cleanupReport.temporary_files_removed} temporary file(s), {cleanupReport.cache_files_removed} cache file(s), {cleanupReport.trash_entries_purged} trash item(s), and {cleanupReport.job_logs_removed} log record(s) removed.</small></span></div>{/if}
    {#if cleanupError}<InlineError title="Couldn't run cleanup" message={cleanupError} />{/if}
  </div>
  {#snippet footer()}<Button disabled={cleanupBusy} onclick={() => (cleanupOpen = false)}>Close</Button><Button variant="accent" disabled={cleanupBusy} onclick={() => void saveAndRunCleanup()}>{cleanupBusy ? "Cleaning…" : "Save and clean now"}</Button>{/snippet}
</Dialog>

<ConfirmDialog
  open={!!removeEntry}
  title={removeEntry?.state === "trashed" ? "Delete permanently?" : "Move to trash?"}
  message={removeEntry?.state === "trashed" ? "This removes the file and its library record. This action cannot be undone." : "The item can be restored later from the Library trash filter."}
  confirmLabel={removeEntry?.state === "trashed" ? "Delete permanently" : "Move to trash"}
  destructive
  busy={removeBusy}
  error={removeError}
  onConfirm={() => void confirmRemove()}
  onClose={() => !removeBusy && (removeEntry = null)}
/>

<style>
  .page { height: 100%; display: flex; flex-direction: column; min-width: 0; }
  .toolbar { display: flex; gap: var(--space-2); padding: 0 var(--page-padding) var(--space-4); flex-wrap: wrap; }
  .toolbar :global(.search-box) { flex: 1; max-width: 520px; }
  .import-banner { display: flex; align-items: center; gap: var(--space-2); margin: 0 var(--page-padding) var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--accent-border); border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  .workspace { display: grid; grid-template-columns: minmax(0, 1fr); flex: 1; min-height: 0; gap: var(--space-3); padding: 0 var(--page-padding) var(--page-padding); }
  .workspace.with-details { grid-template-columns: minmax(0, 1fr) minmax(300px, 370px); }
  :global(.list-surface) { min-height: 0; display: flex; flex-direction: column; }
  .state { padding: var(--space-6); }
  .muted { color: var(--text-secondary); }
  .table-header, .library-row { display: grid; grid-template-columns: minmax(240px, 2fr) minmax(100px, .7fr) 90px 150px minmax(0, auto); align-items: center; column-gap: var(--space-3); }
  .table-header { min-height: 36px; padding: 0 var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .rows { min-height: 0; overflow: auto; }
  .library-row { width: 100%; min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); color: var(--text-primary); background: transparent; text-align: left; cursor: default; }
  .library-row:hover { background: var(--bg-subtle-hover); }
  .library-row.selected { background: color-mix(in srgb, var(--accent-subtle) 54%, transparent); box-shadow: inset 2px 0 var(--accent-default); }
  .name-cell { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .file-icon, .detail-icon { display: grid; place-items: center; width: 34px; height: 34px; flex: none; border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  .file-copy { display: flex; flex-direction: column; min-width: 0; }
  .file-copy strong, .file-copy small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-copy strong { font-weight: 500; }
  .file-copy small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .category-cell { text-transform: capitalize; color: var(--text-secondary); }
  .row-status { justify-self: end; }
  .details { min-width: 0; overflow: hidden; border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--surface-card); box-shadow: var(--shadow-card); }
  .details header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .details header > div { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .details h2 { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-body-strong); }
  .details-body { height: calc(100% - 67px); overflow: auto; padding: var(--space-4); }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-4); margin: 0; }
  dt { color: var(--text-secondary); }
  dd { margin: 0; }
  .wrap { word-break: break-word; }
  .mono { font: 12px/18px Consolas, monospace; }
  .detail-actions { display: flex; flex-wrap: wrap; gap: var(--space-2); margin-top: var(--space-5); }
  .dialog-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .dialog-stack p { margin: 0; color: var(--text-secondary); }
  .duplicate-list { display: flex; flex-direction: column; max-height: 430px; overflow: auto; }
  .duplicate-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); min-height: 56px; padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .duplicate-row > span:nth-child(2) { display: flex; min-width: 0; flex-direction: column; }
  .duplicate-row strong, .duplicate-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .duplicate-row small, .match-list { color: var(--text-tertiary); font-size: var(--text-caption); }
  .match-list { padding: 3px 7px; border-radius: var(--radius-pill); background: var(--accent-subtle); color: var(--accent-text); }
  .cleanup-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  .cleanup-result { display: flex; align-items: flex-start; gap: var(--space-3); padding: var(--space-3); border-radius: var(--radius-medium); color: var(--status-success); background: var(--status-success-bg); }
  .cleanup-result > span { display: flex; flex-direction: column; }
  .cleanup-result small { color: inherit; }
  @media (max-width: 1120px) { .workspace.with-details { grid-template-columns: minmax(0, 1fr) 320px; } .table-header, .library-row { grid-template-columns: minmax(220px, 2fr) 100px 90px minmax(0, auto); } .table-header span:nth-child(4), .library-row > span:nth-child(4) { display: none; } }
  @media (max-width: 800px) { .workspace.with-details { grid-template-columns: minmax(0, 1fr); } .details { position: absolute; inset: 92px var(--page-padding) var(--page-padding); z-index: 20; backdrop-filter: blur(30px); } .table-header { display: none; } .library-row { grid-template-columns: minmax(0, 1fr) auto; } .library-row > span:nth-child(2), .library-row > span:nth-child(3), .library-row > span:nth-child(4) { display: none; } }
</style>
