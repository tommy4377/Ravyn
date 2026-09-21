<script lang="ts">
  import { untrack } from "svelte";
  import { describeError } from "../api/errors";
  import type { TorrentDhtStats, TorrentDhtTable, TorrentEngineList, TorrentGlobalStats } from "../api/types";
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import CompactSummary, { type SummaryItem } from "../components/CompactSummary.svelte";
  import Dialog from "../components/Dialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatBytes, formatSpeed } from "../util/format";

  let {
    open,
    managedHashes,
    onClose,
  }: {
    open: boolean;
    managedHashes: Set<string>;
    onClose: () => void;
  } = $props();

  let engineList = $state<TorrentEngineList | null>(null);
  let engineStats = $state<TorrentGlobalStats | null>(null);
  let dhtStats = $state<TorrentDhtStats | null>(null);
  let dhtTable = $state<TorrentDhtTable | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const summaryItems = $derived<SummaryItem[]>([
    { label: "download", value: formatSpeed(engineStats?.download_speed_bps) },
    { label: "upload", value: formatSpeed(engineStats?.upload_speed_bps) },
    { label: "active torrents", value: String(engineStats?.active_torrents ?? engineList?.torrents.length ?? 0) },
    { label: "DHT nodes", value: String((dhtStats?.routing_table_size ?? 0) + (dhtStats?.routing_table_size_v6 ?? 0)) },
  ]);

  async function load(): Promise<void> {
    if (!connection.client || loading) return;
    loading = true;
    error = null;
    try {
      const [list, stats, dht, table] = await Promise.all([
        connection.client.listEngineTorrents(),
        connection.client.getTorrentEngineStats().catch(() => null),
        connection.client.getTorrentDhtStats().catch(() => null),
        connection.client.getTorrentDhtTable().catch(() => null),
      ]);
      engineList = list;
      engineStats = stats;
      dhtStats = dht;
      dhtTable = table;
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  // load() reads `loading` synchronously (its own re-entrancy guard) and
  // writes it after the awaited requests settle; without untrack that read
  // makes this effect depend on `loading`, so every completed load flipped
  // it back to false and re-triggered the effect — an unbroken refresh loop
  // for as long as the dialog stayed open.
  $effect(() => {
    if (open) untrack(() => void load());
  });

  async function copyDht(): Promise<void> {
    if (!dhtTable) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(dhtTable, null, 2));
      notifications.info("DHT routing table copied");
    } catch {
      notifications.warning("Couldn't copy the DHT routing table");
    }
  }
</script>

<Dialog {open} title="Torrent engine details" size="large" onClose={onClose}>
  <div class="dialog-stack">
    <div class="dialog-command">
      <CompactSummary items={summaryItems} ariaLabel="Torrent engine summary" />
      <Button variant="subtle" disabled={loading} onclick={() => void load()}><Icon name="refresh" size={15} /> Refresh</Button>
    </div>

    {#if error}
      <InlineError title="Couldn't read the torrent engine" message={error} retry={() => void load()} />
    {:else if loading && engineList === null}
      <p class="muted">Reading torrent engine state…</p>
    {:else if !engineList?.torrents.length}
      <EmptyState icon="torrent" title="The engine has no torrents" message="Torrents added through Ravyn or directly in rqbit will appear here." />
    {:else}
      <div class="engine-table">
        <div class="engine-header" aria-hidden="true"><span>Name</span><span>State</span><span>Progress</span><span>Origin</span></div>
        {#each engineList.torrents as torrent, index (torrent.torrent_id ?? torrent.info_hash ?? index)}
          {@const managed = !!torrent.info_hash && managedHashes.has(torrent.info_hash.toLowerCase())}
          <div class="engine-row">
            <span><strong>{torrent.name ?? torrent.info_hash ?? `Engine torrent ${torrent.torrent_id ?? index}`}</strong><small>{torrent.info_hash ?? "No info hash reported"}</small></span>
            <span>{torrent.state ?? "unknown"}</span>
            <span>{torrent.progress !== null ? `${Math.round(torrent.progress * 100)}%` : formatBytes(torrent.downloaded_bytes)}</span>
            <StatusBadge label={managed ? "Managed by Ravyn" : "Engine only"} severity={managed ? "success" : "warning"} />
          </div>
        {/each}
      </div>
    {/if}

    <AdvancedDisclosure title="DHT routing table" description="Raw IPv4 and IPv6 routing data for troubleshooting.">
      <div class="dht-command">
        <span>{dhtStats ? `${dhtStats.routing_table_size} IPv4 · ${dhtStats.routing_table_size_v6} IPv6 · ${dhtStats.outstanding_requests} requests` : "DHT statistics unavailable"}</span>
        <Button variant="subtle" disabled={!dhtTable} onclick={() => void copyDht()}><Icon name="copy" size={14} /> Copy JSON</Button>
      </div>
      {#if dhtTable}
        <div class="dht-grid">
          <section><h3>IPv4</h3><pre>{JSON.stringify(dhtTable.v4, null, 2)}</pre></section>
          <section><h3>IPv6</h3><pre>{JSON.stringify(dhtTable.v6, null, 2)}</pre></section>
        </div>
      {:else}
        <p class="muted">The DHT routing table is not available.</p>
      {/if}
    </AdvancedDisclosure>
  </div>

  {#snippet footer()}<Button variant="accent" onclick={onClose}>Done</Button>{/snippet}
</Dialog>

<style>
  .dialog-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .dialog-command, .dht-command { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  .muted { color: var(--text-secondary); }
  .engine-table { max-height: 320px; overflow: auto; border-block: 1px solid var(--stroke-divider); }
  .engine-header, .engine-row { display: grid; grid-template-columns: minmax(220px, 1.7fr) 120px 100px 150px; align-items: center; gap: var(--space-3); }
  .engine-header { min-height: 34px; color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .engine-row { min-height: 48px; border-top: 1px solid var(--stroke-divider); }
  .engine-row > span:first-child { min-width: 0; display: flex; flex-direction: column; }
  .engine-row strong, .engine-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .engine-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .dht-command { margin-bottom: var(--space-3); color: var(--text-secondary); }
  .dht-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-3); }
  .dht-grid h3 { margin: 0 0 var(--space-2); font-size: var(--text-caption); color: var(--text-tertiary); }
  pre { max-height: 240px; margin: 0; padding: var(--space-3); overflow: auto; border: 1px solid var(--stroke-divider); border-radius: var(--radius-control); background: var(--bg-subtle); font-size: var(--text-caption); }
  @media (max-width: 720px) { .engine-header, .engine-row { grid-template-columns: minmax(0, 1fr) 130px; } .engine-header span:nth-child(2), .engine-header span:nth-child(3), .engine-row > span:nth-child(2), .engine-row > span:nth-child(3) { display: none; } .dht-grid { grid-template-columns: 1fr; } }
</style>
