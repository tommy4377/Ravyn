<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  let {
    items,
    itemHeight,
    getKey,
    row,
    overscan = 8,
    ariaLabel,
    ariaMultiselectable = false,
    scrollToIndex = $bindable(null),
    onkeydown,
    activeDescendant,
  }: {
    items: T[];
    itemHeight: number;
    getKey: (item: T) => string;
    row: Snippet<[T, number]>;
    overscan?: number;
    ariaLabel: string;
    ariaMultiselectable?: boolean;
    /** Set to an index to scroll it into view; consumed and reset to null. */
    scrollToIndex?: number | null;
    /** Attached to the listbox element itself (a real keyboard-focusable composite widget). */
    onkeydown?: (event: KeyboardEvent) => void;
    /** DOM id of the visually-focused row, for `aria-activedescendant`. */
    activeDescendant?: string;
  } = $props();

  let viewportEl = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  function onScroll(): void {
    scrollTop = viewportEl?.scrollTop ?? 0;
  }

  $effect(() => {
    if (!viewportEl) return;
    viewportHeight = viewportEl.clientHeight;
    const observer = new ResizeObserver((entries) => {
      viewportHeight = entries[0]?.contentRect.height ?? viewportHeight;
    });
    observer.observe(viewportEl);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (scrollToIndex === null || !viewportEl) return;
    const top = scrollToIndex * itemHeight;
    let next = scrollTop;
    if (top < scrollTop) next = top;
    else if (top + itemHeight > scrollTop + viewportHeight) next = top + itemHeight - viewportHeight;
    viewportEl.scrollTop = next;
    scrollTop = next;
    scrollToIndex = null;
  });

  const totalHeight = $derived(items.length * itemHeight);
  const startIndex = $derived(
    Math.max(0, Math.floor(scrollTop / itemHeight) - overscan),
  );
  const endIndex = $derived(
    Math.min(
      items.length,
      Math.ceil((scrollTop + viewportHeight) / itemHeight) + overscan,
    ),
  );
  const visible = $derived(
    items
      .slice(startIndex, endIndex)
      .map((item, offset) => ({ item, index: startIndex + offset })),
  );
</script>

<div
  bind:this={viewportEl}
  class="viewport"
  role="listbox"
  tabindex="0"
  aria-label={ariaLabel}
  aria-multiselectable={ariaMultiselectable}
  aria-activedescendant={activeDescendant}
  onscroll={onScroll}
  {onkeydown}
>
  <div class="spacer" style="height:{totalHeight}px;">
    <div class="window" style="transform: translateY({startIndex * itemHeight}px);">
      {#each visible as entry (getKey(entry.item))}
        <div class="row" style="height:{itemHeight}px;">
          {@render row(entry.item, entry.index)}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    /* Keep row width independent of scrollbar visibility so rows stay
       aligned with fixed column headers rendered outside the viewport. */
    scrollbar-gutter: stable;
  }
  .spacer {
    position: relative;
    width: 100%;
  }
  .window {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
  .row {
    display: flex;
    align-items: stretch;
  }
</style>
