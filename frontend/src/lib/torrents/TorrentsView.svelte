<script lang="ts">
  import { describeError } from "../api/errors";
  import type {
    TorrentDetails,
    TorrentDhtStats,
    TorrentGlobalStats,
    TorrentPeerStats,
    TorrentRecord,
    TorrentSeedingState,
    TorrentSnapshot,
  } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dialog from "../components/Dialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import MetricCard from "../components/MetricCard.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import TextArea from "../components/TextArea.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import { connection } from "../stores/connection.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes, formatSpeed } from "../util/format";

  type DetailTab = "overview" | "files" | "peers";

  let torrents = $state<TorrentRecord[]>([]);
  let engineStats = $state<TorrentGlobalStats | null>(null);
  let dhtStats = $state<TorrentDhtStats | null>(null);
  let search = $state("");
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let detailTab = $state<DetailTab>("overview");
  let details = $state<TorrentDetails | null>(null);
  let snapshot = $state<TorrentSnapshot | null>(null);
  let peerStats = $state<TorrentPeerStats | null>(null);
  let seeding = $state<TorrentSeedingState | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);
  let selectedFiles = $state<number[]>([]);
  let savingFiles = $state(false);
  let removeTarget = $state<TorrentRecord | null>(null);
  let deleteFiles = $state(false);
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);
  let peersDialogOpen = $state(false);
  let peersInput = $state("");
  let peersBusy = $state(false);
  let peersError = $state<string | null>(null);

  const visible = $derived(
    search.trim()
      ? torrents.filter((torrent) => `${torrent.name ?? ""} ${torrent.info_hash ?? ""} ${torrent.state}`.toLowerCase().includes(search.toLowerCase()))
      : torrents,
  );
  const selected = $derived(torrents.find((torrent) => torrent.job_id === selectedId) ?? null);
  const totalDown = $derived(engineStats?.download_speed_bps ?? torrents.reduce((sum, torrent) => sum + torrent.download_speed_bps, 0));
  const totalUp = $derived(engineStats?.upload_speed_bps ?? torrents.reduce((sum, torrent) => sum + torrent.upload_speed_bps, 0));
  const peerCount = $derived(torrents.reduce((sum, torrent) => sum + torrent.peers_connected, 0));
  const allFilesSelected = $derived(!!details?.files.length && selectedFiles.length === details.files.length);

  function severity(state: string): "neutral" | "info" | "success" | "warning" | "error" {
    const normalized = state.toLowerCase();
    if (normalized.includes("error") || normalized.includes("fail")) return "error";
    if (normalized.includes("seed") || normalized.includes("complete")) return "success";
    if (normalized.includes("pause") || normalized.includes("stop")) return "warning";
    if (normalized.includes("download") || normalized.includes("active")) return "info";
    return "neutral";
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const [page, engine, dht] = await Promise.all([
        connection.client.listTorrents({ limit: 250 }),
        connection.client.getTorrentEngineStats().catch(() => null),
        connection.client.getTorrentDhtStats().catch(() => null),
      ]);
      torrents = page.items;
      engineStats = engine;
      dhtStats = dht;
      if (selectedId && !torrents.some((torrent) => torrent.job_id === selectedId)) selectedId = null;
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  async function loadDetails(id: string): Promise<void> {
    if (!connection.client) return;
    detailLoading = true;
    detailError = null;
    try {
      const [nextDetails, nextSnapshot, nextPeers, nextSeeding] = await Promise.all([
        connection.client.getTorrentDetails(id),
        connection.client.getTorrentStats(id).catch(() => null),
        connection.client.getTorrentPeers(id).catch(() => null),
        connection.client.getTorrentSeedingState(id).catch(() => null),
      ]);
      if (selectedId !== id) return;
      details = nextDetails;
      snapshot = nextSnapshot;
      peerStats = nextPeers;
      seeding = nextSeeding;
      selectedFiles = nextDetails.files.map((file) => file.index);
    } catch (cause) {
      if (selectedId === id) detailError = describeError(cause);
    } finally {
      if (selectedId === id) detailLoading = false;
    }
  }

  $effect(() => { void load(); });
  $effect(() => {
    if (!selectedId) {
      details = null;
      snapshot = null;
      peerStats = null;
      seeding = null;
      detailError = null;
      return;
    }
    detailTab = "overview";
    void loadDetails(selectedId);
  });

  function selectTorrent(id: string): void {
    selectedId = id;
  }

  function toggleFile(index: number): void {
    selectedFiles = selectedFiles.includes(index)
      ? selectedFiles.filter((value) => value !== index)
      : [...selectedFiles, index].sort((a, b) => a - b);
  }

  function toggleAllFiles(): void {
    selectedFiles = allFilesSelected ? [] : (details?.files.map((file) => file.index) ?? []);
  }

  async function saveFiles(): Promise<void> {
    if (!connection.client || !selectedId || savingFiles) return;
    savingFiles = true;
    try {
      await connection.client.updateTorrentFiles(selectedId, selectedFiles);
      notifications.success("Torrent file selection updated", `${selectedFiles.length} file${selectedFiles.length === 1 ? "" : "s"} selected.`);
      await loadDetails(selectedId);
    } catch (cause) {
      notifications.error("Couldn't update torrent files", describeError(cause));
    } finally {
      savingFiles = false;
    }
  }

  async function addPeers(): Promise<void> {
    if (!connection.client || !selectedId || peersBusy) return;
    const peers = peersInput.split(/\r?\n|,/).map((peer) => peer.trim()).filter(Boolean);
    if (peers.length === 0) return;
    peersBusy = true;
    peersError = null;
    try {
      await connection.client.addTorrentPeers(selectedId, peers);
      peersDialogOpen = false;
      peersInput = "";
      notifications.success(peers.length === 1 ? "Peer added" : `${peers.length} peers added`);
      await loadDetails(selectedId);
    } catch (cause) {
      peersError = describeError(cause);
    } finally {
      peersBusy = false;
    }
  }

  async function confirmRemove(): Promise<void> {
    if (!connection.client || !removeTarget) return;
    removeBusy = true;
    removeError = null;
    try {
      await connection.client.removeTorrent(removeTarget.job_id, deleteFiles);
      notifications.info(deleteFiles ? "Torrent and files removed" : "Torrent removed");
      removeTarget = null;
      selectedId = null;
      await load();
    } catch (cause) {
      removeError = describeError(cause);
    } finally {
      removeBusy = false;
    }
  }
</script>

<div class="page">
  <PageHeader title="Torrents" description="Engine activity, peers, seeding, and managed torrent downloads.">
    {#snippet actions()}
      <Button onclick={() => void load()}><Icon name="refresh" size={16} /> Refresh</Button>
      <Button variant="accent" onclick={() => navigation.requestAdd("torrent")}><Icon name="add" size={16} /> Add torrent</Button>
    {/snippet}
  </PageHeader>

  <div class="metrics">
    <MetricCard label="Download" value={formatSpeed(totalDown)} detail={`${engineStats?.active_torrents ?? torrents.length} active in engine`} icon="download" />
    <MetricCard label="Upload" value={formatSpeed(totalUp)} detail={`${formatBytes(engineStats?.uploaded_bytes ?? 0)} uploaded`} icon="upload" />
    <MetricCard label="Connected peers" value={peerCount.toLocaleString()} detail={`${dhtStats?.routing_table_size ?? 0} IPv4 · ${dhtStats?.routing_table_size_v6 ?? 0} IPv6 DHT nodes`} icon="peer" />
    <MetricCard label="Managed torrents" value={torrents.length.toLocaleString()} detail={`${formatBytes(engineStats?.downloaded_bytes ?? 0)} downloaded by engine`} icon="speed" />
  </div>

  <div class="toolbar"><SearchBox bind:value={search} label="Search torrents" placeholder="Search name or info hash" /></div>

  <div class="workspace" class:with-details={!!selected}>
    <Surface padding="none" class="torrent-list">
      {#if error}
        <div class="state"><InlineError title="Couldn't load torrents" message={error} retry={() => void load()} /></div>
      {:else if loading}
        <div class="state muted">Loading torrent engine…</div>
      {:else if visible.length === 0}
        <EmptyState icon="torrent" title="No torrents" message={search ? "No torrents match the current search." : "Torrent downloads will appear here after they are added."}>
          {#if !search}<Button variant="accent" onclick={() => (navigation.section = "downloads")}>Go to Downloads</Button>{/if}
        </EmptyState>
      {:else}
        <div class="header-row" aria-hidden="true"><span>Name</span><span>Progress</span><span>Down</span><span>Up</span><span>Peers</span><span>Status</span></div>
        <div class="rows">
          {#each visible as torrent (torrent.job_id)}
            {@const progress = torrent.total_bytes && torrent.total_bytes > 0 ? Math.min(100, torrent.downloaded_bytes / torrent.total_bytes * 100) : 0}
            <button type="button" class="torrent-row" class:selected={selectedId === torrent.job_id} onclick={() => selectTorrent(torrent.job_id)}>
              <span class="torrent-name"><span class="torrent-icon"><Icon name="torrent" size={19} /></span><span><strong>{torrent.name ?? torrent.info_hash ?? "Unnamed torrent"}</strong><small>{torrent.info_hash ?? torrent.torrent_id}</small></span></span>
              <span class="progress-cell"><span class="progress-track"><span style={`width:${progress}%`}></span></span><small>{progress.toFixed(0)}% · {formatBytes(torrent.downloaded_bytes)} / {formatBytes(torrent.total_bytes)}</small></span>
              <span>{formatSpeed(torrent.download_speed_bps)}</span>
              <span>{formatSpeed(torrent.upload_speed_bps)}</span>
              <span>{torrent.peers_connected}</span>
              <span><StatusBadge label={torrent.state} severity={severity(torrent.state)} /></span>
            </button>
          {/each}
        </div>
      {/if}
    </Surface>

    {#if selected}
      <aside class="details">
        <header><div><span class="detail-icon"><Icon name="torrent" size={22} /></span><h2>{selected.name ?? "Torrent details"}</h2></div><IconButton icon="close" label="Close details" variant="subtle" onclick={() => (selectedId = null)} /></header>
        <nav class="detail-tabs" aria-label="Torrent details">
          {#each ["overview", "files", "peers"] as tab}
            <button type="button" class:active={detailTab === tab} onclick={() => (detailTab = tab as DetailTab)}>{tab[0]?.toUpperCase()}{tab.slice(1)}</button>
          {/each}
        </nav>
        <div class="details-body">
          {#if detailError}
            <InlineError title="Couldn't load torrent details" message={detailError} retry={() => selectedId && void loadDetails(selectedId)} />
          {:else if detailLoading}
            <p class="muted">Loading details…</p>
          {:else if detailTab === "overview"}
            <dl>
              <dt>Status</dt><dd>{snapshot?.state ?? selected.state}</dd>
              <dt>Downloaded</dt><dd>{formatBytes(snapshot?.downloaded_bytes ?? selected.downloaded_bytes)}</dd>
              <dt>Uploaded</dt><dd>{formatBytes(snapshot?.uploaded_bytes ?? selected.uploaded_bytes)}</dd>
              <dt>Total size</dt><dd>{formatBytes(snapshot?.total_bytes ?? selected.total_bytes)}</dd>
              <dt>Download speed</dt><dd>{formatSpeed(snapshot?.download_speed_bps ?? selected.download_speed_bps)}</dd>
              <dt>Upload speed</dt><dd>{formatSpeed(snapshot?.upload_speed_bps ?? selected.upload_speed_bps)}</dd>
              <dt>Peers</dt><dd>{snapshot?.peers_connected ?? selected.peers_connected}</dd>
              <dt>Seeders / leechers</dt><dd>{snapshot?.seeders ?? selected.seeders} / {snapshot?.leechers ?? selected.leechers}</dd>
              {#if seeding}<dt>Seeding ratio</dt><dd>{seeding.last_ratio?.toFixed(2) ?? "—"}</dd><dt>Seeding since</dt><dd>{formatAbsoluteTime(seeding.started_at)}</dd>{/if}
              {#if selected.info_hash}<dt>Info hash</dt><dd class="mono">{selected.info_hash}</dd>{/if}
            </dl>
            <div class="detail-actions"><Button onclick={() => { removeTarget = selected; deleteFiles = false; }}><Icon name="trash" size={16} /> Remove torrent</Button></div>
          {:else if detailTab === "files"}
            {#if !details?.files.length}
              <EmptyState icon="file" title="No file information" message="The engine did not return a file list for this torrent." />
            {:else}
              <div class="file-command"><label><input type="checkbox" checked={allFilesSelected} onchange={toggleAllFiles} /> Select all</label><Button variant="accent" disabled={savingFiles || selectedFiles.length === 0} onclick={() => void saveFiles()}>{savingFiles ? "Saving…" : "Save selection"}</Button></div>
              <div class="file-list">
                {#each details.files as file (file.index)}
                  <label class="file-row"><input type="checkbox" checked={selectedFiles.includes(file.index)} onchange={() => toggleFile(file.index)} /><span><strong>{file.path.split(/[\\/]/).at(-1)}</strong><small>{file.path}</small></span><span>{formatBytes(file.size_bytes)}</span></label>
                {/each}
              </div>
            {/if}
          {:else}
            <div class="peer-command"><span>{peerStats?.peers.length ?? 0} peer{peerStats?.peers.length === 1 ? "" : "s"} reported</span><Button onclick={() => { peersDialogOpen = true; peersError = null; }}><Icon name="add" size={16} /> Add peers</Button></div>
            {#if !peerStats?.peers.length}
              <EmptyState icon="peer" title="No connected peers" message="Peers will appear here while the torrent is active." />
            {:else}
              <div class="peer-list">
                {#each peerStats.peers as peer, index (`${peer.address}-${index}`)}
                  <div class="peer-row"><span><strong>{peer.address ?? "Unknown address"}</strong><small>{peer.client ?? peer.state ?? "Unknown client"}</small></span><span>↓ {formatSpeed(peer.download_speed_bps)}</span><span>↑ {formatSpeed(peer.upload_speed_bps)}</span></div>
                {/each}
              </div>
            {/if}
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</div>

<ConfirmDialog
  open={!!removeTarget}
  title="Remove torrent?"
  message={deleteFiles ? "The torrent and downloaded files will be deleted." : "The torrent will be removed from the engine. Downloaded files will be kept."}
  confirmLabel="Remove"
  destructive
  busy={removeBusy}
  error={removeError}
  onConfirm={() => void confirmRemove()}
  onClose={() => !removeBusy && (removeTarget = null)}
>
  {#snippet details()}
    <ToggleSwitch bind:checked={deleteFiles} label="Delete downloaded files" description="This cannot be undone." />
  {/snippet}
</ConfirmDialog>

<Dialog open={peersDialogOpen} title="Add torrent peers" size="small" preventClose={peersBusy} onClose={() => !peersBusy && (peersDialogOpen = false)}>
  <TextArea bind:value={peersInput} label="Peer addresses" placeholder={"192.0.2.10:6881\n[2001:db8::1]:6881"} rows={6} hint="Enter one peer per line or separate addresses with commas." />
  {#if peersError}<div class="dialog-error"><InlineError title="Couldn't add peers" message={peersError} /></div>{/if}
  {#snippet footer()}<Button disabled={peersBusy} onclick={() => (peersDialogOpen = false)}>Cancel</Button><Button variant="accent" disabled={peersBusy || !peersInput.trim()} onclick={() => void addPeers()}>{peersBusy ? "Adding…" : "Add peers"}</Button>{/snippet}
</Dialog>

<style>
  .page { height: 100%; display: flex; flex-direction: column; min-width: 0; }
  .metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-3); padding: 0 var(--page-padding) var(--space-4); }
  .toolbar { padding: 0 var(--page-padding) var(--space-4); }
  .toolbar :global(.search-box) { width: min(520px, 100%); }
  .workspace { position: relative; display: grid; grid-template-columns: minmax(0, 1fr); flex: 1; min-height: 0; gap: var(--space-3); padding: 0 var(--page-padding) var(--page-padding); }
  .workspace.with-details { grid-template-columns: minmax(0, 1fr) minmax(340px, 390px); }
  :global(.torrent-list) { display: flex; flex-direction: column; min-height: 0; }
  .state { padding: var(--space-6); }
  .muted { color: var(--text-secondary); }
  .header-row, .torrent-row { display: grid; grid-template-columns: minmax(240px, 1.8fr) minmax(180px, 1fr) 90px 90px 60px 110px; gap: var(--space-3); align-items: center; }
  .header-row { min-height: 36px; padding: 0 var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .rows { min-height: 0; overflow: auto; }
  .torrent-row { width: 100%; min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); background: transparent; color: var(--text-primary); text-align: left; cursor: default; }
  .torrent-row:hover { background: var(--bg-subtle-hover); }
  .torrent-row.selected { background: var(--accent-subtle); box-shadow: inset 3px 0 var(--accent-default); }
  .torrent-name { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .torrent-icon, .detail-icon { display: grid; place-items: center; width: 34px; height: 34px; flex: none; border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  .torrent-name > span:last-child { display: flex; flex-direction: column; min-width: 0; }
  .torrent-name strong, .torrent-name small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .torrent-name strong { font-weight: 500; }
  .torrent-name small, .progress-cell small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .progress-cell { display: flex; flex-direction: column; gap: 4px; }
  .progress-track { height: 4px; border-radius: var(--radius-pill); background: var(--bg-subtle); overflow: hidden; }
  .progress-track span { display: block; height: 100%; border-radius: inherit; background: var(--accent-default); }
  .details { min-width: 0; overflow: hidden; display: flex; flex-direction: column; border: 1px solid var(--stroke-surface); border-radius: var(--radius-layer); background: var(--surface-card); }
  .details > header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .details > header > div { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .details h2 { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-body-strong); }
  .detail-tabs { display: flex; gap: var(--space-1); padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .detail-tabs button { min-height: 32px; padding: 0 var(--space-3); border: 0; border-radius: var(--radius-medium); color: var(--text-secondary); background: transparent; }
  .detail-tabs button:hover { background: var(--bg-subtle-hover); }
  .detail-tabs button.active { color: var(--text-primary); background: var(--accent-subtle); font-weight: 600; }
  .details-body { flex: 1; min-height: 0; padding: var(--space-4); overflow: auto; }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-4); margin: 0 0 var(--space-5); }
  dt { color: var(--text-secondary); } dd { margin: 0; } .mono { font: 12px/18px Consolas, monospace; word-break: break-all; }
  .detail-actions, .file-command, .peer-command { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  .file-command { position: sticky; top: calc(-1 * var(--space-4)); z-index: 1; margin: calc(-1 * var(--space-4)) calc(-1 * var(--space-4)) var(--space-2); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); background: var(--surface-card); }
  .file-command label { display: flex; align-items: center; gap: var(--space-2); }
  .file-list, .peer-list { display: flex; flex-direction: column; }
  .file-row, .peer-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); min-height: 48px; padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .file-row { grid-template-columns: auto minmax(0, 1fr) auto; }
  .file-row > span, .peer-row > span:first-child { display: flex; min-width: 0; flex-direction: column; }
  .file-row strong, .file-row small, .peer-row strong, .peer-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-row small, .peer-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .peer-command { margin-bottom: var(--space-3); color: var(--text-secondary); }
  .peer-row { grid-template-columns: minmax(0, 1fr) auto auto; font-size: var(--text-caption); }
  .dialog-error { margin-top: var(--space-4); }
  @media (max-width: 1200px) { .metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); } .header-row, .torrent-row { grid-template-columns: minmax(230px, 1.6fr) minmax(170px, 1fr) 90px 70px 100px; } .header-row span:nth-child(4), .torrent-row > span:nth-child(4) { display: none; } }
  @media (max-width: 900px) { .workspace.with-details { grid-template-columns: minmax(0, 1fr); } .details { position: absolute; inset: 0 var(--page-padding) var(--page-padding); z-index: 20; background: var(--surface-overlay); backdrop-filter: blur(30px); } .header-row, .torrent-row { grid-template-columns: minmax(0, 1fr) 100px; } .header-row span:nth-child(n+2):not(:last-child), .torrent-row > span:nth-child(n+2):not(:last-child) { display: none; } }
  @media (max-width: 620px) { .metrics { grid-template-columns: 1fr; } }
</style>
