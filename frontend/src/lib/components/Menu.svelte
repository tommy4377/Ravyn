<script lang="ts">
  import { tick } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  export interface MenuItem {
    id: string;
    label: string;
    icon?: IconName;
    disabled?: boolean;
    danger?: boolean;
    separatorBefore?: boolean;
    onSelect?: () => void;
  }

  let {
    items,
    open,
    x,
    y,
    onClose,
  }: {
    items: MenuItem[];
    open: boolean;
    x: number;
    y: number;
    onClose: () => void;
  } = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let itemEls = $state<(HTMLButtonElement | null)[]>([]);
  let resolvedX = $state(0);
  let resolvedY = $state(0);

  const enabledIndexes = $derived(
    items.reduce<number[]>((acc, item, index) => {
      if (!item.disabled) acc.push(index);
      return acc;
    }, []),
  );

  function clampPosition(): void {
    if (!menuEl) return;
    const margin = 8;
    const rect = menuEl.getBoundingClientRect();
    resolvedX = Math.max(margin, Math.min(x, window.innerWidth - rect.width - margin));
    resolvedY = Math.max(margin, Math.min(y, window.innerHeight - rect.height - margin));
  }

  $effect(() => {
    if (!open) return;
    resolvedX = x;
    resolvedY = y;
    void tick().then(() => {
      clampPosition();
      const first = enabledIndexes[0];
      if (first !== undefined) itemEls[first]?.focus();
    });

    function onPointerDown(event: PointerEvent): void {
      if (menuEl && event.target instanceof Node && !menuEl.contains(event.target)) {
        onClose();
      }
    }
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("resize", clampPosition);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("resize", clampPosition);
    };
  });

  function focusOffset(currentIndex: number, delta: number): void {
    if (enabledIndexes.length === 0) return;
    const pos = enabledIndexes.indexOf(currentIndex);
    const next =
      pos === -1
        ? enabledIndexes[0]
        : enabledIndexes[(pos + delta + enabledIndexes.length) % enabledIndexes.length];
    if (next !== undefined) itemEls[next]?.focus();
  }

  function onKeydown(event: KeyboardEvent, index: number): void {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focusOffset(index, 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        focusOffset(index, -1);
        break;
      case "Home":
        event.preventDefault();
        itemEls[enabledIndexes[0] ?? 0]?.focus();
        break;
      case "End":
        event.preventDefault();
        itemEls[enabledIndexes.at(-1) ?? 0]?.focus();
        break;
      case "Escape":
        event.preventDefault();
        onClose();
        break;
      case "Tab":
        onClose();
        break;
      default:
        break;
    }
  }

  function select(item: MenuItem): void {
    if (item.disabled) return;
    onClose();
    item.onSelect?.();
  }
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="menu"
    role="menu"
    style="left:{resolvedX}px; top:{resolvedY}px;"
  >
    {#each items as item, index (item.id)}
      {#if item.separatorBefore}
        <div class="separator" role="separator"></div>
      {/if}
      <button
        bind:this={itemEls[index]}
        type="button"
        role="menuitem"
        class="item"
        class:danger={item.danger}
        disabled={item.disabled}
        tabindex="-1"
        onclick={() => select(item)}
        onkeydown={(event) => onKeydown(event, index)}
      >
        {#if item.icon}<Icon name={item.icon} size={15} />{/if}
        <span>{item.label}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 200;
    min-width: 200px;
    max-width: 320px;
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-layer);
    border: 1px solid var(--stroke-surface);
    background: var(--surface-flyout);
    box-shadow: var(--shadow-flyout);
    backdrop-filter: blur(28px) saturate(125%);
    -webkit-backdrop-filter: blur(28px) saturate(125%);
  }
  .separator {
    height: 1px;
    margin: var(--space-1) var(--space-2);
    background: var(--stroke-divider);
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--text-body);
    text-align: left;
    cursor: default;
  }
  .item:hover:not(:disabled),
  .item:focus-visible {
    background: var(--bg-subtle-hover);
  }
  .item:disabled {
    color: var(--text-disabled);
  }
  .item.danger {
    color: var(--status-error);
  }
</style>
