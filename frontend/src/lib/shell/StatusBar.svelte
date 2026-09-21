<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatSpeed } from "../util/format";

  const activeCount = $derived(jobsStore.jobsFor("active").length);
  const total = $derived(jobsStore.list.length);
  const totalSpeed = $derived(
    [...jobsStore.liveProgress.values()].reduce((sum, progress) => sum + Math.max(0, progress.bytesPerSecond), 0),
  );
</script>

<div class="status-bar">
  <span class="item">
    <span class="dot" class:connected={connection.events?.connected}></span>
    {connection.events?.connected ? "Connected" : "Reconnecting…"}
  </span>
  <span class="divider" aria-hidden="true"></span>
  <span class="item" role="status">
    {activeCount} active · {total} loaded
    {#if totalSpeed > 0}<span class="speed">· {formatSpeed(totalSpeed)}</span>{/if}
  </span>
  <button type="button" class="notifications" aria-label={notifications.unreadCount ? `Open notifications, ${notifications.unreadCount} unread` : "Open notifications"} onclick={() => navigation.openNotifications()}>
    <Icon name="bell" size={14} />
    <span>Notifications</span>
    {#if notifications.unreadCount}<span class="badge">{Math.min(99, notifications.unreadCount)}</span>{/if}
  </button>
  {#if connection.setupState?.app_version}
    <span class="version">Ravyn {connection.setupState.app_version}</span>
  {/if}
</div>

<style>
  .status-bar { display: flex; align-items: center; gap: var(--space-3); height: 26px; padding: 0 var(--space-3); border-top: 1px solid var(--stroke-divider); background: color-mix(in srgb, var(--surface-navigation) 88%, transparent); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px); color: var(--text-secondary); font-size: var(--text-caption); flex: none; }
  .item { display: inline-flex; align-items: center; gap: var(--space-1); }
  .speed { color: var(--accent-text); font-family: var(--font-family-mono); font-variant-numeric: tabular-nums; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--status-warning); }
  .dot.connected { background: var(--status-success); }
  .divider { width: 1px; height: 12px; background: var(--stroke-divider); }
  .notifications { display: inline-flex; align-items: center; gap: var(--space-1); height: 22px; padding: 0 var(--space-2); border: 0; border-radius: var(--radius-control); background: transparent; color: var(--text-secondary); font: inherit; }
  .notifications:hover { background: var(--bg-subtle-hover); color: var(--text-primary); }
  .badge { box-sizing: border-box; min-width: 16px; height: 16px; display: inline-flex; align-items: center; justify-content: center; flex: none; padding: 0 4px; border-radius: var(--radius-pill); background: var(--accent-default); color: var(--text-on-accent); font-size: 10px; font-weight: 700; line-height: 1; font-variant-numeric: tabular-nums; }
  .version { margin-left: auto; color: var(--text-tertiary); }
  @media (max-width: 640px) { .notifications > span:not(.badge) { display: none; } }
</style>
