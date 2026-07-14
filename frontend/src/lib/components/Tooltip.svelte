<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    text,
    placement = "bottom",
    disabled = false,
    children,
  }: {
    text: string;
    placement?: "top" | "bottom" | "left" | "right";
    disabled?: boolean;
    children: Snippet;
  } = $props();

  let visible = $state(false);
  let showTimer: ReturnType<typeof setTimeout> | null = null;

  function show(): void {
    if (disabled) return;
    showTimer = setTimeout(() => {
      visible = true;
    }, 400);
  }

  function hide(): void {
    if (showTimer) clearTimeout(showTimer);
    showTimer = null;
    visible = false;
  }
</script>

<span
  class="tooltip-host tooltip-wrapper"
  role="presentation"
  onmouseenter={show}
  onmouseleave={hide}
  onfocusin={() => { if (!disabled) visible = true; }}
  onfocusout={hide}
>
  {@render children()}
  {#if visible && text && !disabled}
    <span class="tooltip {placement}" role="tooltip">{text}</span>
  {/if}
</span>

<style>
  .tooltip-host {
    position: relative;
    display: inline-flex;
  }
  .tooltip {
    position: absolute;
    z-index: 100;
    white-space: nowrap;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-control);
    background: var(--text-primary);
    color: var(--bg-layer);
    font-size: var(--text-caption);
    box-shadow: var(--shadow-flyout);
    pointer-events: none;
  }
  .tooltip.bottom {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  .tooltip.top {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  .tooltip.right {
    left: calc(100% + 6px);
    top: 50%;
    transform: translateY(-50%);
  }
  .tooltip.left {
    right: calc(100% + 6px);
    top: 50%;
    transform: translateY(-50%);
  }

  @media (forced-colors: active) {
    .tooltip {
      border: 1px solid CanvasText;
    }
  }
</style>
