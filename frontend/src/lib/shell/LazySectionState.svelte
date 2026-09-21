<script lang="ts">
  import Button from "../components/Button.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Skeleton from "../components/Skeleton.svelte";

  let {
    error = null,
    onRetry,
  }: {
    error?: unknown;
    onRetry?: () => void;
  } = $props();

  const message = $derived(error instanceof Error ? error.message : error ? String(error) : "");
</script>

{#if error}
  <div class="state-shell" role="alert">
    <EmptyState icon="warning" title="This section could not be loaded" message={message || "The application bundle could not be opened."}>
      {#if onRetry}
        <Button onclick={onRetry}>Try again</Button>
      {/if}
    </EmptyState>
  </div>
{:else}
  <div class="state-shell loading" aria-label="Loading section" aria-busy="true">
    <div class="header-row">
      <div class="header-copy">
        <Skeleton width="172px" height="26px" />
        <Skeleton width="280px" height="14px" />
      </div>
      <Skeleton width="112px" height="34px" />
    </div>
    <div class="toolbar-row">
      <Skeleton width="260px" height="32px" />
      <Skeleton width="92px" height="32px" />
    </div>
    <div class="content-card">
      <Skeleton width="100%" height="44px" />
      <Skeleton width="100%" height="56px" />
      <Skeleton width="100%" height="56px" />
      <Skeleton width="100%" height="56px" />
    </div>
  </div>
{/if}

<style>
  .state-shell {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .loading {
    align-items: stretch;
    justify-content: flex-start;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-5);
  }
  .header-row,
  .toolbar-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .header-copy {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .content-card {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
    padding: 1px;
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-large);
    background: var(--stroke-divider);
  }
</style>
