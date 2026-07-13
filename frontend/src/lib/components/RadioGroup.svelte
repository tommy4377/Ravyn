<script lang="ts">
  export interface RadioOption {
    value: string;
    label: string;
    description?: string;
  }

  let {
    legend,
    options,
    value = $bindable(""),
    disabled = false,
    onchange,
  }: {
    legend: string;
    options: RadioOption[];
    value?: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
  } = $props();

  const name = $props.id();
</script>

<fieldset class="group" {disabled}>
  <legend class="legend">{legend}</legend>
  {#each options as option (option.value)}
    <label class="option" class:selected={value === option.value}>
      <input
        type="radio"
        {name}
        value={option.value}
        bind:group={value}
        onchange={() => onchange?.(option.value)}
      />
      <span class="text">
        <span class="label">{option.label}</span>
        {#if option.description}
          <span class="description">{option.description}</span>
        {/if}
      </span>
    </label>
  {/each}
</fieldset>

<style>
  .group {
    border: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .legend {
    padding: 0;
    margin-bottom: var(--space-2);
    font-size: var(--text-body);
    font-weight: 600;
    color: var(--text-primary);
  }
  .option {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--stroke-control);
    border-radius: var(--radius-medium);
    background: var(--bg-control);
    min-height: var(--hit-target-minimum);
    transition: background var(--motion-fast) var(--motion-easing);
  }
  .option:hover {
    background: var(--bg-control-hover);
  }
  .option.selected {
    border-color: var(--accent-border);
    background: var(--accent-subtle);
  }
  input {
    width: 18px;
    height: 18px;
    margin: 2px 0 0;
    accent-color: var(--accent-default);
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: var(--text-body);
    font-weight: 600;
  }
  .description {
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
</style>
