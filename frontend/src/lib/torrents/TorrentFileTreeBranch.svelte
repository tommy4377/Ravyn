<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { formatBytes } from "../util/format";
  import type { TorrentFileTreeNode } from "./torrentPresentation";
  import TorrentFileTreeBranch from "./TorrentFileTreeBranch.svelte";

  let {
    node,
    selectedFiles,
    onToggleFile,
    onToggleFolder,
  }: {
    node: TorrentFileTreeNode;
    selectedFiles: number[];
    onToggleFile: (index: number) => void;
    onToggleFolder: (indexes: number[], checked: boolean) => void;
  } = $props();

  const selectedCount = $derived(node.descendantFileIndexes.filter((index) => selectedFiles.includes(index)).length);
  const allSelected = $derived(node.descendantFileIndexes.length > 0 && selectedCount === node.descendantFileIndexes.length);
</script>

<details class="folder" open>
  <summary>
    <input
      type="checkbox"
      checked={allSelected}
      aria-label={`Select all files in ${node.name}`}
      onclick={(event) => event.stopPropagation()}
      onchange={() => onToggleFolder(node.descendantFileIndexes, !allSelected)}
    />
    <Icon name="folder-open" size={16} />
    <span><strong>{node.name}</strong><small>{selectedCount} of {node.descendantFileIndexes.length} selected · {formatBytes(node.descendantSizeBytes)}</small></span>
  </summary>
  <div class="children">
    {#each node.directories as directory (directory.path)}
      <TorrentFileTreeBranch node={directory} {selectedFiles} {onToggleFile} {onToggleFolder} />
    {/each}
    {#each node.files as file (file.index)}
      <label class="file-row">
        <input type="checkbox" checked={selectedFiles.includes(file.index)} onchange={() => onToggleFile(file.index)} />
        <span><strong>{file.path}</strong><small>{formatBytes(file.size_bytes)}</small></span>
      </label>
    {/each}
  </div>
</details>

<style>
  .folder { border-bottom: 1px solid var(--stroke-divider); }
  summary { min-height: 44px; display: grid; grid-template-columns: auto auto minmax(0, 1fr); align-items: center; gap: var(--space-2); list-style: none; cursor: default; }
  summary::-webkit-details-marker { display: none; }
  summary:hover { background: var(--bg-subtle-hover); }
  summary span, .file-row span { min-width: 0; display: flex; flex-direction: column; }
  summary strong, summary small, .file-row strong, .file-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  summary small, .file-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .children { padding-left: var(--space-5); border-left: 1px solid var(--stroke-divider); }
  .file-row { min-height: 40px; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: var(--space-2); border-top: 1px solid var(--stroke-divider); }
</style>
