<script lang="ts">
  import { describeError } from "../api/errors";
  import type { CreateJob, DuplicatePolicy, Job, JobKind, MediaProbe, SecretReference, TorrentProbe, TrustReport } from "../api/types";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextArea from "../components/TextArea.svelte";
  import TextField from "../components/TextField.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatBytes, formatDuration } from "../util/format";

  let {
    open,
    initialSource = "",
    initialKind = "http",
    onClose,
  }: {
    open: boolean;
    initialSource?: string;
    initialKind?: JobKind;
    onClose: () => void;
  } = $props();

  type DialogKind = JobKind | "metalink" | "batch";

  let kind = $state<DialogKind>("http");
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
  let probing = $state(false);
  let probeError = $state<string | null>(null);
  let mediaProbe = $state<MediaProbe | null>(null);
  let torrentProbe = $state<TorrentProbe | null>(null);
  let mediaFormat = $state("");
  let audioOnly = $state(false);
  let playlist = $state(true);
  let subtitles = $state(false);
  let automaticSubtitles = $state(false);
  let thumbnail = $state(false);
  let embedMetadata = $state(true);
  let seedAfterDownload = $state(true);
  let selectedTorrentFiles = $state<number[]>([]);
  let secretReferences = $state<SecretReference[]>([]);
  let secretsError = $state<string | null>(null);
  let proxySecretId = $state("");
  let cookiesSecretId = $state("");
  let authenticationHeaderSecretId = $state("");
  let overwriteExisting = $state(false);
  let batchResults = $state<{ source: string; error: string }[] | null>(null);
  let trustReport = $state<TrustReport | null>(null);
  let trustCheckedUrl = $state("");
  let trustBusy = $state(false);
  let metalinkFileName = $state("");
  let metalinkFileInput = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (open) {
      source = initialSource;
      kind = initialKind;
      error = null;
      clearProbe();
      void loadSecretReferences();
    }
  });

  const lines = $derived(source.split(/\r?\n/).map((line) => line.trim()).filter((line) => line.length > 0 && !line.startsWith("#") && !line.startsWith("//")));
  const lineCount = $derived(lines.length);
  const parsedBatch = $derived.by((): { jobs: CreateJob[]; error: string | null } => {
    if (kind !== "batch" || !source.trim()) return { jobs: [], error: null };
    try {
      const value: unknown = JSON.parse(source);
      if (!Array.isArray(value)) return { jobs: [], error: "The batch document must be a JSON array of jobs." };
      if (value.length === 0) return { jobs: [], error: "The batch array is empty." };
      for (const [index, entry] of value.entries()) {
        if (typeof entry !== "object" || entry === null || typeof (entry as CreateJob).source !== "string" || !(entry as CreateJob).source) {
          return { jobs: [], error: `Entry ${index + 1} must be an object with a non-empty "source".` };
        }
      }
      return { jobs: value as CreateJob[], error: null };
    } catch (cause) {
      return { jobs: [], error: cause instanceof Error ? cause.message : "Invalid JSON." };
    }
  });
  const canSubmit = $derived(
    kind === "metalink"
      ? source.trim().length > 0
      : kind === "batch"
        ? parsedBatch.jobs.length > 0
        : lineCount > 0 && (kind === "http" || lineCount === 1),
  );
  const allTorrentFilesSelected = $derived(!!torrentProbe?.files.length && selectedTorrentFiles.length === torrentProbe.files.length);
  const kindOptions: DropdownOption[] = [
    { value: "http", label: "Direct download" },
    { value: "media", label: "Video or audio" },
    { value: "torrent", label: "Torrent or magnet" },
    { value: "metalink", label: "Metalink document" },
    { value: "batch", label: "Batch (JSON)" },
  ];
  const duplicateOptions: DropdownOption[] = [
    { value: "allow", label: "Allow duplicates" },
    { value: "reuse_existing", label: "Reuse an identical existing download" },
    { value: "skip", label: "Skip if a duplicate exists" },
    { value: "overwrite", label: "Overwrite the existing file" },
    { value: "reject", label: "Reject duplicates" },
  ];
  const formatOptions = $derived<DropdownOption[]>([
    { value: "", label: "Best available quality" },
    ...(mediaProbe?.formats ?? []).map((format) => ({
      value: format.format_id,
      label: [
        format.height ? `${format.height}p` : format.note,
        format.extension?.toUpperCase(),
        format.video_codec && format.video_codec !== "none" ? format.video_codec : null,
        format.audio_codec && format.audio_codec !== "none" ? format.audio_codec : null,
        format.filesize ?? format.filesize_approx ? formatBytes(format.filesize ?? format.filesize_approx) : null,
      ].filter(Boolean).join(" · ") || format.format_id,
    })),
  ]);
  const proxySecretOptions = $derived(secretOptions("proxy_credentials", "No stored proxy credentials"));
  const cookieSecretOptions = $derived(secretOptions("cookies", "No stored cookie set"));
  const authenticationSecretOptions = $derived(secretOptions("authentication_header", "No stored authorization header"));

  function secretOptions(type: SecretReference["secret_type"], emptyLabel: string): DropdownOption[] {
    return [
      { value: "", label: emptyLabel },
      ...secretReferences
        .filter((reference) => reference.secret_type === type)
        .map((reference) => ({ value: reference.id, label: reference.name })),
    ];
  }

  async function loadSecretReferences(): Promise<void> {
    if (!connection.client) return;
    secretsError = null;
    try {
      secretReferences = (await connection.client.listSecrets({ limit: 100 })).items;
    } catch (cause) {
      secretsError = describeError(cause);
      secretReferences = [];
    }
  }

  function clearProbe(): void {
    mediaProbe = null;
    torrentProbe = null;
    probeError = null;
    mediaFormat = "";
    selectedTorrentFiles = [];
    trustReport = null;
    trustCheckedUrl = "";
  }

  async function checkTrust(): Promise<void> {
    if (!connection.client || lineCount !== 1 || trustBusy) return;
    const url = lines[0]!;
    trustBusy = true;
    try {
      trustReport = await connection.client.previewTrust({
        source_url: url,
        checksum_available: !!expectedSha256.trim(),
      });
      trustCheckedUrl = url;
    } catch (cause) {
      notifications.error("Couldn't evaluate this source", describeError(cause));
    } finally {
      trustBusy = false;
    }
  }

  function reset(): void {
    kind = "http";
    source = "";
    destination = "";
    filename = "";
    expectedSha256 = "";
    duplicatePolicy = "allow";
    tagsInput = "";
    userAgent = "";
    referer = "";
    audioOnly = false;
    playlist = true;
    subtitles = false;
    automaticSubtitles = false;
    thumbnail = false;
    embedMetadata = true;
    seedAfterDownload = true;
    proxySecretId = "";
    cookiesSecretId = "";
    authenticationHeaderSecretId = "";
    overwriteExisting = false;
    metalinkFileName = "";
    batchResults = null;
    clearProbe();
  }

  function changeKind(value: string): void {
    kind = value as DialogKind;
    clearProbe();
  }

  function pickMetalinkFile(): void {
    metalinkFileInput?.click();
  }

  function onMetalinkFileChosen(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      source = typeof reader.result === "string" ? reader.result : "";
      metalinkFileName = file.name;
    };
    reader.onerror = () => notifications.error("Couldn't read this Metalink file");
    reader.readAsText(file);
    input.value = "";
  }

  async function submitBatch(): Promise<void> {
    if (!connection.client) return;
    const result = await connection.client.createBatchJobs(parsedBatch.jobs);
    for (const item of result.items) if (item.job) jobsStore.upsert(item.job);
    const failures = result.items
      .filter((item) => item.error)
      .map((item) => ({ source: item.source, error: item.error as string }));
    if (result.accepted > 0) {
      notifications.success(
        `${result.accepted} download${result.accepted === 1 ? "" : "s"} added`,
        failures.length ? `${failures.length} entr${failures.length === 1 ? "y" : "ies"} rejected.` : undefined,
      );
    }
    if (failures.length > 0) {
      batchResults = failures;
      if (result.accepted === 0) error = "No downloads were created.";
    } else {
      reset();
      onClose();
    }
  }

  async function submitMetalink(service: JobsService): Promise<void> {
    const job = await service.addMetalink(source, {
      destination: destination || undefined,
      overwrite: overwriteExisting,
    });
    jobsStore.upsert(job);
    notifications.success("Metalink download added");
    reset();
    onClose();
  }

  function close(): void {
    if (busy || probing) return;
    onClose();
  }

  async function analyze(): Promise<void> {
    if (!connection.client || lineCount !== 1 || kind === "http" || probing) return;
    probing = true;
    probeError = null;
    mediaProbe = null;
    torrentProbe = null;
    try {
      if (kind === "media") {
        mediaProbe = await connection.client.probeMedia({ url: lines[0]! });
        playlist = (mediaProbe.playlist_count ?? 0) > 1;
      } else {
        torrentProbe = await connection.client.probeTorrent({ source: lines[0]!, destination: destination || null });
        selectedTorrentFiles = torrentProbe.files.map((file) => file.index);
      }
    } catch (cause) {
      probeError = describeError(cause);
    } finally {
      probing = false;
    }
  }

  function toggleTorrentFile(index: number): void {
    selectedTorrentFiles = selectedTorrentFiles.includes(index)
      ? selectedTorrentFiles.filter((value) => value !== index)
      : [...selectedTorrentFiles, index].sort((a, b) => a - b);
  }

  function toggleAllTorrentFiles(): void {
    selectedTorrentFiles = allTorrentFilesSelected ? [] : (torrentProbe?.files.map((file) => file.index) ?? []);
  }

  async function submit(): Promise<void> {
    if (!connection.client || !canSubmit || busy) return;
    busy = true;
    error = null;
    const service = new JobsService(connection.client);
    try {
      if (kind === "metalink") {
        await submitMetalink(service);
        return;
      }
      if (kind === "batch") {
        await submitBatch();
        return;
      }
      const result = await service.addFromInput({
        source,
        destination: destination || undefined,
        filename: lineCount === 1 ? filename || undefined : undefined,
        expectedSha256: kind === "http" && lineCount === 1 ? expectedSha256 || undefined : undefined,
        duplicatePolicy: duplicatePolicy as DuplicatePolicy,
        tags: tagsInput ? tagsInput.split(",").map((tag) => tag.trim()).filter(Boolean) : undefined,
        userAgent: userAgent || undefined,
        referer: referer || undefined,
        proxySecretId: proxySecretId || undefined,
        cookiesSecretId: cookiesSecretId || undefined,
        authenticationHeaderSecretId: authenticationHeaderSecretId || undefined,
        media: kind === "media" ? {
          format: mediaFormat || null,
          audio_only: audioOnly,
          playlist,
          write_subtitles: subtitles,
          write_automatic_subtitles: automaticSubtitles,
          write_thumbnail: thumbnail,
          embed_metadata: embedMetadata,
        } : undefined,
        torrent: kind === "torrent" ? {
          selected_files: torrentProbe ? selectedTorrentFiles : undefined,
          seed_after_download: seedAfterDownload,
        } : undefined,
      }, kind);

      for (const job of result.jobs as Job[]) jobsStore.upsert(job);
      if (result.createdCount > 0) {
        const noun = kind === "media" ? "Media download" : kind === "torrent" ? "Torrent" : "Download";
        notifications.success(result.createdCount === 1 ? `${noun} added` : `${result.createdCount} downloads added`, result.errors.length > 0 ? `${result.errors.length} line(s) were skipped.` : undefined);
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

<svelte:window onkeydown={(event) => {
  if (!open || busy || probing) return;
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) void submit();
}} />

<Dialog {open} title={kind === "media" ? "Add media" : kind === "torrent" ? "Add torrent" : kind === "metalink" ? "Import Metalink" : kind === "batch" ? "Batch import" : "Add download"} size={kind === "http" ? "medium" : "large"} preventClose={busy || probing} onClose={close}>
  <div class="form">
    <div class="kind-field"><span>Download type</span><Dropdown options={kindOptions} bind:value={kind} label="Download type" /></div>
    {#if kind === "batch"}
      <TextArea
        bind:value={source}
        label="Batch document"
        placeholder={'[\n  { "kind": "http", "source": "https://example.com/a.zip", "destination": null, "priority": 5 },\n  { "kind": "http", "source": "https://example.com/b.zip", "expected_sha256": "…" }\n]'}
        rows={10}
        hint={'JSON array of job objects. Each entry needs "kind" and "source"; destination, filename, priority, speed_limit_bps, expected_sha256, duplicate_policy, and options are optional.'}
      />
      {#if parsedBatch.error && source.trim()}
        <InlineError title="Invalid batch document" message={parsedBatch.error} />
      {:else if parsedBatch.jobs.length > 0}
        <p class="batch-summary">{parsedBatch.jobs.length} job{parsedBatch.jobs.length === 1 ? "" : "s"} ready to submit.</p>
      {/if}
      {#if batchResults}
        <div class="batch-failures">
          <strong>Rejected entries</strong>
          <ul>
            {#each batchResults as failure (failure.source + failure.error)}
              <li><span class="failure-source" title={failure.source}>{failure.source}</span><span class="failure-error">{failure.error}</span></li>
            {/each}
          </ul>
        </div>
      {/if}
    {:else if kind === "metalink"}
      <div class="metalink-pick">
        <Button variant="standard" onclick={pickMetalinkFile}><Icon name="folder-open" size={16} /> Choose .metalink file…</Button>
        {#if metalinkFileName}<span class="metalink-name">{metalinkFileName}</span>{/if}
        <input
          type="file"
          accept=".metalink,.meta4,.xml,application/metalink4+xml,application/metalink+xml"
          hidden
          bind:this={metalinkFileInput}
          onchange={onMetalinkFileChosen}
        />
      </div>
      <TextArea
        bind:value={source}
        label="Metalink document"
        placeholder={'<?xml version="1.0" encoding="UTF-8"?>\n<metalink xmlns="urn:ietf:params:xml:ns:metalink">…'}
        rows={8}
        hint="Paste the Metalink XML or choose a .metalink/.meta4 file. File names, sizes, checksums, and mirror URLs come from the document."
      />
    {:else}
      <TextArea
        bind:value={source}
        label={kind === "torrent" ? "Magnet link or torrent source" : kind === "media" ? "Media URL" : "URL or URLs"}
        placeholder={kind === "torrent" ? "magnet:?xt=urn:btih:…" : kind === "media" ? "https://example.com/watch?v=…" : "https://example.com/file.zip\nhttps://example.com/another-file.zip"}
        rows={kind === "http" ? 4 : 3}
        hint={kind === "http" ? "One URL per line. Multiple lines create multiple downloads." : "Media and torrent downloads accept one source at a time so the content can be analyzed first."}
      />
    {/if}
    {#if kind !== "http" && kind !== "metalink" && lineCount > 1}<InlineError title="Use one source" message="Media and torrent downloads must be added one at a time." />{/if}
    {#if kind === "http" && lineCount === 1}
      <div class="trust-row">
        <Button variant="subtle" disabled={trustBusy} onclick={() => void checkTrust()}><Icon name="shield" size={14} /> {trustBusy ? "Checking…" : "Check source"}</Button>
        {#if trustReport && trustCheckedUrl === lines[0]}
          <span class="trust-result trust-{trustReport.level}">Trust {trustReport.score}/100 · {trustReport.level}</span>
          <span class="trust-hint">{trustReport.factors.filter((factor) => !factor.satisfied && factor.points > 0).map((factor) => factor.label).slice(0, 2).join(" · ") || "No missing safeguards detected"}</span>
        {/if}
      </div>
    {/if}
    {#if kind !== "batch"}
      <PathPicker bind:value={destination} label="Destination" placeholder="Use the library default" />
    {/if}

    {#if kind === "metalink"}
      <ToggleSwitch bind:checked={overwriteExisting} label="Overwrite existing files" description="Replace files that already exist at the destination." />
    {/if}

    {#if kind !== "http" && kind !== "metalink" && kind !== "batch"}
      <div class="analyze-row">
        <div><strong>{kind === "media" ? "Inspect available formats" : "Inspect torrent contents"}</strong><small>{kind === "media" ? "Uses yt-dlp to read title, playlist, and quality information." : "Uses the managed torrent engine to read metadata and file names."}</small></div>
        <Button disabled={probing || lineCount !== 1} onclick={() => void analyze()}><Icon name={probing ? "spinner" : "search"} size={16} /> {probing ? "Analyzing…" : mediaProbe || torrentProbe ? "Analyze again" : "Analyze"}</Button>
      </div>
      {#if probeError}<InlineError title={`Couldn't analyze this ${kind}`} message={probeError} retry={() => void analyze()} />{/if}
    {/if}

    {#if kind === "media" && mediaProbe}
      <section class="probe-card media-card">
        {#if mediaProbe.thumbnail}<img src={mediaProbe.thumbnail} alt="" />{/if}
        <div class="probe-copy"><span class="eyebrow">{mediaProbe.extractor ?? "Media"}</span><h3>{mediaProbe.title ?? "Untitled media"}</h3><p>{[mediaProbe.uploader, mediaProbe.duration ? formatDuration(mediaProbe.duration) : null, mediaProbe.playlist_count ? `${mediaProbe.playlist_count} playlist items` : null].filter(Boolean).join(" · ")}</p></div>
      </section>
      <div class="dropdown-field"><span class="dropdown-label">Format</span><Dropdown options={formatOptions} bind:value={mediaFormat} label="Media format" /></div>
      <div class="option-grid">
        <ToggleSwitch bind:checked={audioOnly} label="Audio only" description="Download and process audio without video." />
        <ToggleSwitch bind:checked={playlist} label="Download playlist" description="Include playlist items when the URL contains one." />
        <ToggleSwitch bind:checked={subtitles} label="Download subtitles" description="Save available human-created subtitles." />
        <ToggleSwitch bind:checked={automaticSubtitles} label="Automatic captions" description="Also save automatically generated captions." />
        <ToggleSwitch bind:checked={thumbnail} label="Save thumbnail" description="Write the media thumbnail next to the output." />
        <ToggleSwitch bind:checked={embedMetadata} label="Embed metadata" description="Store title and source metadata in supported outputs." />
      </div>
    {:else if kind === "torrent" && torrentProbe}
      <section class="probe-card"><span class="probe-icon"><Icon name="torrent" size={22} /></span><div class="probe-copy"><span class="eyebrow">Torrent metadata</span><h3>{torrentProbe.name ?? torrentProbe.info_hash ?? "Unnamed torrent"}</h3><p>{formatBytes(torrentProbe.total_bytes)} · {torrentProbe.files.length} file{torrentProbe.files.length === 1 ? "" : "s"}</p></div></section>
      <div class="file-selection">
        <div class="file-command"><label><input type="checkbox" checked={allTorrentFilesSelected} onchange={toggleAllTorrentFiles} /> Select all files</label><span>{selectedTorrentFiles.length} selected</span></div>
        <div class="file-list">
          {#each torrentProbe.files as file (file.index)}
            <label class="file-row"><input type="checkbox" checked={selectedTorrentFiles.includes(file.index)} onchange={() => toggleTorrentFile(file.index)} /><span><strong>{file.path.split(/[\\/]/).at(-1)}</strong><small>{file.path}</small></span><span>{formatBytes(file.size_bytes)}</span></label>
          {/each}
        </div>
      </div>
      <ToggleSwitch bind:checked={seedAfterDownload} label="Seed after download" description="Keep the torrent active after all selected files are complete." />
    {/if}

    {#if kind !== "metalink" && kind !== "batch"}
    <details class="advanced">
      <summary>Advanced options</summary>
      <div class="advanced-body">
        {#if lineCount <= 1}<TextField bind:value={filename} label="File name" placeholder="Detected automatically" />{/if}
        {#if kind === "http" && lineCount <= 1}<TextField bind:value={expectedSha256} label="Expected SHA-256 checksum" placeholder="Optional integrity check" />{/if}
        <div class="dropdown-field"><span class="dropdown-label">Duplicate handling</span><Dropdown options={duplicateOptions} bind:value={duplicatePolicy} label="Duplicate handling" /></div>
        <TextField bind:value={tagsInput} label="Tags" placeholder="comma, separated, tags" />
        <TextField bind:value={userAgent} label="User agent" placeholder="Optional" />
        <TextField bind:value={referer} label="Referer" placeholder="Optional" />
        {#if kind !== "torrent"}
          <div class="secret-grid">
            <div class="dropdown-field"><span class="dropdown-label">Proxy credentials</span><Dropdown options={proxySecretOptions} bind:value={proxySecretId} label="Proxy credentials" /></div>
            <div class="dropdown-field"><span class="dropdown-label">Cookies</span><Dropdown options={cookieSecretOptions} bind:value={cookiesSecretId} label="Cookie secret" /></div>
            <div class="dropdown-field"><span class="dropdown-label">Authorization header</span><Dropdown options={authenticationSecretOptions} bind:value={authenticationHeaderSecretId} label="Authorization header secret" /></div>
          </div>
          {#if secretsError}<p class="secret-note warning">Stored credentials could not be loaded: {secretsError}</p>{:else}<p class="secret-note">Create or replace secret values from Settings. Values are never read back into this dialog.</p>{/if}
        {/if}
      </div>
    </details>
    {/if}

    {#if error}<InlineError title="Couldn't add this download" message={error} />{/if}
  </div>

  {#snippet footer()}
    <Button variant="standard" disabled={busy || probing} onclick={close}>Cancel</Button>
    <Button variant="accent" disabled={busy || probing || !canSubmit || (kind === "torrent" && !!torrentProbe && selectedTorrentFiles.length === 0)} onclick={() => void submit()}>
      {busy ? "Adding…" : kind === "media" ? "Add media" : kind === "torrent" ? "Add torrent" : kind === "metalink" ? "Import Metalink" : kind === "batch" ? `Submit ${parsedBatch.jobs.length || "batch"}` : lineCount > 1 ? `Add ${lineCount} downloads` : "Add download"}
    </Button>
  {/snippet}
</Dialog>

<style>
  .form { display: flex; flex-direction: column; gap: var(--space-4); }
  .kind-field, .dropdown-field { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-1); }
  .kind-field > span, .dropdown-label { font-size: var(--text-body); color: var(--text-primary); }
  .analyze-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-4); border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--bg-subtle); }
  .analyze-row > div { display: flex; flex-direction: column; }
  .analyze-row small { color: var(--text-secondary); }
  .probe-card { display: flex; align-items: center; min-width: 0; gap: var(--space-4); padding: var(--space-4); border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--surface-card); overflow: hidden; }
  .probe-card img { width: 150px; height: 86px; flex: none; object-fit: cover; border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .probe-icon { display: grid; place-items: center; width: 48px; height: 48px; flex: none; border-radius: var(--radius-large); color: var(--accent-text); background: var(--accent-subtle); }
  .probe-copy { min-width: 0; }
  .probe-copy h3 { margin: 2px 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-subtitle); }
  .probe-copy p { margin: 0; color: var(--text-secondary); }
  .eyebrow { color: var(--accent-text); font-size: var(--text-caption); font-weight: 600; text-transform: uppercase; letter-spacing: .04em; }
  .option-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0 var(--space-6); padding: 0 var(--space-3); border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--bg-subtle); }
  .option-grid :global(.toggle) { border-bottom: 1px solid var(--stroke-divider); }
  .file-selection { overflow: hidden; max-height: 290px; display: flex; flex-direction: column; border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); }
  .file-command { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); background: var(--bg-subtle); color: var(--text-secondary); }
  .file-command label { display: flex; align-items: center; gap: var(--space-2); color: var(--text-primary); }
  .file-list { overflow: auto; }
  .file-row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); min-height: 46px; padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .file-row > span:nth-child(2) { display: flex; min-width: 0; flex-direction: column; }
  .file-row strong, .file-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .metalink-pick { display: flex; align-items: center; gap: var(--space-3); }
  .trust-row { display: flex; align-items: center; gap: var(--space-3); min-height: 30px; }
  .batch-summary { margin: calc(var(--space-3) * -1) 0 0; color: var(--text-secondary); font-size: var(--text-caption); }
  .batch-failures { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--stroke-divider); border-radius: var(--radius-control); background: var(--bg-subtle); }
  .batch-failures ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--space-1); max-height: 180px; overflow: auto; }
  .batch-failures li { display: flex; flex-direction: column; font-size: var(--text-caption); }
  .failure-source { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: "Consolas", ui-monospace, monospace; }
  .failure-error { color: var(--status-error); }
  .trust-result { font-weight: 600; font-size: var(--text-caption); }
  .trust-hint { color: var(--text-tertiary); font-size: var(--text-caption); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .metalink-name { color: var(--text-secondary); font-size: var(--text-caption); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .advanced summary { cursor: default; font-size: var(--text-body); font-weight: 600; color: var(--text-primary); padding: var(--space-1) 0; }
  .advanced-body { display: flex; flex-direction: column; gap: var(--space-4); padding-top: var(--space-3); }
  .secret-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--space-3); }
  .secret-grid :global(.dropdown), .secret-grid :global(select) { width: 100%; }
  .secret-note { margin: calc(var(--space-2) * -1) 0 0; color: var(--text-secondary); font-size: var(--text-caption); }
  .secret-note.warning { color: var(--status-warning); }
  @media (max-width: 680px) { .option-grid, .secret-grid { grid-template-columns: 1fr; } .analyze-row { align-items: stretch; flex-direction: column; } .media-card { align-items: flex-start; } .probe-card img { width: 112px; height: 74px; } }
</style>
