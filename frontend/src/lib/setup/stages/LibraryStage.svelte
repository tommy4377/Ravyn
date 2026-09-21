<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import Button from "../../components/Button.svelte";
  import TextField from "../../components/TextField.svelte";
  import { formatBytes } from "../componentStates";
  import { pickFolder } from "../../native/tauri";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  async function browse() {
    const chosen = await pickFolder(controller.libraryPath || undefined);
    if (chosen) {
      controller.libraryPath = chosen.endsWith("\\Ravyn")
        ? chosen
        : `${chosen}\\Ravyn`;
      controller.libraryError = null;
    }
  }

  async function next() {
    if (await controller.prepareLibrary()) {
      controller.step = "preferences";
    }
  }
</script>

<StageShell
  title="Choose your Ravyn library"
  subtitle="Downloads are organized into folders inside this location."
  busy={controller.busy}
  onback={() => (controller.step = "features")}
  onnext={next}
  nextDisabled={!controller.libraryPath.trim()}
>
  <div class="picker">
    <div class="field">
      <TextField
        label="Library location"
        bind:value={controller.libraryPath}
        error={controller.libraryError ?? ""}
        oninput={() => (controller.libraryError = null)}
      />
    </div>
    <div class="browse">
      <Button onclick={() => void browse()}>Browse…</Button>
    </div>
  </div>

  {#if controller.availableBytes !== null}
    <p class="space">
      Available space: {formatBytes(controller.availableBytes)}
    </p>
  {/if}

  <div class="structure">
    <p class="structure-title">Ravyn will create this structure:</p>
    <ul>
      <li>Downloads, Videos, Music, Documents, Images</li>
      <li>Archives, Torrents, Playlists</li>
      <li>Temporary and Trash (managed by Ravyn)</li>
    </ul>
  </div>
</StageShell>

<style>
  .picker {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
  }
  .field {
    flex: 1;
  }
  .browse {
    padding-top: 22px;
  }
  .space {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--text-secondary);
  }
  .structure {
    padding: var(--space-4);
    border: 1px solid var(--stroke-divider);
    border-radius: var(--radius-layer);
    background: var(--bg-layer);
  }
  .structure-title {
    margin: 0 0 var(--space-2);
    font-weight: 600;
  }
  ul {
    margin: 0;
    padding-left: var(--space-5);
    color: var(--text-secondary);
  }
</style>
