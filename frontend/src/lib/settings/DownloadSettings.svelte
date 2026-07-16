<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import BandwidthScheduleEditor from "./BandwidthScheduleEditor.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import Surface from "../components/Surface.svelte";
  import TextField from "../components/TextField.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
</script>

<SettingsCategoryHeader title="Downloads" description="Choose the default destination and tune normal transfer behavior." />
<Surface class="form-surface">
  <PathPicker bind:value={controller.downloadDir} label="Default download folder" placeholder="Use the backend default" />
  <div class="grid">
    <TextField bind:value={controller.maxActive} label="Active downloads" inputmode="numeric" />
    <TextField bind:value={controller.speedLimitMbps} label="Global speed limit (Mbit/s)" inputmode="decimal" placeholder="0 for unlimited" />
  </div>
  <AdvancedDisclosure title="Transfer tuning" description="Advanced concurrency and retry controls.">
    <div class="grid advanced-grid">
      <TextField bind:value={controller.maxSegments} label="Maximum segments" inputmode="numeric" />
      <TextField bind:value={controller.segmentThresholdMib} label="Segment threshold (MiB)" inputmode="numeric" hint="Files below this size use a single connection." />
      <TextField bind:value={controller.maxConnections} label="Connections per host" inputmode="numeric" />
      <TextField bind:value={controller.maxRetries} label="Maximum retries" inputmode="numeric" />
      <TextField bind:value={controller.maxBatchUrls} label="Maximum URLs per batch" inputmode="numeric" />
    </div>
  </AdvancedDisclosure>
  <AdvancedDisclosure title="Scheduled bandwidth" description="Apply different speed limits by weekday and local time.">
    <BandwidthScheduleEditor {controller} />
  </AdvancedDisclosure>
</Surface>

<style>
  :global(.form-surface) { display: flex; flex-direction: column; gap: var(--space-5); overflow: visible; }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  .advanced-grid { padding-right: var(--space-4); }
  @media (max-width: 680px) { .grid { grid-template-columns: 1fr; } }
</style>
