<script lang="ts">
  import Button from "../components/Button.svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import { notifications, type NotificationSeverity } from "../stores/notifications.svelte";
  import { formatAbsoluteTime } from "../util/format";

  let { onClose }: { onClose: () => void } = $props();

  const icons: Record<NotificationSeverity, IconName> = {
    info: "info",
    success: "check-circle",
    warning: "warning",
    error: "alert-circle",
  };

  function runAction(id: string, action?: () => void): void {
    notifications.markRead(id);
    action?.();
  }
</script>

<header class="header">
  <div>
    <h2>Notifications</h2>
    <p>{notifications.unreadCount ? `${notifications.unreadCount} unread` : "No unread notifications"}</p>
  </div>
  <IconButton icon="close" label="Close notifications" variant="subtle" onclick={onClose} />
</header>

<div class="commands">
  <Button variant="subtle" disabled={notifications.unreadCount === 0} onclick={() => notifications.markAllRead()}>Mark all read</Button>
  <Button variant="subtle" disabled={notifications.history.length === 0} onclick={() => notifications.clearHistory()}>Clear history</Button>
</div>

<div class="history">
  {#if notifications.history.length === 0}
    <div class="empty">
      <Icon name="bell" size={24} />
      <strong>No notifications yet</strong>
      <span>Important download, update and maintenance events will appear here.</span>
    </div>
  {:else}
    {#each notifications.history as item (item.id)}
      <article class="item {item.severity}" class:unread={!item.read}>
        <span class="icon"><Icon name={icons[item.severity]} size={16} /></span>
        <div class="copy">
          <div class="title-row">
            <strong>{item.title}</strong>
            <time datetime={new Date(item.createdAt).toISOString()}>{formatAbsoluteTime(new Date(item.createdAt).toISOString())}</time>
          </div>
          {#if item.message}<p>{item.message}</p>{/if}
          {#if item.actionLabel && item.onAction}
            <button type="button" class="action" onclick={(event) => { event.stopPropagation(); runAction(item.id, item.onAction); }}>{item.actionLabel}</button>
          {/if}
        </div>
        {#if !item.read}<button type="button" class="mark-read" aria-label={`Mark ${item.title} as read`} onclick={() => notifications.markRead(item.id)}><span class="unread-dot"></span></button>{/if}
      </article>
    {/each}
  {/if}
</div>

<style>
  .header { min-height: 72px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .header h2 { margin: 0; font-size: var(--text-subtitle); }
  .header p { margin: 2px 0 0; color: var(--text-secondary); font-size: var(--text-caption); }
  .commands { min-height: 44px; display: flex; align-items: center; justify-content: flex-end; gap: var(--space-2); padding: var(--space-1) var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .history { flex: 1; min-height: 0; overflow: auto; outline: none; }
  .item { min-height: 74px; display: grid; grid-template-columns: 32px minmax(0, 1fr) 10px; align-items: start; gap: var(--space-3); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); background: transparent; }
  .item.unread { background: var(--bg-subtle); }
  .icon { width: 30px; height: 30px; display: grid; place-items: center; color: var(--text-secondary); }
  .item.error .icon { color: var(--status-error); }
  .item.warning .icon { color: var(--status-warning); }
  .item.success .icon { color: var(--status-success); }
  .copy { min-width: 0; }
  .title-row { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-3); }
  .title-row strong { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  time { flex: none; color: var(--text-tertiary); font-size: 11px; }
  p { margin: var(--space-1) 0 0; color: var(--text-secondary); font-size: var(--text-caption); line-height: 1.4; }
  .action { margin-top: var(--space-2); padding: 0; border: 0; background: transparent; color: var(--accent-text); font: inherit; font-size: var(--text-caption); font-weight: 600; }
  .mark-read { width: 20px; height: 24px; display: grid; place-items: center; margin-top: 1px; padding: 0; border: 0; border-radius: var(--radius-control); background: transparent; }
  .mark-read:hover { background: var(--bg-subtle-hover); }
  .unread-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent-default); }
  .empty { min-height: 260px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--space-2); padding: var(--space-6); color: var(--text-secondary); text-align: center; }
  .empty strong { color: var(--text-primary); }
  .empty span { max-width: 320px; font-size: var(--text-caption); }
</style>
