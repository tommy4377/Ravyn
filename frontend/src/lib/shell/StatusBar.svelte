<script lang="ts">
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";

  const activeCount = $derived(jobsStore.jobsFor("active").length);
  const total = $derived(jobsStore.list.length);
</script>

<div class="status-bar">
  <span class="item">
    <span class="dot" class:connected={connection.events?.connected}></span>
    {connection.events?.connected ? "Connected" : "Reconnecting…"}
  </span>
  <span class="divider" aria-hidden="true"></span>
  <span class="item" role="status">
    {activeCount} active · {total} loaded
  </span>
  {#if connection.setupState?.app_version}
    <span class="version">Ravyn {connection.setupState.app_version}</span>
  {/if}
</div>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 26px;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--stroke-divider);
    background: color-mix(in srgb, var(--surface-navigation) 88%, transparent);
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
    color: var(--text-secondary);
    font-size: var(--text-caption);
    flex: none;
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--status-warning);
  }
  .dot.connected {
    background: var(--status-success);
  }
  .divider {
    width: 1px;
    height: 12px;
    background: var(--stroke-divider);
  }
  .version {
    margin-left: auto;
    color: var(--text-tertiary);
  }
</style>
