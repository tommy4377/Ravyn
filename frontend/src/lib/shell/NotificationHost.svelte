<script lang="ts">
  import Icon, { type IconName } from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import { notifications, type NotificationSeverity } from "../stores/notifications.svelte";

  const icons: Record<NotificationSeverity, IconName> = {
    info: "info",
    success: "check-circle",
    warning: "warning",
    error: "alert-circle",
  };
</script>

<div class="host" aria-live="polite" aria-atomic="false">
  {#each notifications.items as item (item.id)}
    <div class="toast {item.severity}" role={item.severity === "error" ? "alert" : "status"}>
      <Icon name={icons[item.severity]} size={16} />
      <div class="body">
        <p class="title">{item.title}</p>
        {#if item.message}<p class="message">{item.message}</p>{/if}
        {#if item.actionLabel && item.onAction}
          <button
            type="button"
            class="action"
            onclick={() => {
              item.onAction?.();
              notifications.dismiss(item.id);
            }}
          >
            {item.actionLabel}
          </button>
        {/if}
      </div>
      <IconButton icon="close" label="Dismiss" variant="subtle" onclick={() => notifications.dismiss(item.id)} />
    </div>
  {/each}
</div>

<style>
  .host {
    position: fixed;
    right: var(--space-4);
    bottom: calc(26px + var(--space-4));
    z-index: 400;
    display: flex;
    flex-direction: column-reverse;
    gap: var(--space-2);
    max-width: 360px;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3);
    border-radius: var(--radius-layer);
    border: 1px solid var(--stroke-control);
    border-left: 3px solid var(--accent-default);
    background: var(--bg-layer);
    box-shadow: var(--shadow-flyout);
    color: var(--text-primary);
  }
  .toast > :first-child { flex: none; color: var(--accent-default); margin-top: 2px; }
  .toast.error {
    border-color: var(--status-error);
    border-left-color: var(--status-error);
  }
  .toast.error > :first-child { color: var(--status-error); }
  .toast.success {
    border-color: var(--status-success);
    border-left-color: var(--status-success);
  }
  .toast.success > :first-child { color: var(--status-success); }
  .toast.warning {
    border-color: var(--status-warning);
    border-left-color: var(--status-warning);
  }
  .toast.warning > :first-child { color: var(--status-warning); }
  .body {
    flex: 1;
    min-width: 0;
  }
  .title {
    margin: 0;
    font-size: var(--text-body);
    font-weight: 600;
  }
  .message {
    margin: var(--space-1) 0 0;
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
  .action {
    margin-top: var(--space-2);
    border: none;
    background: none;
    padding: 0;
    color: var(--accent-text);
    font-weight: 600;
    font-size: var(--text-caption);
    cursor: pointer;
  }
  @media (max-width: 520px) {
    .host { left: var(--space-3); right: var(--space-3); bottom: calc(26px + var(--space-3)); max-width: none; }
  }
</style>
