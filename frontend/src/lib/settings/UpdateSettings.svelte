<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
  const progress = $derived(controller.updateStatus?.total_bytes
    ? Math.min(100, controller.updateStatus.downloaded_bytes / controller.updateStatus.total_bytes * 100)
    : 0);
  const severity = $derived(controller.updateStatus?.phase === "error" ? "error" : controller.updateStatus?.phase === "ready" ? "success" : controller.updateStatus?.phase === "downloading" || controller.updateStatus?.phase === "checking" ? "info" : "neutral");
</script>

<SettingsCategoryHeader title="Updates" description="Ravyn checks quietly, downloads verified updates in the background, and installs them after a normal close." />
<Surface padding="none">
  {#if controller.updateStatus}
    <div class="status-row">
      <span class="status-icon"><Icon name={controller.updateStatus.phase === "ready" ? "check-circle" : controller.updateStatus.phase === "error" ? "alert-circle" : "refresh"} size={21} /></span>
      <div class="status-copy">
        <div class="title-line"><strong>{controller.updateHeading(controller.updateStatus)}</strong><StatusBadge label={controller.updateStatus.phase.replaceAll("_", " ")} {severity} spinning={controller.updateStatus.phase === "checking" || controller.updateStatus.phase === "downloading"} /></div>
        <span>{controller.updateDescription(controller.updateStatus)}</span>
        {#if controller.updateStatus.phase === "downloading"}<ProgressBar value={progress} label="Update download progress" />{/if}
      </div>
      <div class="actions">
        <Button disabled={controller.updateBusy || controller.updateStatus.phase === "downloading" || controller.updateStatus.phase === "installing"} onclick={() => void controller.recheckApplicationUpdate()}><Icon name="refresh" size={15} /> {controller.updateBusy ? "Checking…" : "Check now"}</Button>
        <Button variant="subtle" disabled={controller.repairBusy || controller.updateStatus.phase === "downloading" || controller.updateStatus.phase === "installing"} onclick={() => void controller.repairInstalledApplication()}><Icon name="wrench" size={15} /> {controller.repairBusy ? "Preparing…" : "Repair"}</Button>
      </div>
    </div>
    {#if controller.updateStatus.last_result}
      <AdvancedDisclosure title="Last update result" description={controller.updateResultHeading(controller.updateStatus)}>
        <div class="result"><strong>{controller.updateResultHeading(controller.updateStatus)}</strong><span>{controller.updateResultDescription(controller.updateStatus)}</span></div>
      </AdvancedDisclosure>
    {/if}
  {:else}
    <div class="status-row"><span class="status-icon"><Icon name="info" size={21} /></span><div class="status-copy"><strong>Update status unavailable</strong><span>Application updates are available only in the native Windows build.</span></div></div>
  {/if}
</Surface>

<style>
  .status-row { min-height: 112px; display: flex; align-items: center; gap: var(--space-4); padding: var(--space-5); }
  .status-icon { width: 42px; height: 42px; flex: none; display: grid; place-items: center; color: var(--accent-text); }
  .status-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: var(--space-2); }
  .status-copy > span, .result span { color: var(--text-secondary); font-size: var(--text-caption); }
  .title-line { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
  .actions { display: flex; gap: var(--space-2); flex-wrap: wrap; }
  .result { display: flex; flex-direction: column; gap: var(--space-1); padding-right: var(--space-4); }
  @media (max-width: 720px) { .status-row { align-items: stretch; flex-direction: column; } .status-icon { display: none; } }
</style>
