<script lang="ts">
  import Button from "../components/Button.svelte";
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Icon from "../components/Icon.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import Surface from "../components/Surface.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import { formatBytes } from "../util/format";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import LibraryMoveDialog from "./LibraryMoveDialog.svelte";
  import CategoryOverridesEditor from "./CategoryOverridesEditor.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
  let moveOpen = $state(false);

  function moveActivated(destination: string): void {
    controller.libraryRoot = destination;
    controller.restartRequired = true;
    void controller.load();
  }
</script>

<SettingsCategoryHeader title="Storage and Library" description="Control the library root, organization, retention, and cleanup." />
<div class="stack">
  <Surface class="form-surface">
    <PathPicker bind:value={controller.libraryRoot} label="Library root" placeholder="Use the configured library" />
    <ToggleSwitch bind:checked={controller.autoOrganize} label="Organize completed downloads automatically" description="Move completed items into category folders under the library root." />
    <div class="move-row">
      <div>
        <strong>Move the existing Library</strong>
        <span>Copy, checksum, activate, and recover the entire tracked Library as one durable operation.</span>
      </div>
      <Button onclick={() => (moveOpen = true)}><Icon name="folder-open" size={15} /> Move Library</Button>
    </div>
    <AdvancedDisclosure title="Category routing" description="Override automatic classification for selected file extensions.">
      <CategoryOverridesEditor {controller} />
    </AdvancedDisclosure>
  </Surface>

  {#if controller.cleanupPolicies}
    <Surface padding="none">
      <div class="heading"><div><strong>Cleanup policy</strong><span>Retention applies only to eligible temporary data and trashed entries.</span></div><Button disabled={controller.cleanupBusy} onclick={() => void controller.saveCleanupPolicies()}><Icon name="save" size={15} /> Save policy</Button></div>
      <div class="policy-grid">
        <label><span>Temporary files (days)</span><input type="number" min="0" bind:value={controller.cleanupPolicies.temporary_max_age_days} /></label>
        <label><span>Trash retention (days)</span><input type="number" min="0" bind:value={controller.cleanupPolicies.trash_retention_days} /></label>
        <label><span>Log retention (days)</span><input type="number" min="0" bind:value={controller.cleanupPolicies.log_retention_days} /></label>
        <label><span>Cache retention (days)</span><input type="number" min="0" bind:value={controller.cleanupPolicies.cache_retention_days} /></label>
      </div>
      <div class="cleanup-row">
        <div><strong>Run cleanup now</strong><span>Uses the saved policy and never removes active downloads.</span></div>
        <Button variant="subtle" disabled={controller.cleanupBusy} onclick={() => void controller.runCleanup()}><Icon name="wrench" size={15} /> {controller.cleanupBusy ? "Working…" : "Run cleanup"}</Button>
      </div>
      {#if controller.cleanupReport}
        <div class="report"><Icon name="check-circle" size={16} /><span>{controller.cleanupReport.temporary_files_removed} temporary files and {controller.cleanupReport.cache_files_removed} cache files removed · {formatBytes(controller.cleanupReport.temporary_bytes_removed + controller.cleanupReport.cache_bytes_removed)} freed · {controller.cleanupReport.trash_entries_purged} trash entries purged</span></div>
      {/if}
    </Surface>
  {/if}
</div>

<LibraryMoveDialog
  open={moveOpen}
  currentRoot={controller.libraryRoot}
  onClose={() => (moveOpen = false)}
  onActivated={moveActivated}
/>

<style>
  .stack { display: flex; flex-direction: column; gap: var(--space-4); }
  :global(.form-surface) { display: flex; flex-direction: column; gap: var(--space-5); overflow: visible; }
  .heading, .cleanup-row, .move-row { min-height: 68px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .heading > div, .cleanup-row > div, .move-row > div { display: flex; flex-direction: column; }
  .heading span, .cleanup-row span, .move-row span { color: var(--text-secondary); font-size: var(--text-caption); }
  .move-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding-top: var(--space-4); border-top: 1px solid var(--stroke-divider); }
  .policy-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-4); padding: var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .policy-grid label { display: flex; flex-direction: column; gap: var(--space-1); color: var(--text-primary); font-size: var(--text-body); }
  .policy-grid input { min-width: 0; height: var(--control-default); padding: 0 var(--space-3); border: 1px solid var(--stroke-control); border-bottom-color: var(--stroke-control-strong); border-radius: var(--radius-control); color: var(--text-primary); background: var(--bg-control); font: inherit; }
  .policy-grid input:focus { border-bottom: 2px solid var(--accent-default); outline: none; }
  .report { display: flex; align-items: flex-start; gap: var(--space-2); padding: var(--space-3) var(--space-4); color: var(--text-secondary); font-size: var(--text-caption); }
  @media (max-width: 900px) { .policy-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 600px) { .policy-grid { grid-template-columns: 1fr; } .heading, .cleanup-row, .move-row { align-items: stretch; flex-direction: column; } }
</style>
