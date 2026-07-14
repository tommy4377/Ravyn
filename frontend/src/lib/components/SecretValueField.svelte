<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    value = $bindable(""),
    label,
    hint,
    placeholder = "Enter a secret value",
    disabled = false,
  }: {
    value?: string;
    label: string;
    hint?: string;
    placeholder?: string;
    disabled?: boolean;
  } = $props();

  let revealed = $state(false);
  const id = $props.id();
</script>

<div class="field">
  <div class="label-row">
    <label for="{id}-secret">{label}</label>
    <button
      type="button"
      class="reveal"
      disabled={disabled}
      aria-pressed={revealed}
      onclick={() => (revealed = !revealed)}
    >
      <Icon name="eye" size={14} />
      {revealed ? "Hide" : "Show"}
    </button>
  </div>
  <textarea
    id="{id}-secret"
    class:masked={!revealed}
    bind:value
    {placeholder}
    {disabled}
    autocomplete="new-password"
    autocapitalize="off"
    spellcheck="false"
    aria-describedby={hint ? `${id}-hint` : undefined}
    onblur={() => (revealed = false)}
  ></textarea>
  {#if hint}<p id="{id}-hint">{hint}</p>{/if}
</div>

<style>
  .field { display: flex; flex-direction: column; gap: var(--space-1); }
  .label-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  label { color: var(--text-primary); font-size: var(--text-body); }
  .reveal { display: inline-flex; align-items: center; gap: 5px; padding: 3px 5px; border: 0; color: var(--text-secondary); background: transparent; font: inherit; font-size: var(--text-caption); }
  .reveal:hover:not(:disabled) { color: var(--text-primary); }
  textarea { min-height: 112px; resize: vertical; padding: var(--space-3); border: 1px solid var(--stroke-control); border-bottom-color: var(--stroke-control-strong); border-radius: var(--radius-control); color: var(--text-primary); background: var(--bg-control); font: 12px/1.55 ui-monospace, "Cascadia Code", Consolas, monospace; }
  textarea:focus { border-bottom: 2px solid var(--accent-default); outline: none; }
  textarea.masked { -webkit-text-security: disc; }
  textarea:disabled { color: var(--text-disabled); background: var(--bg-control-disabled); }
  p { margin: 0; color: var(--text-secondary); font-size: var(--text-caption); }
</style>
