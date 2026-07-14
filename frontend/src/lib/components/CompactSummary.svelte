<script lang="ts">
  export interface SummaryItem {
    label: string;
    value: string;
    tone?: "default" | "success" | "warning" | "error";
  }

  let { items, ariaLabel = "Summary" }: { items: SummaryItem[]; ariaLabel?: string } = $props();
</script>

<div class="compact-summary" aria-label={ariaLabel}>
  {#each items as item, index (item.label)}
    {#if index > 0}<span class="separator" aria-hidden="true">·</span>{/if}
    <span class="item" data-tone={item.tone ?? "default"}>
      <strong>{item.value}</strong>
      <span>{item.label}</span>
    </span>
  {/each}
</div>

<style>
  .compact-summary { min-width: 0; display: flex; align-items: center; flex-wrap: wrap; gap: var(--space-2); color: var(--text-secondary); font-size: var(--text-caption); }
  .item { display: inline-flex; align-items: baseline; gap: 4px; white-space: nowrap; }
  strong { color: var(--text-primary); font-weight: 600; }
  .separator { color: var(--text-disabled); }
  .item[data-tone="success"] strong { color: var(--status-success); }
  .item[data-tone="warning"] strong { color: var(--status-warning); }
  .item[data-tone="error"] strong { color: var(--status-error); }
</style>
