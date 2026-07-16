<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import ComponentsView from "../components/ComponentsView.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import Surface from "../components/Surface.svelte";
  import TextField from "../components/TextField.svelte";
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
        <PathPicker mode="executable" bind:value={controller.imageConverterPath} label="ImageMagick executable" />
        <PathPicker bind:value={controller.cookieDir} label="Cookie working folder" placeholder="Use the managed private folder" />
      </div>
    </AdvancedDisclosure>
  </Surface>

  <Surface padding="none">
    <AdvancedDisclosure title="Media processing limits" description="Bound metadata probing and tune AVIF output quality.">
      <div class="limit-grid">
        <TextField bind:value={controller.mediaProbeTimeoutSecs} label="Media probe timeout (seconds)" inputmode="numeric" />
        <TextField bind:value={controller.mediaProbeMaxMib} label="Media probe output limit (MiB)" inputmode="numeric" />
        <TextField bind:value={controller.avifQuality} label="AVIF quality (1–100)" inputmode="numeric" />
      </div>
    </AdvancedDisclosure>
  </Surface>

  <Surface padding="none">
    <AdvancedDisclosure title="Archive safety limits" description="Reject extraction workloads that exceed bounded size, count, depth, or expansion ratio.">
      <div class="limit-grid">
        <TextField bind:value={controller.maxExtractMib} label="Maximum extracted size (MiB)" inputmode="numeric" />
        <TextField bind:value={controller.maxExtractFiles} label="Maximum extracted files" inputmode="numeric" />
        <TextField bind:value={controller.maxExtractDepth} label="Maximum directory depth" inputmode="numeric" />
        <TextField bind:value={controller.maxExtractRatio} label="Maximum expansion ratio" inputmode="numeric" />
      </div>
    </AdvancedDisclosure>
  </Surface>
</div>

<style>
  .stack { display: flex; flex-direction: column; gap: var(--space-4); }
  :global(.provision-row) { overflow: visible; }
  .path-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); padding: var(--space-2) var(--space-4) 0 0; }
  .limit-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); padding: var(--space-2) var(--space-4) 0 0; }
  @media (max-width: 760px) { .path-grid, .limit-grid { grid-template-columns: 1fr; } }
</style>
