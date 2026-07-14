<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import { formatBytes } from "../util/format";
  import {
    canCancelAppUpdate,
    canInstallAppUpdateNow,
    formatAppUpdateTime,
  } from "./appUpdatePresentation";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
  const progress = $derived(controller.updateStatus?.total_bytes
    ? Math.min(100, controller.updateStatus.downloaded_bytes / controller.updateStatus.total_bytes * 100)
    : 0);
  const severity = $derived(
    controller.updateStatus?.phase === "error"
      ? "error"
      : controller.updateStatus?.phase === "ready"
        ? "success"
        : controller.updateStatus?.phase === "downloading"
            || controller.updateStatus?.phase === "checking"
            || controller.updateStatus?.phase === "cancelling"
          ? "info"
          : "neutral",
  );
  const spinning = $derived(
    controller.updateStatus?.phase === "checking"
      || controller.updateStatus?.phase === "downloading"
      || controller.updateStatus?.phase === "cancelling",
  );
</script>

<SettingsCategoryHeader title="Updates" description="Ravyn checks periodically, downloads verified updates in the background, and installs them after a normal close." />
<Surface padding="none">
  {#if controller.updateStatus}
    <div class="status-row">
      <span class="status-icon">
        <Icon
          name={controller.updateStatus.phase === "ready"
            ? "check-circle"
            : controller.updateStatus.phase === "error"
              ? "alert-circle"
              : controller.updateStatus.phase === "cancelled"
                ? "cancel"
                : spinning
                  ? "spinner"
                  : "refresh"}
          size={21}
        />
      </span>
      <div class="status-copy">
        <div class="title-line">
          <strong>{controller.updateHeading(controller.updateStatus)}</strong>
          <StatusBadge label={controller.updateStatus.phase.replaceAll("_", " ")} {severity} {spinning} />
        </div>
        <span>{controller.updateDescription(controller.updateStatus)}</span>
        {#if controller.updateStatus.phase === "downloading"}
          <ProgressBar value={progress} label="Update download progress" />
        {/if}
      </div>
      <div class="actions">
        {#if canInstallAppUpdateNow(controller.updateStatus)}
          <Button
            variant="accent"
            disabled={controller.updateInstallBusy || controller.updateCancelBusy}
            onclick={() => void controller.installApplicationUpdateNow()}
          >
            <Icon name="refresh" size={15} /> {controller.updateInstallBusy ? "Restarting…" : "Restart and install"}
          </Button>
        {/if}
        {#if canCancelAppUpdate(controller.updateStatus)}
          <Button
            variant="subtle"
            disabled={controller.updateCancelBusy || controller.updateInstallBusy || controller.updateStatus.phase === "cancelling"}
            onclick={() => void controller.cancelApplicationUpdate()}
          >
            <Icon name="cancel" size={15} />
            {controller.updateCancelBusy
              ? "Stopping…"
              : controller.updateStatus.phase === "ready"
                ? "Discard update"
                : "Cancel"}
          </Button>
        {/if}
        {#if !canInstallAppUpdateNow(controller.updateStatus)}
          <Button
            disabled={controller.updateBusy
              || controller.updateStatus.phase === "downloading"
              || controller.updateStatus.phase === "checking"
              || controller.updateStatus.phase === "cancelling"
              || controller.updateStatus.phase === "installing"}
            onclick={() => void controller.recheckApplicationUpdate()}
          >
            <Icon name="refresh" size={15} /> {controller.updateBusy ? "Checking…" : "Check now"}
          </Button>
          <Button
            variant="subtle"
            disabled={controller.repairBusy
              || controller.updateStatus.phase === "downloading"
              || controller.updateStatus.phase === "checking"
              || controller.updateStatus.phase === "cancelling"
              || controller.updateStatus.phase === "installing"}
            onclick={() => void controller.repairInstalledApplication()}
          >
            <Icon name="wrench" size={15} /> {controller.repairBusy ? "Preparing…" : "Repair"}
          </Button>
        {/if}
      </div>
    </div>

    <AdvancedDisclosure title="Update details" description="Schedule, signed package information, and release notes.">
      <dl class="details-grid">
        <div><dt>Current version</dt><dd>{controller.updateStatus.current_version}</dd></div>
        <div><dt>Available version</dt><dd>{controller.updateStatus.available_version ?? "None"}</dd></div>
        <div><dt>Last checked</dt><dd>{formatAppUpdateTime(controller.updateStatus.last_checked_at_unix_ms)}</dd></div>
        <div><dt>Next automatic check</dt><dd>{controller.updateStatus.automatic ? formatAppUpdateTime(controller.updateStatus.next_check_at_unix_ms) : "Disabled"}</dd></div>
        <div><dt>Staged package</dt><dd>{controller.updateStatus.total_bytes ? formatBytes(controller.updateStatus.total_bytes) : "None"}</dd></div>
        <div><dt>Install behavior</dt><dd>{controller.updateStatus.install_on_exit ? "Install after normal close" : "No pending installation"}</dd></div>
      </dl>
      {#if controller.updateStatus.notes}
        <div class="notes"><strong>Release notes</strong><span>{controller.updateStatus.notes}</span></div>
      {/if}
    </AdvancedDisclosure>

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
  .status-copy > span, .result span, .notes span { color: var(--text-secondary); font-size: var(--text-caption); }
  .title-line { display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap; }
  .actions { display: flex; justify-content: flex-end; gap: var(--space-2); flex-wrap: wrap; }
  .result, .notes { display: flex; flex-direction: column; gap: var(--space-1); padding-right: var(--space-4); }
  .details-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-3) var(--space-6); margin: 0 0 var(--space-4); }
  .details-grid > div { min-width: 0; padding-bottom: var(--space-2); border-bottom: 1px solid var(--stroke-divider); }
  dt { color: var(--text-secondary); font-size: var(--text-caption); }
  dd { margin: var(--space-1) 0 0; overflow-wrap: anywhere; }
  @media (max-width: 720px) {
    .status-row { align-items: stretch; flex-direction: column; }
    .status-icon { display: none; }
    .actions { justify-content: flex-start; }
    .details-grid { grid-template-columns: 1fr; }
  }
</style>
