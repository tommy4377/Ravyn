<script lang="ts">
  import { describeError } from "../api/errors";
  import type { CreateJob, DuplicatePolicy, ImportResult, JobKind } from "../api/types";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextArea from "../components/TextArea.svelte";
  import TextField from "../components/TextField.svelte";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { analyzeBatchInput } from "./batchImport";

  let {
    open,
    initialText = "",
    onClose,
  }: {
    open: boolean;
    initialText?: string;
    onClose: () => void;
  } = $props();

  let text = $state("");
  let fileName = $state("");
  let destination = $state("");
  let kind = $state<JobKind>("http");
  let duplicatePolicy = $state<DuplicatePolicy>("reuse_existing");
  let priority = $state("0");
  let speedLimitMbps = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let result = $state<ImportResult | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);

  const kindOptions: DropdownOption[] = [
    { value: "http", label: "Direct downloads" },
    { value: "media", label: "Video or audio" },
    { value: "torrent", label: "Torrent or magnet" },
  ];
  const duplicateOptions: DropdownOption[] = [
    { value: "reuse_existing", label: "Reuse identical downloads" },
    { value: "allow", label: "Allow duplicates" },
    { value: "skip", label: "Skip duplicates" },
    { value: "reject", label: "Reject duplicates" },
    { value: "overwrite", label: "Replace existing files" },
  ];
  const analysis = $derived(analyzeBatchInput(text));
  const uniqueCount = $derived(analysis.uniqueLines.length);
  const duplicateCount = $derived(analysis.duplicateCount);
  const jsonBatch = $derived<CreateJob[] | null>(analysis.jsonBatch);
  const itemCount = $derived(analysis.itemCount);

  $effect(() => {
    if (!open) return;
    if (initialText && !text) text = initialText;
    error = null;
  });

  function reset(): void {
    text = "";
    fileName = "";
    destination = "";
    kind = "http";
    duplicatePolicy = "reuse_existing";
    priority = "0";
    speedLimitMbps = "";
    error = null;
    result = null;
  }

  function close(): void {
    if (busy) return;
    reset();
    onClose();
  }

  function chooseFile(): void {
    fileInput?.click();
  }

  function readFile(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      text = typeof reader.result === "string" ? reader.result : "";
      fileName = file.name;
      result = null;
      error = null;
    };
    reader.onerror = () => {
      error = "The selected batch file could not be read.";
    };
    reader.readAsText(file);
    input.value = "";
  }

  async function submit(): Promise<void> {
    if (!connection.client || itemCount === 0 || busy) return;
    busy = true;
    error = null;
    result = null;
    const parsedPriority = Number.parseInt(priority, 10);
    const parsedSpeed = Number.parseFloat(speedLimitMbps.replace(",", "."));
    try {
      result = jsonBatch
        ? await connection.client.createBatchJobs(jsonBatch)
        : await connection.client.importJobsText({
            text,
            defaults: {
              kind,
              destination: destination.trim() || null,
              priority: Number.isFinite(parsedPriority) ? parsedPriority : 0,
              speed_limit_bps: Number.isFinite(parsedSpeed) && parsedSpeed > 0 ? Math.round(parsedSpeed * 125_000) : null,
              duplicate_policy: duplicatePolicy,
            },
          });
      await jobsStore.refreshAll();
      if (result.rejected === 0 && !result.truncated) {
        notifications.success("Batch imported", `${result.accepted} download${result.accepted === 1 ? "" : "s"} added.`);
        close();
      } else {
        notifications.warning("Batch imported with issues", `${result.accepted} added · ${result.rejected} rejected`);
      }
    } catch (cause) {
      error = describeError(cause);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog {open} title="Import batch" onClose={close} preventClose={busy}>
  <div class="form">
    <div class="intro">
      <span class="intro-icon"><Icon name="basket" size={20} /></span>
      <div>
        <strong>Add many downloads safely</strong>
        <span>Use one source per line, or import a JSON array of complete job requests.</span>
      </div>
    </div>

    <input bind:this={fileInput} class="file-input" type="file" accept=".txt,.urls,.json,text/plain,application/json" onchange={readFile} />
    <div class="file-row">
      <Button onclick={chooseFile}><Icon name="folder-open" size={15} /> Choose batch file</Button>
      {#if fileName}<span title={fileName}>{fileName}</span>{/if}
    </div>

    <TextArea bind:value={text} label="Sources" rows={10} placeholder={"https://example.com/file.zip\nhttps://example.com/video"} hint="Blank lines and lines beginning with # or // are ignored." />
    <div class="summary" role="status">
      <span><strong>{itemCount}</strong> unique item{itemCount === 1 ? "" : "s"}</span>
      {#if duplicateCount}<span>{duplicateCount} duplicate line{duplicateCount === 1 ? "" : "s"} ignored</span>{/if}
      {#if jsonBatch}<span>JSON job batch detected</span>{/if}
    </div>

    {#if !jsonBatch}
      <PathPicker bind:value={destination} label="Destination" placeholder="Use the Library default" />
      <div class="two-column">
        <div class="dropdown-field"><label for="batch-kind">Download type</label><Dropdown id="batch-kind" options={kindOptions} bind:value={kind} label="Download type" /></div>
        <div class="dropdown-field"><label for="batch-duplicates">Duplicates</label><Dropdown id="batch-duplicates" options={duplicateOptions} bind:value={duplicatePolicy} label="Duplicate handling" /></div>
      </div>
      <details class="advanced">
        <summary>Advanced options</summary>
        <div class="advanced-body two-column">
          <TextField bind:value={priority} label="Priority" inputmode="numeric" />
          <TextField bind:value={speedLimitMbps} label="Speed limit (Mbit/s)" inputmode="decimal" placeholder="Unlimited" />
        </div>
      </details>
    {/if}

    {#if result && (result.rejected > 0 || result.truncated)}
      <section class="result-panel" aria-label="Batch import results">
        <strong>{result.accepted} added · {result.rejected} rejected{result.truncated ? " · input limit reached" : ""}</strong>
        <ul>
          {#each result.items.filter((item) => item.error).slice(0, 50) as item}
            <li><span title={item.source}>{item.source}</span><small>{item.error}</small></li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if error}<InlineError title="Couldn't import this batch" message={error} />{/if}
  </div>

  {#snippet footer()}
    <Button disabled={busy} onclick={close}>Cancel</Button>
    <Button variant="accent" disabled={busy || itemCount === 0} onclick={() => void submit()}>{busy ? "Importing…" : `Import ${itemCount || "batch"}`}</Button>
  {/snippet}
</Dialog>

<style>
  .form { display: flex; flex-direction: column; gap: var(--space-4); }
  .intro { min-height: 62px; display: flex; align-items: center; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--stroke-divider); border-radius: var(--radius-layer); background: var(--bg-subtle); }
  .intro-icon { width: 38px; height: 38px; flex: none; display: grid; place-items: center; color: var(--accent-text); }
  .intro > div { min-width: 0; display: flex; flex-direction: column; }
  .intro span:last-child, .file-row span { overflow: hidden; color: var(--text-secondary); font-size: var(--text-caption); text-overflow: ellipsis; white-space: nowrap; }
  .file-input { display: none; }
  .file-row { min-width: 0; display: flex; align-items: center; gap: var(--space-3); }
  .file-row span { min-width: 0; }
  .summary { display: flex; flex-wrap: wrap; gap: var(--space-2) var(--space-5); margin-top: calc(var(--space-2) * -1); color: var(--text-secondary); font-size: var(--text-caption); }
  .summary strong { color: var(--text-primary); }
  .two-column { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  .dropdown-field { display: flex; flex-direction: column; gap: var(--space-1); }
  .dropdown-field :global(.dropdown), .dropdown-field :global(select) { width: 100%; }
  .advanced summary { cursor: default; font-weight: 600; }
  .advanced-body { padding-top: var(--space-3); }
  .result-panel { max-height: 230px; display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--status-warning); border-radius: var(--radius-layer); background: var(--bg-subtle); }
  .result-panel ul { min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
  .result-panel li { display: flex; flex-direction: column; }
  .result-panel li span, .result-panel li small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .result-panel li small { color: var(--status-error); }
  @media (max-width: 620px) { .two-column { grid-template-columns: 1fr; } }
</style>
