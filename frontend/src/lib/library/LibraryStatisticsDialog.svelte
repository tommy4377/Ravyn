<script lang="ts">
  import { describeError } from "../api/errors";
  import type { PersonalStatistics } from "../api/types";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import { connection } from "../stores/connection.svelte";
  import { formatBytes, formatSpeed } from "../util/format";

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();

  let statistics = $state<PersonalStatistics | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let loadedForOpen = $state(false);

  const categories = $derived(
    statistics
      ? Object.entries(statistics.categories)
          .map(([name, value]) => ({ name, ...value }))
          .sort((left, right) => right.bytes - left.bytes || left.name.localeCompare(right.name))
      : [],
  );
  const maxCategoryBytes = $derived(Math.max(1, ...categories.map((category) => category.bytes)));
  const recentMonths = $derived(statistics?.monthly_activity.slice(-12) ?? []);
  const maxMonthlyBytes = $derived(Math.max(1, ...recentMonths.map((bucket) => bucket.bytes)));

  $effect(() => {
    if (!open) {
      loadedForOpen = false;
      return;
    }
    if (!loadedForOpen) {
      loadedForOpen = true;
      void load();
    }
  });

  async function load(): Promise<void> {
    if (!connection.client || loading) return;
    loading = true;
    error = null;
    try {
      statistics = await connection.client.getStatistics();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }
</script>

<Dialog {open} title="Library statistics" onClose={onClose} size="large">
  {#if error}
    <InlineError title="Couldn't load statistics" message={error} retry={() => void load()} />
  {:else if loading && !statistics}
    <div class="loading"><Icon name="spinner" size={18} /> Loading statistics…</div>
  {:else if statistics}
    <div class="content">
      <section class="summary" aria-label="Library statistics summary">
        <div><span>Total files</span><strong>{statistics.total_files.toLocaleString()}</strong></div>
        <div><span>Downloaded</span><strong>{formatBytes(statistics.total_downloaded_bytes)}</strong></div>
        <div><span>Average speed</span><strong>{formatSpeed(statistics.average_speed_bps)}</strong></div>
        <div><span>Bandwidth saved</span><strong>{formatBytes(statistics.saved_bandwidth_bytes)}</strong></div>
        <div><span>Duplicates avoided</span><strong>{statistics.duplicate_avoidance_count.toLocaleString()}</strong></div>
        <div><span>In trash</span><strong>{formatBytes(statistics.trashed_storage_bytes)}</strong></div>
      </section>

      <section class="section">
        <header><strong>Storage by category</strong><span>{formatBytes(statistics.active_storage_bytes)} active</span></header>
        {#if categories.length === 0}
          <p class="empty">No categorized files yet.</p>
        {:else}
          <div class="category-list">
            {#each categories as category (category.name)}
              <div class="category-row">
                <div><strong>{category.name}</strong><span>{category.files.toLocaleString()} file{category.files === 1 ? "" : "s"}</span></div>
                <div class="bar" aria-hidden="true"><span style:width={`${Math.max(2, category.bytes / maxCategoryBytes * 100)}%`}></span></div>
                <span>{formatBytes(category.bytes)}</span>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <section class="section">
        <header><strong>Recent monthly activity</strong><span>Last {recentMonths.length || 0} recorded month{recentMonths.length === 1 ? "" : "s"}</span></header>
        {#if recentMonths.length === 0}
          <p class="empty">Activity appears after completed downloads are indexed.</p>
        {:else}
          <div class="activity" aria-label="Monthly downloaded bytes">
            {#each recentMonths as bucket (bucket.period)}
              <div class="month" title={`${bucket.period}: ${formatBytes(bucket.bytes)} across ${bucket.files} files`}>
                <div class="column"><span style:height={`${Math.max(4, bucket.bytes / maxMonthlyBytes * 100)}%`}></span></div>
                <strong>{bucket.period.slice(5)}</strong>
                <small>{formatBytes(bucket.bytes)}</small>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    </div>
  {/if}

  {#snippet footer()}
    <Button disabled={loading} onclick={() => void load()}><Icon name="refresh" size={15} /> Refresh</Button>
    <Button variant="accent" onclick={onClose}>Close</Button>
  {/snippet}
</Dialog>

<style>
  .loading { min-height: 240px; display: flex; align-items: center; justify-content: center; gap: var(--space-2); color: var(--text-secondary); }
  .content { display: flex; flex-direction: column; gap: var(--space-5); }
  .summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); border: 1px solid var(--stroke-divider); border-radius: var(--radius-layer); }
  .summary div { min-height: 72px; display: flex; flex-direction: column; justify-content: center; gap: 3px; padding: var(--space-3); border-right: 1px solid var(--stroke-divider); border-bottom: 1px solid var(--stroke-divider); }
  .summary div:nth-child(3n) { border-right: 0; }
  .summary div:nth-last-child(-n + 3) { border-bottom: 0; }
  .summary span, .section header span, .category-row span, .category-row div span, .month small { color: var(--text-secondary); font-size: var(--text-caption); }
  .summary strong { font-size: var(--text-subtitle); font-weight: 620; }
  .section { border-top: 1px solid var(--stroke-divider); }
  .section header { min-height: 48px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  .category-list { display: flex; flex-direction: column; }
  .category-row { min-height: 54px; display: grid; grid-template-columns: minmax(120px, .8fr) minmax(160px, 1.4fr) 90px; align-items: center; gap: var(--space-3); border-top: 1px solid var(--stroke-divider); }
  .category-row > div:first-child { min-width: 0; display: flex; flex-direction: column; text-transform: capitalize; }
  .bar { height: 6px; overflow: hidden; border-radius: var(--radius-pill); background: var(--bg-subtle); }
  .bar span { display: block; height: 100%; border-radius: inherit; background: var(--accent-default); }
  .category-row > span { text-align: right; }
  .activity { height: 190px; display: grid; grid-template-columns: repeat(12, minmax(28px, 1fr)); align-items: end; gap: var(--space-2); padding-top: var(--space-3); border-top: 1px solid var(--stroke-divider); overflow-x: auto; }
  .month { min-width: 34px; height: 170px; display: grid; grid-template-rows: minmax(80px, 1fr) auto auto; align-items: end; gap: 3px; text-align: center; }
  .column { width: 18px; height: 100%; justify-self: center; display: flex; align-items: end; overflow: hidden; border-radius: var(--radius-control) var(--radius-control) 0 0; background: var(--bg-subtle); }
  .column span { width: 100%; display: block; background: var(--accent-default); }
  .month strong { font-size: 11px; font-weight: 600; }
  .month small { max-width: 54px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { margin: 0; padding: var(--space-4) 0; color: var(--text-secondary); }
  @media (max-width: 650px) { .summary { grid-template-columns: repeat(2, minmax(0, 1fr)); } .summary div:nth-child(3n) { border-right: 1px solid var(--stroke-divider); } .summary div:nth-child(2n) { border-right: 0; } .summary div:nth-last-child(-n + 3) { border-bottom: 1px solid var(--stroke-divider); } .summary div:nth-last-child(-n + 2) { border-bottom: 0; } .category-row { grid-template-columns: minmax(120px, 1fr) 82px; } .bar { display: none; } }
</style>
