<script lang="ts">
  let {
    value = $bindable(""),
    label,
    placeholder = "",
    disabled = false,
    readonly = false,
    error = "",
    oninput,
  }: {
    value?: string;
    label: string;
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    error?: string;
    oninput?: (value: string) => void;
  } = $props();

  const id = $props.id();
</script>

<div class="field">
  <label class="label" for="{id}-input">{label}</label>
  <input
    id="{id}-input"
    class="input"
    class:invalid={!!error}
    type="text"
    bind:value
    {placeholder}
    {disabled}
    {readonly}
    aria-invalid={error ? "true" : undefined}
    aria-describedby={error ? `${id}-error` : undefined}
    oninput={() => oninput?.(value)}
  />
  {#if error}
    <p class="error" id="{id}-error" role="alert">{error}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .label {
    font-size: var(--text-body);
    color: var(--text-primary);
  }
  .input {
    height: var(--control-default);
    padding: 0 var(--space-3);
    font-family: inherit;
    font-size: var(--text-body);
    color: var(--text-primary);
    background: var(--bg-control);
    border: 1px solid var(--stroke-control);
    border-bottom-color: var(--stroke-control-strong);
    border-radius: var(--radius-control);
  }
  .input:focus {
    border-bottom: 2px solid var(--accent-default);
    outline: none;
  }
  .input:disabled {
    background: var(--bg-control-disabled);
    color: var(--text-disabled);
  }
  .input.invalid {
    border-bottom-color: var(--status-error);
  }
  .error {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--status-error);
  }
</style>
