<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    selectedCount = 0,
    leading,
    actions,
    selectionContent,
    ariaLabel = "Page commands",
  }: {
    selectedCount?: number;
    leading?: Snippet;
    actions?: Snippet;
    selectionContent?: Snippet;
    ariaLabel?: string;
  } = $props();
</script>

<div class="command-bar" class:selection-mode={selectedCount > 0} aria-label={ariaLabel}>
  {#if selectedCount > 0 && selectionContent}
    <div class="selection-content">{@render selectionContent()}</div>
  {:else}
    <div class="leading">{#if leading}{@render leading()}{/if}</div>
    <div class="actions">{#if actions}{@render actions()}{/if}</div>
  {/if}
</div>

<style>
  .command-bar {
    min-height: 52px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--page-padding);
    border-bottom: 1px solid var(--stroke-divider);
    background: var(--surface-content);
  }
  .command-bar.selection-mode {
    background: color-mix(in srgb, var(--accent-subtle) 56%, var(--surface-content));
  }
  .leading, .actions, .selection-content {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .selection-content { width: 100%; justify-content: space-between; }
  .actions { justify-content: flex-end; }
  @media (max-width: 980px) {
    .command-bar:not(.selection-mode) { align-items: stretch; flex-direction: column; }
    .actions { width: 100%; }
    .actions :global(.search-box) { flex: 1; }
  }
  @media (max-width: 680px) {
    .actions { overflow-x: auto; padding-bottom: 1px; }
    .selection-content { align-items: flex-start; flex-direction: column; }
  }
</style>
