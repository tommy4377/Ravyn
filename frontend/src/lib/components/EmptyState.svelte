<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    icon = "download",
    title,
    message,
    children,
  }: {
    icon?: IconName;
    title: string;
    message?: string;
    /** Optional action(s), e.g. a primary Button. */
    children?: Snippet;
  } = $props();
</script>

<div class="empty-state">
  <div class="glyph"><Icon name={icon} size={28} /></div>
  <p class="title">{title}</p>
  {#if message}
    <p class="message">{message}</p>
  {/if}
  {#if children}
    <div class="actions">{@render children()}</div>
  {/if}
</div>

<style>
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: var(--space-2);
    padding: var(--space-12) var(--space-6);
    color: var(--text-secondary);
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    border-radius: var(--radius-large);
    background: var(--bg-subtle);
    color: var(--text-tertiary);
    margin-bottom: var(--space-2);
  }
  .title {
    margin: 0;
    font-size: var(--text-subtitle);
    font-weight: 600;
    color: var(--text-primary);
  }
  .message {
    margin: 0;
    max-width: 420px;
    font-size: var(--text-body);
  }
  .actions {
    margin-top: var(--space-3);
    display: flex;
    gap: var(--space-2);
  }
</style>
