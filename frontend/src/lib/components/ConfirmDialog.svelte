<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import InlineError from "./InlineError.svelte";

  let {
    open,
    title,
    message,
    confirmLabel,
    cancelLabel = "Cancel",
    destructive = false,
    busy = false,
    error = null,
    onConfirm,
    onClose,
    details,
  }: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel: string;
    cancelLabel?: string;
    destructive?: boolean;
    busy?: boolean;
    error?: string | null;
    onConfirm: () => void;
    onClose: () => void;
    details?: Snippet;
  } = $props();
</script>

<Dialog {open} {title} size="small" preventClose={busy} onClose={() => !busy && onClose()}>
  <p class="message">{message}</p>
  {#if details}
    <div class="details">{@render details()}</div>
  {/if}
  {#if error}
    <InlineError title="Couldn't complete this action" message={error} />
  {/if}
  {#snippet footer()}
    <Button variant="standard" disabled={busy} onclick={onClose}>{cancelLabel}</Button>
    <Button
      variant={destructive ? "standard" : "accent"}
      disabled={busy}
      onclick={onConfirm}
      data-autofocus
    >
      {busy ? "Working…" : confirmLabel}
    </Button>
  {/snippet}
</Dialog>

<style>
  .message {
    margin: 0;
    color: var(--text-secondary);
  }
  .details { margin-top: var(--space-4); }
</style>
