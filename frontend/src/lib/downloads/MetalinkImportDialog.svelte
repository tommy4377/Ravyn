<script lang="ts">
  import { describeError } from "../api/errors";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextArea from "../components/TextArea.svelte";
  import TextField from "../components/TextField.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";

  let {
    open,
    initialDocument = "",
    onClose,
  }: {
    open: boolean;
    initialDocument?: string;
    onClose: () => void;
  } = $props();

  let document = $state("");
  let fileName = $state("");
  let destination = $state("");
  let priority = $state("0");
  let speedLimitMbps = $state("");
  let overwrite = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);

  const documentSummary = $derived.by(() => {
    const trimmed = document.trim();
    if (!trimmed) return "Paste a Metalink v4 document or choose a .meta4 file.";
    const fileMatches = trimmed.match(/<file(?:\s|>)/gi)?.length ?? 0;
    return `${fileMatches || "Unknown number of"} file${fileMatches === 1 ? "" : "s"} described`;
  });

  $effect(() => {
    if (!open) return;
    if (initialDocument && !document) document = initialDocument;
    error = null;
  });

  function reset(): void {
    document = "";
    fileName = "";
    destination = "";
    priority = "0";
    speedLimitMbps = "";
    overwrite = false;
    error = null;
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
      document = typeof reader.result === "string" ? reader.result : "";
      fileName = file.name;
      error = null;
    };
    reader.onerror = () => {
      error = "The selected Metalink document could not be read.";
    };
    reader.readAsText(file);
    input.value = "";
  }

  async function submit(): Promise<void> {
    if (!connection.client || !document.trim() || busy) return;
    busy = true;
    error = null;
    const parsedPriority = Number.parseInt(priority, 10);
    const parsedSpeed = Number.parseFloat(speedLimitMbps.replace(",", "."));
    try {
      const job = await connection.client.createMetalinkJob({
        document: document.trim(),
        destination: destination.trim() || null,
        priority: Number.isFinite(parsedPriority) ? parsedPriority : 0,
        speed_limit_bps: Number.isFinite(parsedSpeed) && parsedSpeed > 0 ? Math.round(parsedSpeed * 125_000) : null,
        overwrite,
      });
      notifications.success("Metalink imported", job.filename ?? "The download was added to Ravyn.");
      await jobsStore.refreshAll();
      close();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog {open} title="Import Metalink" onClose={close} preventClose={busy}>
  <div class="form">
    <div class="intro">
      <span class="intro-icon"><Icon name="document" size={20} /></span>
      <div>
        <strong>Import verified mirrors and checksums</strong>
        <span>{documentSummary}</span>
      </div>
    </div>

    <input bind:this={fileInput} class="file-input" type="file" accept=".meta4,.metalink,application/metalink4+xml,application/metalink+xml,text/xml,application/xml" onchange={readFile} />
    <div class="file-row">
      <Button onclick={chooseFile}><Icon name="folder-open" size={15} /> Choose Metalink file</Button>
      {#if fileName}<span title={fileName}>{fileName}</span>{/if}
    </div>

    <TextArea bind:value={document} label="Metalink document" rows={9} placeholder="Paste the XML document here" />
    <PathPicker bind:value={destination} label="Destination" placeholder="Use the Library default" />

    <details class="advanced">
      <summary>Advanced options</summary>
      <div class="advanced-body">
        <div class="two-column">
          <TextField bind:value={priority} label="Priority" inputmode="numeric" />
          <TextField bind:value={speedLimitMbps} label="Speed limit (Mbit/s)" inputmode="decimal" placeholder="Unlimited" />
        </div>
        <ToggleSwitch bind:checked={overwrite} label="Replace existing files" description="Allow Metalink files to replace destinations with the same name." />
      </div>
    </details>

    {#if error}<InlineError title="Couldn't import this Metalink" message={error} />{/if}
  </div>

  {#snippet footer()}
    <Button disabled={busy} onclick={close}>Cancel</Button>
    <Button variant="accent" disabled={busy || !document.trim()} onclick={() => void submit()}>{busy ? "Importing…" : "Import Metalink"}</Button>
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
  .advanced summary { cursor: default; font-weight: 600; }
  .advanced-body { display: flex; flex-direction: column; gap: var(--space-4); padding-top: var(--space-3); }
  .two-column { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  @media (max-width: 600px) { .two-column { grid-template-columns: 1fr; } }
</style>
