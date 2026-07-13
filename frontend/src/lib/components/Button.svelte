<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "standard",
    disabled = false,
    type = "button",
    onclick,
    children,
    ...rest
  }: {
    variant?: "accent" | "standard" | "subtle";
    disabled?: boolean;
    type?: "button" | "submit";
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
    [key: string]: unknown;
  } = $props();
</script>

<button class="button {variant}" {type} {disabled} {onclick} {...rest}>
  {@render children()}
</button>

<style>
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--control-default);
    padding: 0 var(--space-4);
    font-family: inherit;
    font-size: var(--text-body);
    font-weight: 400;
    border-radius: var(--radius-medium);
    border: 1px solid var(--stroke-control);
    background: var(--bg-control);
    color: var(--text-primary);
    cursor: default;
    user-select: none;
    transition: background var(--motion-fast) var(--motion-easing);
  }

  .button:hover:not(:disabled) {
    background: var(--bg-control-hover);
  }
  .button:active:not(:disabled) {
    background: var(--bg-control-pressed);
    color: var(--text-secondary);
  }
  .button:disabled {
    background: var(--bg-control-disabled);
    color: var(--text-disabled);
    border-color: var(--stroke-divider);
  }

  .accent {
    background: var(--accent-default);
    border-color: transparent;
    color: var(--text-on-accent);
    font-weight: 600;
  }
  .accent:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .accent:active:not(:disabled) {
    background: var(--accent-pressed);
    color: var(--text-on-accent);
  }
  .accent:disabled {
    background: var(--bg-control-disabled);
    color: var(--text-disabled);
  }

  .subtle {
    background: transparent;
    border-color: transparent;
  }
  .subtle:hover:not(:disabled) {
    background: var(--bg-subtle-hover);
  }
  .subtle:active:not(:disabled) {
    background: var(--bg-subtle-pressed);
  }
</style>
