<script lang="ts">
  import { describeError } from "../api/errors";
  import type {
    TorrentDetails,
    TorrentGlobalStats,
    TorrentPeerStats,
    TorrentRecord,
    TorrentSeedingState,
    TorrentSnapshot,
  } from "../api/types";
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import CompactSummary, { type SummaryItem } from "../components/CompactSummary.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import DetailsPane from "../components/DetailsPane.svelte";
  import Dialog from "../components/Dialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import ListDetailsLayout from "../components/ListDetailsLayout.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import type { MenuItem } from "../components/Menu.svelte";
  import PageCommandBar from "../components/PageCommandBar.svelte";
  import PageScaffold from "../components/PageScaffold.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import TextArea from "../components/TextArea.svelte";
  import TextField from "../components/TextField.svelte";
  import { promptTorrentDefaultApp } from "../native/tauri";
  import { connection } from "../stores/connection.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime, formatBytes, formatSpeed } from "../util/format";
  import TorrentEngineDialog from "./TorrentEngineDialog.svelte";
  import TorrentFileTree from "./TorrentFileTree.svelte";
  import {
    buildTorrentFileTree,
    extractTrackers,
    formatTorrentEta,
    torrentEtaSeconds,
    torrentProgress,
    torrentRatio,
    type TorrentDetailTab,
  } from "./torrentPresentation";

  type RemoveMode = "keep" | "delete";

  let torrents = $state<TorrentRecord[]>([]);
  let engineStats = $state<TorrentGlobalStats | null>(null);
  let search = $state("");
  let loading = $state(true);
  let error = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let detailTab = $state<TorrentDetailTab>("overview");
  let torrentDetails = $state<TorrentDetails | null>(null);
  let snapshot = $state<TorrentSnapshot | null>(null);
  let peerStats = $state<TorrentPeerStats | null>(null);
  let seeding = $state<TorrentSeedingState | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);

  let fileSearch = $state("");
  let selectedFiles = $state<number[]>([]);
  let savingFiles = $state(false);

  let removeTarget = $state<TorrentRecord | null>(null);
  let removeMode = $state<RemoveMode>("keep");
  let removeBusy = $state(false);
  let removeError = $state<string | null>(null);

  let engineDialogOpen = $state(false);
  let peersDialogOpen = $state(false);
  let peersInput = $state("");
  let peersBusy = $state(false);
  let peersError = $state<string | null>(null);
  let defaultAppBusy = $state(false);

  const visible = $derived(
    search.trim()
      ? torrents.filter((torrent) => `${torrent.name ?? ""} ${torrent.info_hash ?? ""} ${torrent.state}`.toLowerCase().includes(search.toLowerCase()))
      : torrents,
  );
  const selected = $derived(torrents.find((torrent) => torrent.job_id === selectedId) ?? null);
  const totalDown = $derived(engineStats?.download_speed_bps ?? torrents.reduce((sum, torrent) => sum + torrent.download_speed_bps, 0));
  const totalUp = $derived(engineStats?.upload_speed_bps ?? torrents.reduce((sum, torrent) => sum + torrent.upload_speed_bps, 0));
  const peerCount = $derived(torrents.reduce((sum, torrent) => sum + torrent.peers_connected, 0));
  const activeCount = $derived(torrents.filter((torrent) => ["downloading", "active", "queued"].some((state) => torrent.state.toLowerCase().includes(state))).length);
  const managedHashes = $derived(new Set(torrents.map((torrent) => torrent.info_hash?.toLowerCase()).filter((hash): hash is string => !!hash)));
  const fileTree = $derived(buildTorrentFileTree(torrentDetails?.files ?? [], fileSearch));
  const selectedSize = $derived((torrentDetails?.files ?? []).filter((file) => selectedFiles.includes(file.index)).reduce((sum, file) => sum + (file.size_bytes ?? 0), 0));
  const allFilesSelected = $derived(!!torrentDetails?.files.length && selectedFiles.length === torrentDetails.files.length);
  const trackers = $derived(extractTrackers(torrentDetails?.raw));
  const summaryItems = $derived<SummaryItem[]>([
    { label: engineStats ? "engine ready" : "engine status unavailable", value: engineStats ? "Ready" : "Unknown", tone: engineStats ? "success" : "warning" },
    { label: "down", value: formatSpeed(totalDown) },
    { label: "up", value: formatSpeed(totalUp) },
    { label: "peers", value: peerCount.toLocaleString() },
    { label: "active", value: activeCount.toLocaleString() },
  ]);

  const detailTabs = [
    { id: "overview", label: "Overview" },
    { id: "files", label: "Files" },
    { id: "peers", label: "Peers" },
    { id: "trackers", label: "Trackers" },
    { id: "advanced", label: "Advanced" },
  ];

  function severity(state: string): "neutral" | "info" | "success" | "warning" | "error" {
    const normalized = state.toLowerCase();
    if (normalized.includes("error") || normalized.includes("fail")) return "error";
    if (normalized.includes("seed") || normalized.includes("complete")) return "success";
    if (normalized.includes("pause") || normalized.includes("stop")) return "warning";
    if (normalized.includes("download") || normalized.includes("active") || normalized.includes("queue")) return "info";
    return "neutral";
  }

  async function promptTorrentDefault(): Promise<void> {
    if (defaultAppBusy) return;
    defaultAppBusy = true;
    try {
      await promptTorrentDefaultApp();
      notifications.info("Choose Ravyn for torrent files", "Windows Default Apps is open. Select Ravyn for .torrent files and magnet links.");
    } catch (cause) {
      notifications.error("Couldn't open torrent defaults", describeError(cause));
    } finally {
      defaultAppBusy = false;
    }
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const [page, engine] = await Promise.all([
        connection.client.listTorrents({ limit: 250 }),
        connection.client.getTorrentEngineStats().catch(() => null),
      ]);
      torrents = page.items;
      engineStats = engine;
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
      torrentDetails = nextDetails;
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
    fileSearch = "";
    detailTab = "overview";
    if (!selectedId) {
      torrentDetails = null;
      snapshot = null;
      peerStats = null;
      seeding = null;
      detailError = null;
      return;
    }
    void loadDetails(selectedId);
  });

  function toggleFile(index: number): void {
    selectedFiles = selectedFiles.includes(index)
      ? selectedFiles.filter((value) => value !== index)
      : [...selectedFiles, index].sort((a, b) => a - b);
  }

  function toggleFolder(indexes: number[], checked: boolean): void {
    const next = new Set(selectedFiles);
    for (const index of indexes) {
      if (checked) next.add(index);
      else next.delete(index);
    }
    selectedFiles = [...next].sort((a, b) => a - b);
  }

  function toggleAllFiles(): void {
    selectedFiles = allFilesSelected ? [] : (torrentDetails?.files.map((file) => file.index) ?? []);
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

  function requestRemove(target: TorrentRecord, mode: RemoveMode): void {
    removeTarget = target;
    removeMode = mode;
    removeError = null;
  }

  async function confirmRemove(): Promise<void> {
    if (!connection.client || !removeTarget) return;
    removeBusy = true;
    removeError = null;
    try {
      await connection.client.removeTorrent(removeTarget.job_id, removeMode === "delete");
      notifications.info(removeMode === "delete" ? "Torrent and downloaded files removed" : "Torrent removed; downloaded files kept");
      removeTarget = null;
      selectedId = null;
      await load();
    } catch (cause) {
      removeError = describeError(cause);
    } finally {
      removeBusy = false;
    }
  }

  function moreItems(): MenuItem[] {
    return [
      { id: "engine", label: "Torrent engine details", icon: "diagnostics", onSelect: () => (engineDialogOpen = true) },
      { id: "refresh", label: "Refresh", icon: "refresh", separatorBefore: true, onSelect: () => void load() },
    ];
  }
</script>

<PageScaffold title="Torrents" summary="Managed torrent downloads, selected files, peers, trackers, and seeding.">
  {#snippet actions()}
    <Button variant="subtle" disabled={defaultAppBusy} onclick={() => void promptTorrentDefault()}><Icon name="settings" size={16} /> {defaultAppBusy ? "Opening defaults…" : "Set as default"}</Button>
    <Button variant="accent" onclick={() => navigation.requestAdd("torrent")}><Icon name="add" size={16} /> Add torrent</Button>
  {/snippet}

  {#snippet commandBar()}
    <PageCommandBar ariaLabel="Torrent commands">
      {#snippet leading()}
        <span class="managed-label"><Icon name="torrent" size={16} /> Managed torrents</span>
      {/snippet}
      {#snippet actions()}
        <SearchBox bind:value={search} label="Search torrents" placeholder="Search name, info hash, or state" />
        <MenuButton label="More" icon="more" items={moreItems()} variant="subtle" />
      {/snippet}
    </PageCommandBar>
  {/snippet}

  {#snippet status()}
    <div class="status-strip"><CompactSummary items={summaryItems} ariaLabel="Torrent summary" /></div>
  {/snippet}

  <div class="workspace">
    <ListDetailsLayout detailsOpen={!!selected} detailsLabel="Torrent details" detailsWidth="460px">
      {#snippet list()}
        <Surface padding="none" class="torrent-list">
          {#if error}
            <div class="state"><InlineError title="Couldn't load torrents" message={error} retry={() => void load()} /></div>
          {:else if loading}
            <div class="state muted">Loading torrent engine…</div>
          {:else if visible.length === 0}
            <EmptyState icon="torrent" title="No torrents" message={search ? "No torrents match the current search." : "Torrent downloads will appear here after they are added."}>
              {#if !search}<Button variant="accent" onclick={() => navigation.requestAdd("torrent")}>Add torrent</Button>{/if}
            </EmptyState>
          {:else}
            <div class="header-row" aria-hidden="true"><span>Name</span><span>Progress</span><span>Down</span><span>Up</span><span>ETA / Ratio</span><span>State</span></div>
            <div class="rows" role="listbox" aria-label="Managed torrents">
              {#each visible as torrent (torrent.job_id)}
                {@const progress = torrentProgress(torrent)}
                {@const ratio = torrentRatio(torrent.uploaded_bytes, torrent.downloaded_bytes)}
                <button type="button" class="torrent-row" class:selected={selectedId === torrent.job_id} role="option" aria-selected={selectedId === torrent.job_id} onclick={() => (selectedId = torrent.job_id)}>
                  <span class="torrent-name"><span class="torrent-icon"><Icon name="torrent" size={18} /></span><span><strong>{torrent.name ?? torrent.info_hash ?? "Unnamed torrent"}</strong><small>{torrent.info_hash ?? torrent.torrent_id}</small></span></span>
                  <span class="progress-cell"><span class="progress-track"><span style={`width:${progress}%`}></span></span><small>{progress.toFixed(0)}% · {formatBytes(torrent.downloaded_bytes)} / {formatBytes(torrent.total_bytes)}</small></span>
                  <span>{formatSpeed(torrent.download_speed_bps)}</span>
                  <span>{formatSpeed(torrent.upload_speed_bps)}</span>
                  <span class="eta-cell"><strong>{formatTorrentEta(torrentEtaSeconds(torrent))}</strong><small>Ratio {ratio === null ? "∞" : ratio.toFixed(2)}</small></span>
                  <span><StatusBadge label={torrent.state} severity={severity(torrent.state)} /></span>
                </button>
              {/each}
            </div>
          {/if}
        </Surface>
      {/snippet}

      {#snippet details()}
        {#if selected}
          <DetailsPane
            title={selected.name ?? "Torrent details"}
            subtitle={selected.info_hash ?? selected.torrent_id}
            icon="torrent"
            tabs={detailTabs}
            bind:selectedTab={detailTab}
            onClose={() => (selectedId = null)}
          >
            {#if detailError}
              <InlineError title="Couldn't load torrent details" message={detailError} retry={() => selectedId && void loadDetails(selectedId)} />
            {:else if detailLoading}
              <p class="muted">Loading torrent details…</p>
            {:else if detailTab === "overview"}
              <div class="detail-stack">
                <div class="overview-progress">
                  <span class="progress-track large"><span style={`width:${torrentProgress(selected)}%`}></span></span>
                  <strong>{torrentProgress(selected).toFixed(0)}%</strong>
                  <small>{formatBytes(snapshot?.downloaded_bytes ?? selected.downloaded_bytes)} of {formatBytes(snapshot?.total_bytes ?? selected.total_bytes)}</small>
                </div>
                <dl>
                  <dt>State</dt><dd>{snapshot?.state ?? selected.state}</dd>
                  <dt>Download speed</dt><dd>{formatSpeed(snapshot?.download_speed_bps ?? selected.download_speed_bps)}</dd>
                  <dt>Upload speed</dt><dd>{formatSpeed(snapshot?.upload_speed_bps ?? selected.upload_speed_bps)}</dd>
                  <dt>ETA</dt><dd>{formatTorrentEta(torrentEtaSeconds(selected))}</dd>
                  <dt>Uploaded</dt><dd>{formatBytes(snapshot?.uploaded_bytes ?? selected.uploaded_bytes)}</dd>
                  <dt>Ratio</dt><dd>{torrentRatio(snapshot?.uploaded_bytes ?? selected.uploaded_bytes, snapshot?.downloaded_bytes ?? selected.downloaded_bytes)?.toFixed(2) ?? "∞"}</dd>
                  <dt>Peers</dt><dd>{snapshot?.peers_connected ?? selected.peers_connected}</dd>
                  <dt>Seeders / leechers</dt><dd>{snapshot?.seeders ?? selected.seeders} / {snapshot?.leechers ?? selected.leechers}</dd>
                </dl>
                <div class="remove-actions">
                  <Button onclick={() => requestRemove(selected, "keep")}><Icon name="trash" size={16} /> Remove and keep files</Button>
                  <Button variant="subtle" onclick={() => requestRemove(selected, "delete")}><Icon name="trash" size={16} /> Remove and delete files</Button>
                </div>
              </div>
            {:else if detailTab === "files"}
              {#if !torrentDetails?.files.length}
                <EmptyState icon="file" title="No file information" message="The engine did not return a file list for this torrent." />
              {:else}
                <div class="file-command">
                  <div class="file-search"><TextField bind:value={fileSearch} label="Search files" placeholder="Filter file paths" /></div>
                  <div class="file-selection-summary">
                    <label><input type="checkbox" checked={allFilesSelected} onchange={toggleAllFiles} /> Select all</label>
                    <span>{selectedFiles.length} of {torrentDetails.files.length} · {formatBytes(selectedSize)}</span>
                    <Button variant="accent" disabled={savingFiles || selectedFiles.length === 0} onclick={() => void saveFiles()}>{savingFiles ? "Saving…" : "Save selection"}</Button>
                  </div>
                </div>
                {#if fileTree.descendantFileIndexes.length === 0}
                  <EmptyState icon="search" title="No matching files" message="No torrent file paths match the current search." />
                {:else}
                  <TorrentFileTree root={fileTree} {selectedFiles} onToggleFile={toggleFile} onToggleFolder={toggleFolder} />
                {/if}
              {/if}
            {:else if detailTab === "peers"}
              <div class="peer-command"><span>{peerStats?.peers.length ?? 0} connected peer{peerStats?.peers.length === 1 ? "" : "s"}</span><Button onclick={() => { peersDialogOpen = true; peersError = null; }}><Icon name="add" size={16} /> Add peers</Button></div>
              {#if !peerStats?.peers.length}
                <EmptyState icon="peer" title="No connected peers" message="Peers will appear here while the torrent is active." />
              {:else}
                <div class="peer-list">
                  {#each peerStats.peers as peer, index (`${peer.address}-${index}`)}
                    <div class="peer-row"><span><strong>{peer.address ?? "Unknown address"}</strong><small>{peer.client ?? peer.state ?? "Unknown client"}</small></span><span>↓ {formatSpeed(peer.download_speed_bps)}</span><span>↑ {formatSpeed(peer.upload_speed_bps)}</span></div>
                  {/each}
                </div>
              {/if}
            {:else if detailTab === "trackers"}
              {#if trackers.length === 0}
                <EmptyState icon="cloud" title="No tracker information" message="The torrent engine did not expose tracker URLs for this torrent." />
              {:else}
                <div class="tracker-list">
                  {#each trackers as tracker (tracker)}
                    <div class="tracker-row"><Icon name="cloud" size={16} /><span>{tracker}</span></div>
                  {/each}
                </div>
              {/if}
            {:else}
              <div class="detail-stack">
                <dl>
                  <dt>Torrent ID</dt><dd class="mono">{torrentDetails?.torrent_id ?? selected.torrent_id}</dd>
                  {#if selected.info_hash}<dt>Info hash</dt><dd class="mono">{selected.info_hash}</dd>{/if}
                  {#if seeding}<dt>Seeding since</dt><dd>{formatAbsoluteTime(seeding.started_at)}</dd><dt>Last ratio</dt><dd>{seeding.last_ratio?.toFixed(2) ?? "—"}</dd><dt>Stop reason</dt><dd>{seeding.stop_reason ?? "Still seeding"}</dd>{/if}
                </dl>
                <AdvancedDisclosure title="Raw engine response" description="Technical data for troubleshooting and bug reports.">
                  <pre class="raw-json">{JSON.stringify(torrentDetails?.raw ?? selected.raw, null, 2)}</pre>
                </AdvancedDisclosure>
              </div>
            {/if}
          </DetailsPane>
        {/if}
      {/snippet}
    </ListDetailsLayout>
  </div>
</PageScaffold>

<ConfirmDialog
  open={!!removeTarget}
  title={removeMode === "delete" ? "Remove torrent and delete files?" : "Remove torrent and keep files?"}
  message={removeMode === "delete" ? "The torrent is removed from the engine and all downloaded files are deleted. This cannot be undone." : "The torrent is removed from the engine. Downloaded files remain on disk."}
  confirmLabel={removeMode === "delete" ? "Remove and delete files" : "Remove and keep files"}
  destructive
  busy={removeBusy}
  error={removeError}
  onConfirm={() => void confirmRemove()}
  onClose={() => !removeBusy && (removeTarget = null)}
/>

<Dialog open={peersDialogOpen} title="Add torrent peers" size="small" preventClose={peersBusy} onClose={() => !peersBusy && (peersDialogOpen = false)}>
  <TextArea bind:value={peersInput} label="Peer addresses" placeholder={"192.0.2.10:6881\n[2001:db8::1]:6881"} rows={6} hint="Enter one peer per line or separate addresses with commas." />
  {#if peersError}<div class="dialog-error"><InlineError title="Couldn't add peers" message={peersError} /></div>{/if}
  {#snippet footer()}<Button disabled={peersBusy} onclick={() => (peersDialogOpen = false)}>Cancel</Button><Button variant="accent" disabled={peersBusy || !peersInput.trim()} onclick={() => void addPeers()}>{peersBusy ? "Adding…" : "Add peers"}</Button>{/snippet}
</Dialog>

<TorrentEngineDialog open={engineDialogOpen} {managedHashes} onClose={() => (engineDialogOpen = false)} />

<style>
  .workspace { height: 100%; min-height: 0; padding: 0 var(--page-padding) var(--page-padding); }
  .status-strip { min-height: 38px; display: flex; align-items: center; padding: 0 var(--page-padding); border-bottom: 1px solid var(--stroke-divider); }
  .managed-label { display: inline-flex; align-items: center; gap: var(--space-2); color: var(--text-secondary); font-weight: 600; }
  :global(.torrent-list) { height: 100%; min-height: 0; display: flex; flex-direction: column; border-radius: 0; border-color: var(--stroke-divider); background: var(--surface-content); }
  .state { padding: var(--space-6); }
  .muted { color: var(--text-secondary); }
  .header-row, .torrent-row { display: grid; grid-template-columns: minmax(250px, 1.8fr) minmax(180px, 1fr) 90px 90px 110px 120px; gap: var(--space-3); align-items: center; }
  .header-row { min-height: 36px; padding: 0 var(--space-3); border-bottom: 1px solid var(--stroke-divider); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 600; }
  .rows { flex: 1; min-height: 0; overflow: auto; }
  .torrent-row { width: 100%; min-height: var(--row-height); padding: var(--row-padding-v) var(--space-3); border: 0; border-bottom: 1px solid var(--stroke-divider); background: transparent; color: var(--text-primary); text-align: left; cursor: default; }
  .torrent-row:hover { background: var(--bg-subtle-hover); }
  .torrent-row.selected { background: color-mix(in srgb, var(--accent-subtle) 52%, transparent); box-shadow: inset 2px 0 var(--accent-default); }
  .torrent-name { display: flex; align-items: center; min-width: 0; gap: var(--space-3); }
  .torrent-icon { width: 30px; height: 30px; flex: none; display: grid; place-items: center; color: var(--text-secondary); }
  .torrent-name > span:last-child, .eta-cell { display: flex; flex-direction: column; min-width: 0; }
  .torrent-name strong, .torrent-name small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .torrent-name strong { font-weight: 500; }
  .torrent-name small, .progress-cell small, .eta-cell small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .eta-cell strong { font-weight: 500; }
  .progress-cell { display: flex; flex-direction: column; gap: 4px; }
  .progress-track { height: 4px; border-radius: var(--radius-pill); background: var(--bg-subtle); overflow: hidden; }
  .progress-track.large { height: 6px; }
  .progress-track span { display: block; height: 100%; border-radius: inherit; background: var(--accent-default); }
  .detail-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .overview-progress { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--space-2); align-items: center; }
  .overview-progress small { grid-column: 1 / -1; color: var(--text-tertiary); }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-4); margin: 0; }
  dt { color: var(--text-secondary); }
  dd { min-width: 0; margin: 0; }
  .mono { font: 12px/18px Consolas, ui-monospace, monospace; word-break: break-all; }
  .remove-actions { display: flex; flex-direction: column; align-items: stretch; gap: var(--space-2); padding-top: var(--space-3); border-top: 1px solid var(--stroke-divider); }
  .file-command { position: sticky; top: calc(-1 * var(--space-4)); z-index: 2; margin: calc(-1 * var(--space-4)) calc(-1 * var(--space-4)) var(--space-2); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); background: var(--surface-content); }
  .file-search { margin-bottom: var(--space-3); }
  .file-selection-summary { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); color: var(--text-secondary); font-size: var(--text-caption); }
  .file-selection-summary label { display: flex; align-items: center; gap: var(--space-2); }
  .peer-command { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); margin-bottom: var(--space-3); color: var(--text-secondary); }
  .peer-list, .tracker-list { display: flex; flex-direction: column; }
  .peer-row { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); min-height: 50px; border-bottom: 1px solid var(--stroke-divider); font-size: var(--text-caption); }
  .peer-row > span:first-child { min-width: 0; display: flex; flex-direction: column; }
  .peer-row strong, .peer-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .peer-row small { color: var(--text-tertiary); }
  .tracker-row { min-height: 48px; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .tracker-row span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 12px/18px Consolas, ui-monospace, monospace; }
  .raw-json { max-height: 360px; margin: 0; padding: var(--space-3); overflow: auto; border: 1px solid var(--stroke-divider); border-radius: var(--radius-control); background: var(--bg-subtle); font-size: var(--text-caption); }
  .dialog-error { margin-top: var(--space-4); }
  @media (max-width: 1240px) {
    .header-row, .torrent-row { grid-template-columns: minmax(230px, 1.7fr) minmax(170px, 1fr) 90px 110px 110px; }
    .header-row span:nth-child(4), .torrent-row > span:nth-child(4) { display: none; }
  }
  @media (max-width: 820px) {
    .header-row { display: none; }
    .torrent-row { grid-template-columns: minmax(0, 1fr) auto; }
    .torrent-row > span:nth-child(2), .torrent-row > span:nth-child(3), .torrent-row > span:nth-child(4), .torrent-row > span:nth-child(5) { display: none; }
    .file-selection-summary { align-items: flex-start; flex-direction: column; }
  }
</style>
