<script lang="ts">
  import { describeError } from "../api/errors";
  import type {
    LibraryMoveConflictPolicy,
    LibraryMovePreflight,
    LibraryMoveStatus,
  } from "../api/types";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown from "../components/Dropdown.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import { restartApplication } from "../native/tauri";
  import {
    isLibraryMoveRunning,
    libraryMoveDescription,
    libraryMoveProgress,
    libraryMoveTitle,
  } from "./libraryMovePresentation";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatBytes } from "../util/format";

  let {
    open,
    currentRoot,
    onClose,
    onActivated,
  }: {
    open: boolean;
    currentRoot: string;
    onClose: () => void;
    onActivated: (destination: string) => void;
  } = $props();

  let destination = $state("");
  let conflictPolicy = $state<LibraryMoveConflictPolicy>("fail");
  let preflight = $state<LibraryMovePreflight | null>(null);
  let status = $state<LibraryMoveStatus | null>(null);
  let checking = $state(false);
  let starting = $state(false);
  let cancelling = $state(false);
  let restarting = $state(false);
  let error = $state<string | null>(null);
  let activationReported = $state<string | null>(null);

  const running = $derived(isLibraryMoveRunning(status));
  const progress = $derived(libraryMoveProgress(status));
  const canStart = $derived(
    !!preflight?.can_start && !running && !starting && !checking && destination.trim().length > 0,
  );

  const policyOptions = [
    { value: "fail", label: "Stop when a destination file exists" },
    { value: "reuse_identical", label: "Reuse files with matching checksums" },
  ];

  function resetDraft(): void {
    destination = "";
    conflictPolicy = "fail";
    preflight = null;
    error = null;
  }

  async function refreshStatus(): Promise<void> {
    if (!connection.client) return;
    try {
      status = await connection.client.getLibraryMoveStatus();
      if (status.state === "restart_required" && status.destination_root) {
        if (activationReported !== status.run_id) {
          activationReported = status.run_id;
          onActivated(status.destination_root);
          notifications.warning(
            "Library move ready",
            "Restart Ravyn to verify the new Library root and remove the old copies.",
          );
        }
      }
    } catch (cause) {
      error = describeError(cause);
    }
  }

  $effect(() => {
    if (!open || !connection.client) return;
    resetDraft();
    void refreshStatus();
  });

  $effect(() => {
    if (!open || !running) return;
    const interval = window.setInterval(() => void refreshStatus(), 750);
    return () => window.clearInterval(interval);
  });

  async function checkMove(): Promise<void> {
    if (!connection.client || !destination.trim() || checking) return;
    checking = true;
    error = null;
    preflight = null;
    try {
      preflight = await connection.client.preflightLibraryMove({
        destination: destination.trim(),
        conflict_policy: conflictPolicy,
      });
    } catch (cause) {
      error = describeError(cause);
    } finally {
      checking = false;
    }
  }

  async function startMove(): Promise<void> {
    if (!connection.client || !canStart) return;
    starting = true;
    error = null;
    try {
      status = await connection.client.startLibraryMove({
        destination: destination.trim(),
        conflict_policy: conflictPolicy,
      });
      notifications.info(
        "Library move started",
        "New downloads are paused until the verified move is activated.",
      );
      void refreshStatus();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      starting = false;
    }
  }

  async function cancelMove(): Promise<void> {
    if (!connection.client || !running || cancelling) return;
    cancelling = true;
    error = null;
    try {
      status = await connection.client.cancelLibraryMove();
      notifications.info("Library move cancellation requested");
    } catch (cause) {
      error = describeError(cause);
    } finally {
      cancelling = false;
    }
  }

  async function restart(): Promise<void> {
    if (restarting) return;
    restarting = true;
    error = null;
    try {
      await restartApplication();
    } catch (cause) {
      restarting = false;
      error = describeError(cause);
    }
  }

  function close(): void {
    if (!running) onClose();
  }


</script>

<Dialog {open} title="Move Library" size="large" preventClose={running} onClose={close}>
  <div class="stack">
    <div class="intro">
      <Icon name="folder-open" size={22} />
      <div>
        <strong>Move tracked files to a new Library root</strong>
        <p>Ravyn copies every tracked file, verifies its checksum, activates the new paths, and keeps the old copies until a successful restart.</p>
      </div>
    </div>

    {#if status && status.state !== "idle"}
      <section class="status" aria-live="polite">
        <div class="status-heading">
          <div>
            <strong>{libraryMoveTitle(status)}</strong>
            <span>{libraryMoveDescription(status)}</span>
          </div>
          <span class="state">{status.state.replaceAll("_", " ")}</span>
        </div>
        {#if running}
          <ProgressBar value={progress} label="Library move progress" />
          <div class="metrics">
            <span>{status.verified_files} of {status.total_files} verified</span>
            <span>{formatBytes(status.copied_bytes)} of {formatBytes(status.total_bytes)} copied</span>
            {#if status.reused_files}<span>{status.reused_files} reused</span>{/if}
          </div>
        {:else if status.state === "restart_required"}
          <div class="restart-note">
            <Icon name="warning" size={18} />
            <span>Restarting performs a final destination verification before the old files are removed.</span>
          </div>
        {/if}
      </section>
    {/if}

    {#if !running && status?.state !== "restart_required"}
      <PathPicker
        bind:value={destination}
        label="New Library root"
        placeholder="Choose an empty folder or a folder with identical files"
        hint={currentRoot ? `Current root: ${currentRoot}` : "The source and destination may not contain each other."}
      />

      <label class="policy-field">
        <span>Existing destination files</span>
        <Dropdown
          options={policyOptions}
          value={conflictPolicy}
          label="Existing destination file policy"
          onchange={(value) => {
            conflictPolicy = value as LibraryMoveConflictPolicy;
            preflight = null;
          }}
        />
      </label>

      {#if preflight}
        <section class:blocked={!preflight.can_start} class="preflight">
          <div class="preflight-title">
            <Icon name={preflight.can_start ? "check-circle" : "warning"} size={18} />
            <strong>{preflight.can_start ? "Ready to move" : "Move cannot start"}</strong>
          </div>
          <div class="summary-grid">
            <span><strong>{preflight.total_files}</strong> tracked files</span>
            <span><strong>{formatBytes(preflight.total_bytes)}</strong> total</span>
            <span><strong>{preflight.copy_files}</strong> to copy</span>
            <span><strong>{preflight.reusable_files}</strong> reusable</span>
            <span><strong>{preflight.missing_files}</strong> missing</span>
            <span><strong>{preflight.external_entries}</strong> outside the root</span>
          </div>
          {#if preflight.available_bytes !== null}
            <p>{formatBytes(preflight.available_bytes)} available at the destination.</p>
          {/if}
          {#if preflight.issues.length}
            <ul>
              {#each preflight.issues as issue}<li>{issue}</li>{/each}
            </ul>
          {/if}
        </section>
      {/if}
    {/if}

    {#if error}<InlineError title="Library move failed" message={error} retry={() => void refreshStatus()} />{/if}
  </div>

  {#snippet footer()}
    {#if running}
      <Button disabled={cancelling || status?.state === "cancelling"} onclick={() => void cancelMove()}>
        {cancelling || status?.state === "cancelling" ? "Cancelling…" : "Cancel move"}
      </Button>
    {:else if status?.state === "restart_required"}
      <Button onclick={onClose}>Later</Button>
      <Button variant="accent" disabled={restarting} onclick={() => void restart()}>
        {restarting ? "Restarting…" : "Restart Ravyn"}
      </Button>
    {:else}
      <Button onclick={onClose}>Close</Button>
      <Button disabled={checking || !destination.trim()} onclick={() => void checkMove()}>
        {checking ? "Checking…" : "Check destination"}
      </Button>
      <Button variant="accent" disabled={!canStart} onclick={() => void startMove()}>
        {starting ? "Starting…" : "Move Library"}
      </Button>
    {/if}
  {/snippet}
</Dialog>


<style>
  .stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .intro, .preflight-title, .restart-note { display: flex; align-items: flex-start; gap: var(--space-3); }
  .intro strong, .status strong { display: block; }
  p { margin: 3px 0 0; color: var(--text-secondary); }
  .policy-field { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-1); font-size: var(--text-body); }
  .status, .preflight { display: flex; flex-direction: column; gap: var(--space-3); padding: var(--space-4); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .status-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-4); }
  .status-heading > div { min-width: 0; }
  .status-heading span { display: block; color: var(--text-secondary); font-size: var(--text-caption); }
  .state { flex: none; padding: 2px 8px; border-radius: var(--radius-pill); background: var(--bg-subtle-pressed); text-transform: capitalize; }
  .metrics, .summary-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--space-2) var(--space-4); color: var(--text-secondary); font-size: var(--text-caption); }
  .summary-grid strong { color: var(--text-primary); }
  .preflight.blocked { background: var(--status-warning-bg); border-color: color-mix(in srgb, var(--status-warning) 28%, transparent); }
  .preflight ul { margin: 0; padding-left: var(--space-5); color: var(--text-secondary); }
  .restart-note { color: var(--status-warning); }
  @media (max-width: 680px) { .metrics, .summary-grid { grid-template-columns: 1fr 1fr; } .status-heading { flex-direction: column; } }
</style>
