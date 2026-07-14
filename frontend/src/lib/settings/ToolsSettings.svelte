<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import ComponentsView from "../components/ComponentsView.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import Surface from "../components/Surface.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
</script>

<SettingsCategoryHeader title="Tools" description="Install, update, verify, or repair the optional tools used by media, torrents, and archives." />
<div class="stack">
  <Surface class="provision-row">
    <ToggleSwitch bind:checked={controller.autoProvision} label="Maintain required tools automatically" description="Ravyn downloads verified managed versions when a feature needs them." />
  </Surface>

  <ComponentsView embedded />

  <Surface padding="none">
    <AdvancedDisclosure title="Executable overrides" description="Prefer managed tools unless a specific system executable is required.">
      <div class="path-grid">
        <PathPicker mode="executable" bind:value={controller.ytdlpPath} label="yt-dlp executable" />
        <PathPicker mode="executable" bind:value={controller.ffmpegPath} label="FFmpeg executable" />
        <PathPicker mode="executable" bind:value={controller.rqbitPath} label="rqbit executable" />
        <PathPicker mode="executable" bind:value={controller.sevenZipPath} label="7-Zip executable" />
      </div>
    </AdvancedDisclosure>
  </Surface>
</div>

<style>
  .stack { display: flex; flex-direction: column; gap: var(--space-4); }
  :global(.provision-row) { overflow: visible; }
  .path-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); padding: var(--space-2) var(--space-4) 0 0; }
  @media (max-width: 760px) { .path-grid { grid-template-columns: 1fr; } }
</style>
