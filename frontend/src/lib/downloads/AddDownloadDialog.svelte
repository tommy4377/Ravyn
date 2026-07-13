<script lang="ts">
  import { describeError } from "../api/errors";
  import type { DuplicatePolicy, Job } from "../api/types";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextArea from "../components/TextArea.svelte";
  import TextField from "../components/TextField.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";

  let {
    open,
    initialSource = "",
    onClose,
  }: {
    open: boolean;
    initialSource?: string;
    onClose: () => void;
  } = $props();

  let source = $state("");
  let destination = $state("");
  let filename = $state("");
  let expectedSha256 = $state("");
  let duplicatePolicy = $state("allow");
  let tagsInput = $state("");
  let userAgent = $state("");
  let referer = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) {
      source = initialSource;
      error = null;
    }
  });

  const lineCount = $derived(
    source
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#") && !line.startsWith("//")).length,
  );

  const duplicateOptions: DropdownOption[] = [
    { value: "allow", label: "Allow duplicates" },
    { value: "reuse_existing", label: "Reuse an identical existing download" },
    { value: "skip", label: "Skip if a duplicate exists" },
    { value: "overwrite", label: "Overwrite the existing file" },
    { value: "reject", label: "Reject duplicates" },
  ];

  function reset(): void {
    source = "";
    destination = "";
    filename = "";
    expectedSha256 = "";
    duplicatePolicy = "allow";
    tagsInput = "";
    userAgent = "";
    referer = "";
  }

  function close(): void {
    if (busy) return;
    onClose();
  }

  async function submit(): Promise<void> {
    if (!connection.client || lineCount === 0 || busy) return;
    busy = true;
    error = null;
    const service = new JobsService(connection.client);
    try {
      const result = await service.addFromInput({
        source,
        destination: destination || undefined,
        filename: lineCount === 1 ? filename || undefined : undefined,
        expectedSha256: lineCount === 1 ? expectedSha256 || undefined : undefined,
        duplicatePolicy: duplicatePolicy as DuplicatePolicy,
        tags: tagsInput
          ? tagsInput.split(",").map((tag) => tag.trim()).filter(Boolean)
          : undefined,
        userAgent: userAgent || undefined,
        referer: referer || undefined,
      });

      for (const job of result.jobs as Job[]) jobsStore.upsert(job);

      if (result.createdCount > 0) {
        notifications.success(
          result.createdCount === 1 ? "Download added" : `${result.createdCount} downloads added`,
          result.errors.length > 0 ? `${result.errors.length} line(s) were skipped.` : undefined,
        );
        reset();
        onClose();
      } else {
        error = result.errors[0]?.message ?? "No downloads were created.";
      }
    } catch (cause) {
      error = describeError(cause);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog {open} title="Add download" size="medium" preventClose={busy} onClose={close}>
  <div class="form">
    <TextArea
      bind:value={source}
      label="URL or URLs"
      placeholder={"https://example.com/file.zip\nhttps://example.com/another-file.zip"}
      rows={4}
      hint="One URL per line. Multiple lines create multiple downloads."
    />
    <PathPicker bind:value={destination} label="Destination" placeholder="Use the library default" />

    <details class="advanced">
      <summary>Advanced options</summary>
      <div class="advanced-body">
        {#if lineCount <= 1}
          <TextField bind:value={filename} label="File name" placeholder="Detected automatically" />
          <TextField
            bind:value={expectedSha256}
            label="Expected SHA-256 checksum"
            placeholder="Optional integrity check"
          />
        {/if}
        <div class="dropdown-field">
          <span class="dropdown-label" id="duplicate-policy-label">Duplicate handling</span>
          <Dropdown options={duplicateOptions} bind:value={duplicatePolicy} label="Duplicate handling" />
        </div>
        <TextField bind:value={tagsInput} label="Tags" placeholder="comma, separated, tags" />
        <TextField bind:value={userAgent} label="User agent" placeholder="Optional" />
        <TextField bind:value={referer} label="Referer" placeholder="Optional" />
      </div>
    </details>

    {#if error}
      <InlineError title="Couldn't add this download" message={error} />
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="standard" disabled={busy} onclick={close}>Cancel</Button>
    <Button variant="accent" disabled={busy || lineCount === 0} onclick={submit}>
      {busy ? "Adding…" : lineCount > 1 ? `Add ${lineCount} downloads` : "Add download"}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .advanced summary {
    cursor: default;
    font-size: var(--text-body);
    font-weight: 600;
    color: var(--text-primary);
    padding: var(--space-1) 0;
  }
  .advanced-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding-top: var(--space-3);
  }
  .dropdown-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .dropdown-label {
    font-size: var(--text-body);
    color: var(--text-primary);
  }
</style>
