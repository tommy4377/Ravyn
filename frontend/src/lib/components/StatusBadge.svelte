<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    label,
    severity = "neutral",
    icon,
    spinning = false,
  }: {
    label: string;
    severity?: "neutral" | "info" | "success" | "warning" | "error";
    icon?: IconName;
    spinning?: boolean;
  } = $props();
</script>

<span class="badge {severity}">
  {#if icon}
    <Icon name={spinning ? "spinner" : icon} size={13} />
  {/if}
  {label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px var(--space-2);
    border-radius: var(--radius-pill);
    font-size: var(--text-caption);
    font-weight: 600;
    line-height: 1.4;
    white-space: nowrap;
    color: var(--text-secondary);
    background: var(--bg-subtle);
  }
  .badge.info {
    color: var(--status-info);
    background: var(--status-info-bg);
  }
  .badge.success {
    color: var(--status-success);
    background: var(--status-success-bg);
  }
  .badge.warning {
    color: var(--status-warning);
    background: var(--status-warning-bg);
  }
  .badge.error {
    color: var(--status-error);
    background: var(--status-error-bg);
  }
  @media (forced-colors: active) {
    .badge {
      border: 1px solid CanvasText;
      forced-color-adjust: none;
      color: CanvasText;
      background: Canvas;
    }
  }
</style>
