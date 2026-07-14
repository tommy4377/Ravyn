<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Dropdown from "../components/Dropdown.svelte";
  import Surface from "../components/Surface.svelte";
  import TextField from "../components/TextField.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
</script>

<SettingsCategoryHeader title="Network" description="Tune connection limits, timeouts, retries, and the optional remote torrent engine." />
<Surface class="form-surface">
  <div class="grid">
    <TextField bind:value={controller.maxConnections} label="Connections per host" inputmode="numeric" />
    <TextField bind:value={controller.maxRetries} label="Maximum retries" inputmode="numeric" />
    <TextField bind:value={controller.connectTimeout} label="Connect timeout (seconds)" inputmode="numeric" />
    <TextField bind:value={controller.readTimeout} label="Read timeout (seconds)" inputmode="numeric" />
  </div>

  <AdvancedDisclosure title="Remote torrent engine" description="Use only when rqbit is hosted outside this Ravyn installation.">
    <div class="advanced-stack">
      <TextField bind:value={controller.rqbitApi} label="rqbit API address" placeholder="http://127.0.0.1:3030" />
      <div class="dropdown-field">
        <label for="rqbit-secret">Credentials</label>
        <Dropdown id="rqbit-secret" options={controller.rqbitCredentialOptions} bind:value={controller.rqbitCredentialsSecretId} label="rqbit credentials" />
        <small>Credentials are stored under Privacy and Secrets and are never shown again.</small>
      </div>
    </div>
  </AdvancedDisclosure>
</Surface>

<style>
  :global(.form-surface) { display: flex; flex-direction: column; gap: var(--space-5); overflow: visible; }
  .grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  .advanced-stack { display: grid; grid-template-columns: minmax(0, 1fr) minmax(220px, .7fr); gap: var(--space-4); padding-right: var(--space-4); }
  .dropdown-field { display: flex; flex-direction: column; gap: var(--space-1); }
  .dropdown-field :global(.dropdown), .dropdown-field :global(select) { width: 100%; }
  label { font-size: var(--text-body); }
  small { color: var(--text-secondary); font-size: var(--text-caption); }
  @media (max-width: 680px) { .grid, .advanced-stack { grid-template-columns: 1fr; } }
</style>
