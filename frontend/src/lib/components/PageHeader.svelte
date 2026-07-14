<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    description = "",
    eyebrow = "",
    actions,
  }: {
    title: string;
    description?: string;
    eyebrow?: string;
    actions?: Snippet;
  } = $props();
</script>

<header class="page-header">
  <div class="copy">
    {#if eyebrow}<span class="eyebrow">{eyebrow}</span>{/if}
    <h1>{title}</h1>
    {#if description}<p>{description}</p>{/if}
  </div>
  {#if actions}
    <div class="actions">{@render actions()}</div>
  {/if}
</header>

<style>
  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-6);
    padding: var(--space-5) var(--page-padding) var(--space-4);
  }
  .copy {
    min-width: 0;
  }
  .eyebrow {
    display: block;
    margin-bottom: 4px;
    color: var(--text-tertiary);
    font-size: var(--text-caption);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  h1 {
    margin: 0;
    font-family: var(--font-family-display);
    font-size: var(--text-title);
    font-weight: 620;
    line-height: 1.2;
    letter-spacing: -0.01em;
  }
  p {
    max-width: 680px;
    margin: var(--space-1) 0 0;
    color: var(--text-secondary);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: none;
  }
  @media (max-width: 720px) {
    .page-header {
      flex-direction: column;
      gap: var(--space-3);
    }
    .actions {
      width: 100%;
    }
  }
</style>
