<script lang="ts">
  import { describeError } from "../api/errors";
  import type { BasketItem, JobKind } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import Surface from "../components/Surface.svelte";
  import TextArea from "../components/TextArea.svelte";
  import TextField from "../components/TextField.svelte";
  import { connection } from "../stores/connection.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime } from "../util/format";

  let { embedded = false }: { embedded?: boolean } = $props();

  let items = $state<BasketItem[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let addOpen = $state(false);
  let sources = $state("");
  let destination = $state("");
  let kind = $state("http");
  let addBusy = $state(false);
  let editingItem = $state<BasketItem | null>(null);
  let editSource = $state("");
  let editDestination = $state("");
  let editFilename = $state("");
  let editKind = $state("http");
  let editBusy = $state(false);
  let clearOpen = $state(false);
  let clearBusy = $state(false);
  let clearError = $state<string | null>(null);
  let startBusy = $state(false);
  let reorderBusy = $state(false);

  const sourceLines = $derived(sources.split(/\r?\n/).map((line) => line.trim()).filter(Boolean));
  const kindOptions: DropdownOption[] = [
    { value: "http", label: "Direct download" },
    { value: "media", label: "Media" },
    { value: "torrent", label: "Torrent" },
  ];

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      items = await connection.client.listBasket();
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  $effect(() => { void load(); });

  async function addItems(): Promise<void> {
    if (!connection.client || sourceLines.length === 0) return;
    addBusy = true;
    let created = 0;
    const failures: string[] = [];
    for (const source of sourceLines) {
      try {
        await connection.client.addBasketItem({
          kind: kind as JobKind,
          source,
          destination: destination || null,
          options: {},
        });
        created += 1;
      } catch (cause) {
        failures.push(describeError(cause));
      }
    }
    addBusy = false;
    if (created > 0) {
      notifications.success(`${created} item${created === 1 ? "" : "s"} added to the basket`, failures.length ? `${failures.length} failed` : undefined);
      addOpen = false;
      sources = "";
      await load();
    } else if (failures.length) {
      notifications.error("Couldn't add items to the basket", failures[0]);
    }
  }

  function openEditor(item: BasketItem): void {
    editingItem = item;
    editSource = item.request.source;
    editDestination = item.request.destination ?? "";
    editFilename = item.request.filename ?? "";
    editKind = item.request.kind;
  }

  async function saveItem(): Promise<void> {
    if (!connection.client || !editingItem || !editSource.trim() || editBusy) return;
    editBusy = true;
    try {
      // Carry over request fields the editor does not surface.
      const updated = await connection.client.updateBasketItem(
        editingItem.id,
        {
          ...editingItem.request,
          kind: editKind as JobKind,
          source: editSource.trim(),
          destination: editDestination.trim() || null,
          filename: editFilename.trim() || null,
        },
        editingItem.preset_id,
      );
      items = items.map((item) => (item.id === updated.id ? updated : item));
      editingItem = null;
      notifications.success("Basket item updated");
    } catch (cause) {
      notifications.error("Couldn't update the basket item", describeError(cause));
    } finally {
      editBusy = false;
    }
  }

  async function removeItem(id: string): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.deleteBasketItem(id);
      items = items.filter((item) => item.id !== id);
      notifications.info("Basket item removed");
    } catch (cause) {
      notifications.error("Couldn't remove the basket item", describeError(cause));
    }
  }


  async function moveItem(index: number, direction: -1 | 1): Promise<void> {
    if (!connection.client || reorderBusy) return;
    const target = index + direction;
    if (target < 0 || target >= items.length) return;
    const reordered = [...items];
    const currentItem = reordered[index];
    const targetItem = reordered[target];
    if (!currentItem || !targetItem) return;
    reordered[index] = targetItem;
    reordered[target] = currentItem;
    reorderBusy = true;
    try {
      items = await connection.client.reorderBasket(reordered.map((item) => item.id));
    } catch (cause) {
      notifications.error("Couldn't reorder the basket", describeError(cause));
      await load();
    } finally {
      reorderBusy = false;
    }
  }

  async function startBasket(): Promise<void> {
    if (!connection.client || items.length === 0) return;
    startBusy = true;
    try {
      const result = await connection.client.startBasket();
      notifications.success(`${result.started} download${result.started === 1 ? "" : "s"} started`, result.failed ? `${result.failed} item(s) failed` : undefined);
      await load();
      navigation.navigate("downloads");
    } catch (cause) {
      notifications.error("Couldn't start the basket", describeError(cause));
    } finally {
      startBusy = false;
    }
  }

  async function clearBasket(): Promise<void> {
    if (!connection.client) return;
    clearBusy = true;
    clearError = null;
    try {
      await connection.client.clearBasket();
      items = [];
      clearOpen = false;
      notifications.info("Basket cleared");
    } catch (cause) {
      clearError = describeError(cause);
    } finally {
      clearBusy = false;
    }
  }
</script>

<div class="page">
  {#if !embedded}
  <PageHeader title="Basket" description="Prepare a group of downloads, review it, then start everything together.">
    {#snippet actions()}
      {#if items.length}<Button onclick={() => (clearOpen = true)}><Icon name="trash" size={16} /> Clear</Button>{/if}
      <Button variant="accent" disabled={startBusy || items.length === 0} onclick={() => void startBasket()}><Icon name="play" size={16} /> {startBusy ? "Starting…" : `Start ${items.length || "basket"}`}</Button>
    {/snippet}
  </PageHeader>
  {/if}

  <div class="content">
    <Surface padding="none" class="basket-surface">
      <div class="basket-toolbar">
        <div class="basket-summary"><strong>{items.length} queued item{items.length === 1 ? "" : "s"}</strong><span>Items remain editable until the basket is started.</span></div>
        <div class="basket-actions">
          {#if embedded}
            {#if items.length}<Button onclick={() => (clearOpen = true)}><Icon name="trash" size={16} /> Clear</Button>{/if}
            <Button onclick={() => (addOpen = true)}><Icon name="add" size={16} /> Add</Button>
            <Button variant="accent" disabled={startBusy || items.length === 0} onclick={() => void startBasket()}><Icon name="play" size={16} /> {startBusy ? "Startingâ€¦" : `Start ${items.length || "basket"}`}</Button>
          {:else}
            <Button onclick={() => (addOpen = true)}><Icon name="add" size={16} /> Add to basket</Button>
          {/if}
        </div>
      </div>

      {#if error}
        <div class="state"><InlineError title="Couldn't load the basket" message={error} retry={() => void load()} /></div>
      {:else if loading}
        <div class="state muted">Loading basket…</div>
      {:else if items.length === 0}
        <EmptyState icon="basket" title="Your basket is empty" message="Add URLs here when you want to review a group before downloading it.">
          <Button variant="accent" onclick={() => (addOpen = true)}>Add URLs</Button>
        </EmptyState>
      {:else}
        <div class="basket-list">
          {#each items as item, index (item.id)}
            <article class="basket-item">
              <span class="position">{index + 1}</span>
              <span class="kind-icon"><Icon name={item.request.kind === "torrent" ? "torrent" : item.request.kind === "media" ? "video" : "download"} size={19} /></span>
              <div class="item-copy">
                <strong>{item.request.filename ?? item.request.source}</strong>
                <span>{item.request.source}</span>
                <small>{item.request.kind} · added {formatAbsoluteTime(item.created_at)}</small>
              </div>
              <div class="destination"><span>Destination</span><strong>{item.request.destination ?? "Library default"}</strong></div>
              <div class="row-actions">
                <IconButton icon="chevron-up" label="Move item up" variant="subtle" disabled={reorderBusy || index === 0} onclick={() => void moveItem(index, -1)} />
                <IconButton icon="chevron-down" label="Move item down" variant="subtle" disabled={reorderBusy || index === items.length - 1} onclick={() => void moveItem(index, 1)} />
                <IconButton icon="edit" label="Edit basket item" variant="subtle" onclick={() => openEditor(item)} />
                <IconButton icon="trash" label="Remove from basket" variant="subtle" onclick={() => void removeItem(item.id)} />
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </Surface>
  </div>
</div>

<Dialog open={addOpen} title="Add to basket" onClose={() => !addBusy && (addOpen = false)} preventClose={addBusy}>
  <div class="form">
    <TextArea bind:value={sources} label="URL or URLs" rows={5} placeholder={"https://example.com/file.zip\nhttps://example.com/video"} hint="One source per line." />
    <div class="field"><span>Download type</span><Dropdown options={kindOptions} bind:value={kind} label="Download type" /></div>
    <PathPicker bind:value={destination} label="Destination" placeholder="Use the library default" />
  </div>
  {#snippet footer()}
    <Button disabled={addBusy} onclick={() => (addOpen = false)}>Cancel</Button>
    <Button variant="accent" disabled={addBusy || sourceLines.length === 0} onclick={() => void addItems()}>{addBusy ? "Adding…" : `Add ${sourceLines.length || "items"}`}</Button>
  {/snippet}
</Dialog>

<Dialog open={!!editingItem} title="Edit basket item" onClose={() => !editBusy && (editingItem = null)} preventClose={editBusy}>
  <div class="form">
    <TextArea bind:value={editSource} label="Source" rows={2} />
    <div class="field"><span>Download type</span><Dropdown options={kindOptions} bind:value={editKind} label="Download type" /></div>
    <PathPicker bind:value={editDestination} label="Destination" placeholder="Use the library default" />
    <TextField bind:value={editFilename} label="File name" placeholder="Detected automatically" />
  </div>
  {#snippet footer()}
    <Button disabled={editBusy} onclick={() => (editingItem = null)}>Cancel</Button>
    <Button variant="accent" disabled={editBusy || !editSource.trim()} onclick={() => void saveItem()}>{editBusy ? "Saving…" : "Save item"}</Button>
  {/snippet}
</Dialog>

<ConfirmDialog
  open={clearOpen}
  title="Clear the basket?"
  message="All prepared basket items will be removed. Existing downloads are not affected."
  confirmLabel="Clear basket"
  destructive
  busy={clearBusy}
  error={clearError}
  onConfirm={() => void clearBasket()}
  onClose={() => !clearBusy && (clearOpen = false)}
/>

<style>
  .page { height: 100%; display: flex; flex-direction: column; }
  .content { flex: 1; min-height: 0; padding: 0 var(--page-padding) var(--page-padding); }
  :global(.basket-surface) { height: 100%; display: flex; flex-direction: column; }
  .basket-toolbar { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .basket-summary { display: flex; flex-direction: column; }
  .basket-actions { display: flex; align-items: center; gap: var(--space-2); }
  .basket-toolbar span, .muted { color: var(--text-secondary); }
  .state { padding: var(--space-6); }
  .basket-list { min-height: 0; overflow: auto; }
  .basket-item { display: grid; grid-template-columns: 28px 38px minmax(240px, 1fr) minmax(180px, .55fr) auto; align-items: center; gap: var(--space-3); min-height: 68px; padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .basket-item:hover { background: var(--bg-subtle-hover); }
  .position { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--radius-pill); color: var(--text-secondary); background: var(--bg-subtle); font-size: var(--text-caption); }
  .kind-icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  .item-copy { display: flex; flex-direction: column; min-width: 0; }
  .item-copy strong, .item-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item-copy strong { font-weight: 500; }
  .item-copy span, .item-copy small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .destination { display: flex; flex-direction: column; min-width: 0; }
  .destination span { color: var(--text-tertiary); font-size: var(--text-caption); }
  .destination strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 400; }
  .row-actions { display: flex; align-items: center; gap: 2px; }
  .form { display: flex; flex-direction: column; gap: var(--space-4); }
  .field { display: flex; flex-direction: column; gap: var(--space-1); }
  @media (max-width: 800px) { .basket-item { grid-template-columns: 26px 36px minmax(0, 1fr) auto; } .destination { display: none; } }
</style>
