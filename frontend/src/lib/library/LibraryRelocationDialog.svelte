<script lang="ts">
  import { describeError } from "../api/errors";
  import type { RelocationReport } from "../api/types";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextField from "../components/TextField.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatBytes } from "../util/format";

  let {
    open,
    missingCount,
    entryCount,
    totalSize,
    onClose,
    onCompleted,
  }: {
    open: boolean;
    missingCount: number;
    entryCount: number;
    totalSize: number;
    onClose: () => void;
    onCompleted: () => void;
  } = $props();

  let searchRoot = $state("");
  let currentRoot = $state<string | null>(null);
  let maxEntries = $state("10000");
  let maxDepth = $state("12");
  let busy = $state(false);
  let loadingSettings = $state(false);
  let error = $state<string | null>(null);
  let report = $state<RelocationReport | null>(null);

  $effect(() => {
    if (!open || !connection.client) return;
    report = null;
    error = null;
    loadingSettings = true;
    void connection.client.getSettings()
      .then((response) => {
        currentRoot = response.values.library_root ?? response.values.download_dir;
        if (!searchRoot) searchRoot = currentRoot ?? "";
      })
      .catch((cause) => {
        error = describeError(cause);
      })
      .finally(() => {
        loadingSettings = false;
      });
  });

  async function run(): Promise<void> {
    if (!connection.client || busy || !searchRoot.trim()) return;
    busy = true;
    error = null;
    report = null;
    try {
      report = await connection.client.relocateLibrary({
        path: searchRoot.trim(),
        max_entries: Math.max(1, Math.round(Number(maxEntries) || 10000)),
        max_depth: Math.max(1, Math.round(Number(maxDepth) || 12)),
      });
      const message = report.repaired > 0
        ? `${report.repaired} missing item${report.repaired === 1 ? "" : "s"} repaired.`
        : "No missing library records could be matched.";
      notifications.success("Moved-file scan complete", message);
      onCompleted();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog {open} title="Find moved files" size="medium" preventClose={busy} onClose={() => !busy && onClose()}>
  <div class="dialog-stack">
    <div class="explanation">
      <Icon name="restore" size={20} />
      <div>
        <strong>Repair missing library records</strong>
        <p>Ravyn scans a folder, compares checksums, and reconnects missing records to files that were moved outside the app.</p>
      </div>
    </div>

    <div class="summary" aria-label="Library relocation summary">
      <span><strong>{missingCount}</strong> missing</span>
      <span><strong>{entryCount}</strong> visible items</span>
      <span><strong>{formatBytes(totalSize)}</strong> indexed</span>
    </div>

    <PathPicker
      bind:value={searchRoot}
      label="Search folder"
      placeholder={loadingSettings ? "Loading current library folder…" : "Choose the folder that contains the moved files"}
      hint={currentRoot ? `Current library root: ${currentRoot}` : "Only regular files are scanned. Symbolic links are skipped."}
    />

    <div class="limits">
      <TextField bind:value={maxEntries} inputmode="numeric" label="Maximum entries" hint="Stops the scan after this many filesystem entries." />
      <TextField bind:value={maxDepth} inputmode="numeric" label="Maximum depth" hint="Limits how deeply Ravyn scans nested folders." />
    </div>

    {#if report}
      <div class="result" data-success={report.repaired > 0}>
        <Icon name={report.repaired > 0 ? "check-circle" : "info"} size={18} />
        <div>
          <strong>{report.repaired > 0 ? "Files reconnected" : "Scan completed"}</strong>
          <p>{report.scanned} files checked · {report.repaired} repaired · {report.unmatched} still unmatched</p>
        </div>
      </div>
    {/if}

    {#if error}<InlineError title="Couldn't scan for moved files" message={error} />{/if}
  </div>

  {#snippet footer()}
    <Button disabled={busy} onclick={onClose}>{report ? "Done" : "Cancel"}</Button>
    <Button variant="accent" disabled={busy || !searchRoot.trim()} onclick={() => void run()}>
      {busy ? "Scanning…" : report ? "Scan again" : "Start scan"}
    </Button>
  {/snippet}
</Dialog>

<style>
  .dialog-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .explanation, .result { display: flex; align-items: flex-start; gap: var(--space-3); }
  .explanation strong, .result strong { display: block; }
  p { margin: 3px 0 0; color: var(--text-secondary); }
  .summary { display: flex; flex-wrap: wrap; gap: var(--space-2) var(--space-4); padding: var(--space-3) 0; border-block: 1px solid var(--stroke-divider); color: var(--text-secondary); font-size: var(--text-caption); }
  .summary span { display: inline-flex; gap: 4px; }
  .summary strong { color: var(--text-primary); }
  .limits { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-3); }
  .result { padding: var(--space-3); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .result[data-success="true"] { color: var(--status-success); background: var(--status-success-bg); border-color: color-mix(in srgb, var(--status-success) 28%, transparent); }
  .result p { color: inherit; opacity: .86; }
  @media (max-width: 620px) { .limits { grid-template-columns: 1fr; } }
</style>
