<script lang="ts">
  import { tick } from "svelte";
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    count = 0,
    label = "Filter",
    onClear,
    children,
  }: {
    count?: number;
    label?: string;
    onClear?: () => void;
    children: Snippet;
  } = $props();

  let open = $state(false);
  let root = $state<HTMLDivElement | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);
  let flyout = $state<HTMLDivElement | null>(null);
  let x = $state(0);
  let y = $state(0);
  let positioned = $state(false);

  function positionFlyout(): void {
    if (!trigger || !flyout) return;
    const margin = 8;
    const triggerRect = trigger.getBoundingClientRect();
    const flyoutRect = flyout.getBoundingClientRect();
    x = Math.max(
      margin,
      Math.min(triggerRect.right - flyoutRect.width, window.innerWidth - flyoutRect.width - margin),
    );
    y = Math.max(
      margin,
      Math.min(triggerRect.bottom + 5, window.innerHeight - flyoutRect.height - margin),
    );
    positioned = true;
  }

  function syncPopoverState(event: ToggleEvent): void {
    if (event.newState === "closed" && open) {
      open = false;
    }
  }

  $effect(() => {
    if (!open || !flyout) return;
    positioned = false;
    if (typeof flyout.showPopover === "function") {
      flyout.setAttribute("popover", "auto");
      try {
        flyout.showPopover();
      } catch {
        // A reactive position update can run while the flyout is already open.
      }
    }
    void tick().then(positionFlyout);
    const closeFromOutside = (event: PointerEvent): void => {
      if (
        root &&
        flyout &&
        event.target instanceof Node &&
        !root.contains(event.target) &&
        !flyout.contains(event.target)
      )
        open = false;
    };
    const closeFromKeyboard = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        open = false;
        trigger?.focus();
      }
    };
    const closeFromScroll = (): void => {
      open = false;
    };
    window.addEventListener("pointerdown", closeFromOutside, true);
    window.addEventListener("keydown", closeFromKeyboard, true);
    window.addEventListener("resize", positionFlyout);
    window.addEventListener("scroll", closeFromScroll, true);
    return () => {
      window.removeEventListener("pointerdown", closeFromOutside, true);
      window.removeEventListener("keydown", closeFromKeyboard, true);
      window.removeEventListener("resize", positionFlyout);
      window.removeEventListener("scroll", closeFromScroll, true);
    };
  });
</script>

<div class="filter-flyout" bind:this={root}>
  <button
    bind:this={trigger}
    type="button"
    class="trigger"
    aria-haspopup="dialog"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <Icon name="filter" size={15} />
    <span>{label}</span>
    {#if count > 0}<span class="count" aria-label={`${count} active filters`}>{count}</span>{/if}
  </button>

  {#if open}
    <div
      bind:this={flyout}
      class="flyout"
      role="dialog"
      aria-label="Filters"
      ontoggle={syncPopoverState}
      style="left:{x}px; top:{y}px; visibility:{positioned ? 'visible' : 'hidden'};"
    >
      <header>
        <strong>Filters</strong>
        {#if count > 0 && onClear}
          <button type="button" class="clear" onclick={() => onClear?.()}>Clear</button>
        {/if}
      </header>
      <div class="content">{@render children()}</div>
    </div>
  {/if}
</div>

<style>
  .filter-flyout { position: relative; display: inline-flex; }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-default);
    padding: 0 var(--space-3);
    border: 1px solid var(--stroke-control);
    border-radius: var(--radius-medium);
    background: var(--bg-control);
    color: var(--text-primary);
    font: inherit;
    font-size: var(--text-body);
    cursor: default;
  }
  .trigger:hover, .trigger[aria-expanded="true"] { background: var(--bg-control-hover); }
  .trigger:focus-visible { outline: 2px solid var(--stroke-focus); outline-offset: 1px; }
  .count {
    min-width: 18px;
    height: 18px;
    display: inline-grid;
    place-items: center;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--accent-default);
    color: var(--accent-on-color);
    font-size: 11px;
    font-weight: 700;
  }
  .flyout {
    position: fixed;
    inset: auto;
    margin: 0;
    z-index: 160;
    width: min(300px, calc(100vw - 32px));
    padding: var(--space-2);
    border: 1px solid var(--stroke-surface);
    border-radius: var(--radius-layer);
    background: var(--surface-flyout);
    box-shadow: var(--shadow-flyout);
    backdrop-filter: blur(24px) saturate(120%);
  }
  header { min-height: 34px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: 0 var(--space-2); }
  header strong { font-size: var(--text-body); font-weight: 600; }
  .clear { border: 0; background: transparent; color: var(--accent-text); font: inherit; font-size: var(--text-caption); cursor: default; }
  .clear:hover { text-decoration: underline; }
  .content { padding: var(--space-2); border-top: 1px solid var(--stroke-divider); }
</style>
