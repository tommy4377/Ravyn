<script lang="ts">
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import Surface from "../components/Surface.svelte";
  import { formatAbsoluteTime } from "../util/format";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
</script>

<SettingsCategoryHeader title="Privacy and Secrets" description="Store credentials in the operating-system credential manager without exposing their values in Ravyn." />
<Surface padding="none">
  <div class="heading">
    <div><strong>Stored references</strong><span>Values are write-only. Editing a reference replaces the stored value.</span></div>
    <Button variant="accent" onclick={() => controller.openSecretEditor()}><Icon name="add" size={15} /> Store secret</Button>
  </div>
  {#if controller.secrets.length === 0}
    <p class="empty">No secrets stored.</p>
  {:else}
    {#each controller.secrets as secret (secret.id)}
      <div class="row">
        <span class="row-icon"><Icon name="shield" size={17} /></span>
        <div><strong>{secret.name}</strong><span>{controller.secretTypeLabel(secret.secret_type)} · updated {formatAbsoluteTime(secret.updated_at)}</span></div>
        <MenuButton label={`Actions for ${secret.name}`} icon="more" iconOnly variant="subtle" items={[
          { id: "replace", label: "Replace value", icon: "edit", onSelect: () => controller.openSecretEditor(secret) },
          { id: "delete", label: "Delete", icon: "trash", danger: true, separatorBefore: true, onSelect: () => { controller.secretDeleteError = null; controller.secretDeleteTarget = secret; } },
        ]} />
      </div>
    {/each}
  {/if}
</Surface>

<style>
  .heading { min-height: 72px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .heading > div, .row > div { min-width: 0; display: flex; flex-direction: column; }
  .heading span, .row span, .empty { color: var(--text-secondary); font-size: var(--text-caption); }
  .row { min-height: 60px; display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .row:last-child { border-bottom: 0; }
  .row-icon { width: 32px; height: 32px; display: grid; place-items: center; color: var(--text-secondary); }
  .row strong, .row span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { margin: 0; padding: var(--space-5); }
</style>
