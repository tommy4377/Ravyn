<script lang="ts">
  import { onMount } from "svelte";
  import AppearanceSettings from "./AppearanceSettings.svelte";
  import AboutSettings from "./AboutSettings.svelte";
  import DownloadSettings from "./DownloadSettings.svelte";
  import GeneralSettings from "./GeneralSettings.svelte";
  import NetworkSettings from "./NetworkSettings.svelte";
  import SecretSettings from "./SecretSettings.svelte";
  import SettingsDialogs from "./SettingsDialogs.svelte";
  import SettingsNavigation from "./SettingsNavigation.svelte";
  import StorageSettings from "./StorageSettings.svelte";
  import ToolsSettings from "./ToolsSettings.svelte";
  import TroubleshootingSettings from "./TroubleshootingSettings.svelte";
  import UpdateSettings from "./UpdateSettings.svelte";
  import { SettingsController, type SettingsCategory } from "./settingsController.svelte";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageScaffold from "../components/PageScaffold.svelte";
  import Surface from "../components/Surface.svelte";
  import { navigation } from "../stores/navigation.svelte";

  const controller = new SettingsController();
  let pendingCategory = $state<SettingsCategory | null>(null);

  $effect(() => {
    navigation.settingsDirty = controller.isDirty;
    return () => { navigation.settingsDirty = false; };
  });

  onMount(() => {
    void controller.load();
    void controller.loadNativeInfo();
    const stopPolling = controller.startUpdatePolling();
    return () => stopPolling();
  });

  function selectCategory(category: SettingsCategory): void {
    if (category === controller.category) return;
    if (controller.isDirty) {
      pendingCategory = category;
      return;
    }
    controller.category = category;
  }

  function discardAndSwitch(): void {
    if (!pendingCategory) return;
    controller.discardChanges();
    controller.category = pendingCategory;
    pendingCategory = null;
  }
</script>

<PageScaffold title="Settings" summary="Appearance changes are immediate. Engine settings are validated before saving.">
  {#snippet actions()}
    <Button variant="subtle" disabled={controller.loading || controller.resetBusy} onclick={() => { controller.resetError = null; controller.resetOpen = true; }}><Icon name="restore" size={15} /> Reset backend settings</Button>
  {/snippet}

  {#if controller.error}
    <div class="load-state"><InlineError title="Settings are unavailable" message={controller.error} retry={() => void controller.load()} /></div>
  {:else if controller.loading || !controller.values}
    <div class="load-state"><Surface><p>Loading settings…</p></Surface></div>
  {:else}
    <div class="settings-layout">
      <SettingsNavigation selected={controller.category} onSelect={selectCategory} />
      <div class="settings-content">
        <div class="category-content">
          {#if controller.category === "general"}<GeneralSettings {controller} />
          {:else if controller.category === "downloads"}<DownloadSettings {controller} />
          {:else if controller.category === "storage"}<StorageSettings {controller} />
          {:else if controller.category === "appearance"}<AppearanceSettings />
          {:else if controller.category === "tools"}<ToolsSettings {controller} />
          {:else if controller.category === "network"}<NetworkSettings {controller} />
          {:else if controller.category === "updates"}<UpdateSettings {controller} />
          {:else if controller.category === "privacy"}<SecretSettings {controller} />
          {:else if controller.category === "troubleshooting"}<TroubleshootingSettings {controller} />
          {:else}<AboutSettings {controller} />{/if}
        </div>

        {#if controller.isDirty || controller.restartRequired}
          <div class="save-bar" role="status">
            <div>
              <strong>{controller.isDirty ? "Unsaved changes" : "Restart required"}</strong>
              <span>{controller.isDirty ? "Save or discard backend settings before leaving this category." : "Some saved changes take effect after the backend restarts."}</span>
            </div>
            <div class="save-actions">
              {#if controller.isDirty}<Button disabled={controller.saving} onclick={() => controller.discardChanges()}>Discard</Button><Button variant="accent" disabled={controller.saving} onclick={() => void controller.save()}><Icon name="save" size={15} /> {controller.saving ? "Saving…" : "Save changes"}</Button>{/if}
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</PageScaffold>

<SettingsDialogs {controller} />
<ConfirmDialog open={!!pendingCategory} title="Discard unsaved changes?" message="Changing category now will restore the last saved backend settings. Appearance preferences are unaffected." confirmLabel="Discard and continue" destructive onConfirm={discardAndSwitch} onClose={() => (pendingCategory = null)} />

<style>
  .load-state { padding: var(--page-padding); }
  .load-state p { margin: 0; color: var(--text-secondary); }
  .settings-layout { height: 100%; display: grid; grid-template-columns: minmax(220px, 270px) minmax(0, 1fr); }
  .settings-content { position: relative; min-width: 0; min-height: 0; overflow: auto; }
  .category-content { max-width: 1120px; padding: var(--space-5) var(--page-padding) 104px; }
  .save-bar { position: sticky; z-index: 10; left: 0; right: 0; bottom: 0; min-height: 72px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) var(--page-padding); border-top: 1px solid var(--stroke-surface); background: var(--surface-overlay); box-shadow: 0 -8px 24px color-mix(in srgb, #000 10%, transparent); backdrop-filter: blur(24px) saturate(120%); }
  .save-bar > div:first-child { display: flex; flex-direction: column; min-width: 0; }
  .save-bar span { color: var(--text-secondary); font-size: var(--text-caption); }
  .save-actions { display: flex; gap: var(--space-2); flex: none; }
  @media (max-width: 900px) { .settings-layout { grid-template-columns: 1fr; grid-template-rows: auto minmax(0, 1fr); } .category-content { padding-top: var(--space-4); } }
  @media (max-width: 620px) { .save-bar { align-items: stretch; flex-direction: column; } .save-actions { width: 100%; justify-content: flex-end; } }
</style>
