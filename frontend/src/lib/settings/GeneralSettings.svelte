<script lang="ts">
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import Surface from "../components/Surface.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
</script>

<SettingsCategoryHeader title="General" description="Manage reusable download presets, working profiles, and tags." />

<div class="stack">
  <Surface padding="none">
    <div class="heading"><div><strong>Download presets</strong><span>Reuse a destination, naming pattern, priority, and speed limit.</span></div><Button onclick={() => controller.openPresetEditor(null)}><Icon name="add" size={15} /> New preset</Button></div>
    {#if controller.presets.length === 0}
      <p class="empty">No presets created.</p>
    {:else}
      {#each controller.presets as preset (preset.id)}
        <div class="row">
          <span class="row-icon"><Icon name="download" size={17} /></span>
          <div><strong>{preset.name}</strong><span>{preset.payload.destination ?? "Default destination"}{preset.payload.speed_limit_bps ? ` · ${Math.round(preset.payload.speed_limit_bps / 125000 * 10) / 10} Mbit/s` : " · unlimited"}</span></div>
          <MenuButton label={`Actions for ${preset.name}`} icon="more" iconOnly variant="subtle" items={[
            { id: "edit", label: "Edit", icon: "edit", onSelect: () => controller.openPresetEditor(preset) },
            { id: "delete", label: "Delete", icon: "trash", danger: true, separatorBefore: true, onSelect: () => { controller.deleteError = null; controller.deleteTarget = { kind: "preset", id: preset.id, name: preset.name }; } },
          ]} />
        </div>
      {/each}
    {/if}
  </Surface>

  <Surface padding="none">
    <div class="heading"><div><strong>Settings profiles</strong><span>Switch concurrency, bandwidth, and the default preset together.</span></div><Button onclick={() => controller.openProfileEditor(null)}><Icon name="add" size={15} /> New profile</Button></div>
    {#if controller.profiles.length === 0}
      <p class="empty">No profiles created.</p>
    {:else}
      {#each controller.profiles as profile (profile.id)}
        <div class="row">
          <span class="row-icon"><Icon name="settings" size={17} /></span>
          <div><strong>{profile.name}{profile.active ? " · Active" : ""}</strong><span>{profile.settings_patch.max_active ?? "Default"} active downloads{profile.default_preset_id ? ` · ${controller.presets.find((preset) => preset.id === profile.default_preset_id)?.name ?? "Preset"}` : ""}</span></div>
          {#if !profile.active}<Button variant="subtle" disabled={controller.profileBusy} onclick={() => void controller.activateProfile(profile)}>Activate</Button>{/if}
          <MenuButton label={`Actions for ${profile.name}`} icon="more" iconOnly variant="subtle" items={[
            { id: "edit", label: "Edit", icon: "edit", onSelect: () => controller.openProfileEditor(profile) },
            { id: "delete", label: "Delete", icon: "trash", danger: true, disabled: profile.active, separatorBefore: true, onSelect: () => { controller.deleteError = null; controller.deleteTarget = { kind: "profile", id: profile.id, name: profile.name }; } },
          ]} />
        </div>
      {/each}
    {/if}
  </Surface>

  <Surface padding="none">
    <div class="heading"><div><strong>Tags</strong><span>Labels used by downloads and automation rules.</span></div></div>
    {#if controller.tags.length === 0}
      <p class="empty">No tags in use.</p>
    {:else}
      {#each controller.tags as tag (tag.id)}
        <div class="row">
          <span class="row-icon"><Icon name="tag" size={17} /></span>
          <div><strong>{tag.name}</strong><span>{tag.job_count} download{tag.job_count === 1 ? "" : "s"}</span></div>
          <MenuButton label={`Actions for ${tag.name}`} icon="more" iconOnly variant="subtle" items={[
            { id: "delete", label: "Delete unused tag", icon: "trash", danger: true, disabled: controller.tagDeleteBusy !== null, onSelect: () => void controller.deleteTag(tag) },
          ]} />
        </div>
      {/each}
    {/if}
  </Surface>
</div>

<style>
  .stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .heading { min-height: 68px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .heading > div, .row > div { min-width: 0; display: flex; flex-direction: column; }
  .heading span, .row span, .empty { color: var(--text-secondary); font-size: var(--text-caption); }
  .row { min-height: 60px; display: grid; grid-template-columns: 34px minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .row:last-child { border-bottom: 0; }
  .row-icon { width: 32px; height: 32px; display: grid; place-items: center; color: var(--text-secondary); }
  .row strong, .row span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { margin: 0; padding: var(--space-5); }
  @media (max-width: 650px) { .row { grid-template-columns: 34px minmax(0, 1fr) auto; } .row > :global(button:not(.menu-trigger)) { display: none; } }
</style>
