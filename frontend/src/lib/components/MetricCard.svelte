<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";
  let { label, value, detail = "", icon = "info" }: { label: string; value: string; detail?: string; icon?: IconName } = $props();
</script>

<div class="metric">
  <div class="icon"><Icon name={icon} size={18} /></div>
  <div class="copy">
    <span class="label">{label}</span>
    <strong>{value}</strong>
    {#if detail}<span class="detail">{detail}</span>{/if}
  </div>
</div>

<style>
  .metric {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-width: 0;
    padding: var(--space-4);
    border: 1px solid var(--stroke-surface);
    border-radius: var(--radius-layer);
    background: var(--surface-card);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: var(--radius-medium);
    color: var(--accent-text);
    background: var(--accent-subtle);
  }
  .copy { display: flex; flex-direction: column; min-width: 0; }
  .label, .detail { color: var(--text-secondary); font-size: var(--text-caption); }
  strong { margin-top: 1px; font-size: 18px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; }
</style>
