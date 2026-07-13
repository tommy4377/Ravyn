<script lang="ts">
  import { pickFolder } from "../native/tauri";
  import IconButton from "./IconButton.svelte";

  let {
    value = $bindable(""),
    label,
    placeholder = "",
    hint,
    error = "",
  }: {
    value?: string;
    label: string;
    placeholder?: string;
    hint?: string;
    error?: string;
  } = $props();

  const id = $props.id();

  async function browse(): Promise<void> {
    const picked = await pickFolder(value || undefined);
    if (picked) value = picked;
  }
</script>

<div class="field">
  <label for="{id}-input">{label}</label>
  <div class="row">
    <input
      id="{id}-input"
      class="input"
      class:invalid={!!error}
      type="text"
      bind:value
      {placeholder}
      aria-invalid={error ? "true" : undefined}
      aria-describedby={error ? `${id}-error` : hint ? `${id}-hint` : undefined}
    />
    <IconButton icon="folder-open" label="Browse for a folder" onclick={browse} />
  </div>
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
  .row {
    display: flex;
    gap: var(--space-2);
  }
  .input {
    flex: 1;
    min-width: 0;
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
  .input.invalid {
    border-bottom-color: var(--status-error);
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
