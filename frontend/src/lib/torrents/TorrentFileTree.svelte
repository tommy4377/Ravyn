<script lang="ts">
  import type { TorrentFileTreeNode } from "./torrentPresentation";
  import TorrentFileTreeBranch from "./TorrentFileTreeBranch.svelte";

  let {
    root,
    selectedFiles,
    onToggleFile,
    onToggleFolder,
  }: {
    root: TorrentFileTreeNode;
    selectedFiles: number[];
    onToggleFile: (index: number) => void;
    onToggleFolder: (indexes: number[], checked: boolean) => void;
  } = $props();
</script>

<div class="file-tree">
  {#each root.directories as directory (directory.path)}
    <TorrentFileTreeBranch node={directory} {selectedFiles} {onToggleFile} {onToggleFolder} />
  {/each}
  {#each root.files as file (file.index)}
    <label class="file-row root-file">
      <input type="checkbox" checked={selectedFiles.includes(file.index)} onchange={() => onToggleFile(file.index)} />
      <span><strong>{file.path}</strong><small>{file.size_bytes === null ? "Unknown size" : `${file.size_bytes.toLocaleString()} bytes`}</small></span>
    </label>
  {/each}
</div>

<style>
  .file-tree { display: flex; flex-direction: column; }
  .file-row { min-height: 42px; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: var(--space-2); padding: var(--space-1) 0; border-bottom: 1px solid var(--stroke-divider); }
  .file-row span { min-width: 0; display: flex; flex-direction: column; }
  .file-row strong, .file-row small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
</style>
