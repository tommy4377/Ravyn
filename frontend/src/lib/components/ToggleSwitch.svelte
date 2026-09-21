<script lang="ts">
  let {
    checked = $bindable(false),
    label,
    description = "",
    disabled = false,
    onchange,
  }: {
    checked?: boolean;
    label: string;
    description?: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
  } = $props();
</script>

<label class="toggle" class:disabled>
  <span class="copy">
    <span class="label">{label}</span>
    {#if description}<span class="description">{description}</span>{/if}
  </span>
  <input type="checkbox" bind:checked {disabled} onchange={() => onchange?.(checked)} />
  <span class="track" aria-hidden="true"><span class="thumb"></span></span>
</label>

<style>
  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-5);
    min-height: 44px;
    cursor: default;
  }
  .copy { display: flex; flex-direction: column; min-width: 0; }
  .label { color: var(--text-primary); }
  .description { color: var(--text-secondary); font-size: var(--text-caption); }
  input { position: absolute; opacity: 0; pointer-events: none; }
  .track {
    position: relative;
    width: 40px;
    height: 20px;
    flex: none;
    border: 1px solid var(--stroke-control-strong);
    border-radius: var(--radius-pill);
    background: var(--bg-control);
    transition: background var(--motion-fast) var(--motion-easing), border-color var(--motion-fast) var(--motion-easing);
  }
  .thumb {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-secondary);
    transition: transform var(--motion-fast) var(--motion-easing), background var(--motion-fast) var(--motion-easing);
  }
  input:checked + .track {
    border-color: var(--accent-default);
    background: var(--accent-default);
  }
  input:checked + .track .thumb {
    transform: translateX(20px);
    background: var(--text-on-accent);
  }
  input:focus-visible + .track { outline: 2px solid var(--stroke-focus); outline-offset: 2px; }
  .disabled { opacity: 0.55; }
</style>
