<script lang="ts">
  export interface TabItem {
    id: string;
    label: string;
    badge?: number;
  }

  let {
    tabs,
    selected = $bindable(),
  }: {
    tabs: TabItem[];
    selected: string;
  } = $props();

  let tabEls: (HTMLButtonElement | null)[] = [];

  function onKeydown(event: KeyboardEvent, index: number): void {
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else return;
    event.preventDefault();
    const target = tabs[next];
    if (!target) return;
    selected = target.id;
    tabEls[next]?.focus();
  }
</script>

<div class="tabs" role="tablist">
  {#each tabs as tab, index (tab.id)}
    <button
      bind:this={tabEls[index]}
      type="button"
      role="tab"
      class="tab"
      aria-selected={selected === tab.id}
      tabindex={selected === tab.id ? 0 : -1}
      onclick={() => (selected = tab.id)}
      onkeydown={(event) => onKeydown(event, index)}
    >
      {tab.label}
      {#if tab.badge}<span class="badge">{tab.badge}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    gap: var(--space-1);
    border-bottom: 1px solid var(--stroke-divider);
    padding: 0 var(--space-2);
  }
  .tab {
    position: relative;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: var(--text-body);
    cursor: default;
  }
  .tab:hover {
    color: var(--text-primary);
  }
  .tab[aria-selected="true"] {
    color: var(--text-primary);
    font-weight: 600;
  }
  .tab[aria-selected="true"]::after {
    content: "";
    position: absolute;
    left: var(--space-2);
    right: var(--space-2);
    bottom: -1px;
    height: 2px;
    background: var(--accent-default);
    border-radius: var(--radius-small);
  }
  .badge {
    margin-left: var(--space-1);
    padding: 0 6px;
    border-radius: var(--radius-pill);
    background: var(--bg-subtle);
    font-size: var(--text-caption);
  }
</style>
