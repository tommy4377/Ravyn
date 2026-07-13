<script lang="ts">
  let {
    checked = $bindable(false),
    disabled = false,
    label,
    description,
    onchange,
  }: {
    checked?: boolean;
    disabled?: boolean;
    label: string;
    description?: string;
    onchange?: (checked: boolean) => void;
  } = $props();

  const id = $props.id();
</script>

<div class="field">
  <label class="row" class:disabled for="{id}-input">
    <input
      id="{id}-input"
      type="checkbox"
      bind:checked
      {disabled}
      aria-describedby={description ? `${id}-desc` : undefined}
      onchange={() => onchange?.(checked)}
    />
    <span class="text">
      <span class="label">{label}</span>
      {#if description}
        <span class="description" id="{id}-desc">{description}</span>
      {/if}
    </span>
  </label>
</div>

<style>
  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: var(--control-default);
    padding: var(--space-1) 0;
  }
  .row.disabled {
    color: var(--text-disabled);
  }
  input {
    width: 20px;
    height: 20px;
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
    color: var(--text-primary);
  }
  .disabled .label {
    color: var(--text-disabled);
  }
  .description {
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
  .disabled .description {
    color: var(--text-disabled);
  }
</style>
