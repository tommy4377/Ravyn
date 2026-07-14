<script lang="ts">
  import Icon from "./Icon.svelte";

  export interface DropdownOption {
    value: string;
    label: string;
  }

  let {
    options,
    value = $bindable(""),
    label,
    onchange,
    id,
  }: {
    options: DropdownOption[];
    value?: string;
    label: string;
    onchange?: (value: string) => void;
    id?: string;
  } = $props();
</script>

<div class="dropdown">
  <select {id} aria-label={label} bind:value onchange={() => onchange?.(value)}>
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
  <Icon name="chevron-down" size={12} />
</div>

<style>
  .dropdown {
    position: relative;
    display: inline-flex;
    align-items: center;
  }
  select {
    appearance: none;
    height: var(--control-default);
    padding: 0 var(--space-8) 0 var(--space-3);
    border-radius: var(--radius-medium);
    border: 1px solid var(--stroke-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--text-body);
  }
  select:hover {
    background: var(--bg-control-hover);
  }
  select:focus-visible {
    outline: 2px solid var(--stroke-focus);
    outline-offset: 1px;
  }
  .dropdown :global(svg) {
    position: absolute;
    right: var(--space-3);
    pointer-events: none;
    color: var(--text-secondary);
  }
</style>
