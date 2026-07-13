<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    value = $bindable(""),
    placeholder = "Search",
    label,
  }: {
    value?: string;
    placeholder?: string;
    label: string;
  } = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  function clear(): void {
    value = "";
    inputEl?.focus();
  }
</script>

<div class="search-box">
  <Icon name="search" size={14} />
  <input
    bind:this={inputEl}
    type="text"
    aria-label={label}
    {placeholder}
    bind:value
  />
  {#if value}
    <button type="button" class="clear" aria-label="Clear search" onclick={clear}>
      <Icon name="close" size={12} />
    </button>
  {/if}
</div>

<style>
  .search-box {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-default);
    padding: 0 var(--space-2) 0 var(--space-3);
    border-radius: var(--radius-medium);
    border: 1px solid var(--stroke-control);
    background: var(--bg-control);
    color: var(--text-tertiary);
    min-width: 200px;
  }
  .search-box:focus-within {
    border-color: var(--accent-default);
    outline: 2px solid var(--accent-border);
    outline-offset: -1px;
  }
  input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    font-family: inherit;
    font-size: var(--text-body);
    color: var(--text-primary);
  }
  input:focus {
    outline: none;
  }
  .clear {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--text-tertiary);
  }
  .clear:hover {
    background: var(--bg-subtle-hover);
    color: var(--text-primary);
  }
</style>
