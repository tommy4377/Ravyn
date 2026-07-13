<script lang="ts">
  let {
    value = $bindable(""),
    label,
    placeholder = "",
    rows = 4,
    hint,
    error,
  }: {
    value?: string;
    label: string;
    placeholder?: string;
    rows?: number;
    hint?: string;
    error?: string;
  } = $props();

  const id = $props.id();
</script>

<div class="field">
  <label for="{id}-textarea">{label}</label>
  <textarea
    id="{id}-textarea"
    {rows}
    {placeholder}
    bind:value
    aria-invalid={error ? "true" : undefined}
    aria-describedby={error ? `${id}-error` : hint ? `${id}-hint` : undefined}
  ></textarea>
  {#if error}
    <p class="error" id="{id}-error">{error}</p>
  {:else if hint}
    <p class="hint" id="{id}-hint">{hint}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  label {
    font-size: var(--text-body);
    color: var(--text-primary);
  }
  textarea {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-medium);
    border: 1px solid var(--stroke-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-family: "Consolas", "Cascadia Mono", ui-monospace, monospace;
    font-size: var(--text-body);
    resize: vertical;
  }
  textarea:focus-visible {
    outline: 2px solid var(--stroke-focus);
    outline-offset: 1px;
    border-color: var(--accent-default);
  }
  .hint {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
  .error {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--status-error);
  }
</style>
