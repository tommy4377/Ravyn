<script lang="ts">
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown from "../components/Dropdown.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import SecretValueField from "../components/SecretValueField.svelte";
  import TextField from "../components/TextField.svelte";
  import type { SecretType } from "../api/types";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();
  const secretTypeOptions = [
    { value: "api_token", label: "API token" },
    { value: "proxy_credentials", label: "Proxy credentials" },
    { value: "rqbit_credentials", label: "rqbit credentials" },
    { value: "cookies", label: "Cookies" },
    { value: "authentication_header", label: "Authorization header" },
    { value: "tls_certificate", label: "TLS certificate" },
    { value: "private_key", label: "Private key" },
  ];
  const credentialsMode = $derived(controller.secretType === "proxy_credentials" || controller.secretType === "rqbit_credentials");
  const secretReady = $derived(credentialsMode ? !!controller.secretUsername.trim() && !!controller.secretPassword : !!controller.secretValue);

  function setSecretType(value: string): void {
    controller.secretType = value as SecretType;
    controller.secretValue = "";
    controller.secretUsername = "";
    controller.secretPassword = "";
  }
</script>

<Dialog open={controller.presetOpen} title={controller.editingPreset ? "Edit download preset" : "New download preset"} onClose={() => !controller.presetBusy && (controller.presetOpen = false)} preventClose={controller.presetBusy}>
  <div class="dialog-form">
    <TextField bind:value={controller.presetName} label="Preset name" placeholder="Fast downloads" />
    <PathPicker bind:value={controller.presetDestination} label="Destination" placeholder="Use the default destination" />
    <TextField bind:value={controller.presetTemplate} label="Filename template" placeholder={"{host}/{year}/{stem}.{extension}"} hint={"Variables: {filename} {stem} {extension} {host} {year} {month} {day}"} oninput={() => controller.scheduleTemplatePreview()} />
    {#if controller.templatePreviewError}<p class="template-preview error">Invalid template: {controller.templatePreviewError}</p>{:else if controller.templatePreview}<p class="template-preview">Example: <code>{controller.templatePreview}</code>{#if controller.templatePreviewMissing.length}<span> · unknown variables: {controller.templatePreviewMissing.join(", ")}</span>{/if}</p>{/if}
    <div class="two-column"><TextField bind:value={controller.presetPriority} inputmode="numeric" label="Priority" /><TextField bind:value={controller.presetSpeed} inputmode="decimal" label="Speed limit (Mbit/s)" placeholder="0 for unlimited" /></div>
  </div>
  {#snippet footer()}<Button disabled={controller.presetBusy} onclick={() => (controller.presetOpen = false)}>Cancel</Button><Button variant="accent" disabled={controller.presetBusy || !controller.presetName.trim()} onclick={() => void controller.savePreset()}>{controller.presetBusy ? "Saving…" : controller.editingPreset ? "Save preset" : "Create preset"}</Button>{/snippet}
</Dialog>

<Dialog open={controller.profileOpen} title={controller.editingProfile ? "Edit settings profile" : "New settings profile"} onClose={() => !controller.profileBusy && (controller.profileOpen = false)} preventClose={controller.profileBusy}>
  <div class="dialog-form">
    <TextField bind:value={controller.profileName} label="Profile name" placeholder="Limited bandwidth" />
    <div class="two-column"><TextField bind:value={controller.profileMaxActive} inputmode="numeric" label="Active downloads" /><TextField bind:value={controller.profileSpeed} inputmode="decimal" label="Global speed limit (Mbit/s)" placeholder="0 for unlimited" /></div>
    <div class="dropdown-field"><label for="profile-preset">Default download preset</label><Dropdown id="profile-preset" options={controller.presetOptions} bind:value={controller.profilePresetId} label="Default download preset" /></div>
  </div>
  {#snippet footer()}<Button disabled={controller.profileBusy} onclick={() => (controller.profileOpen = false)}>Cancel</Button><Button variant="accent" disabled={controller.profileBusy || !controller.profileName.trim()} onclick={() => void controller.saveProfile()}>{controller.profileBusy ? "Saving…" : controller.editingProfile ? "Save profile" : "Create profile"}</Button>{/snippet}
</Dialog>

<Dialog open={controller.secretOpen} title={controller.secrets.some((item) => item.name === controller.secretName) ? "Replace secret value" : "Store a secret"} onClose={() => !controller.secretBusy && controller.resetSecretEditor()} preventClose={controller.secretBusy}>
  <div class="dialog-form">
    <TextField bind:value={controller.secretName} label="Reference name" placeholder="Work proxy" disabled={controller.secretBusy} hint="Reusing a name replaces its value without revealing the old one." />
    <div class="dropdown-field"><label for="secret-type">Secret type</label><Dropdown id="secret-type" options={secretTypeOptions} value={controller.secretType} onchange={setSecretType} label="Secret type" /></div>
    {#if credentialsMode}
      <div class="two-column"><TextField bind:value={controller.secretUsername} label="Username" /><TextField bind:value={controller.secretPassword} type="password" label="Password" /></div>
    {:else}
      <SecretValueField bind:value={controller.secretValue} label={controller.secretTypeLabel(controller.secretType)} disabled={controller.secretBusy} hint="The value is sent once to the local backend and stored through the operating-system credential manager." />
    {/if}
    {#if controller.secretError}<InlineError title="Couldn't store the secret" message={controller.secretError} />{/if}
  </div>
  {#snippet footer()}<Button disabled={controller.secretBusy} onclick={() => controller.resetSecretEditor()}>Cancel</Button><Button variant="accent" disabled={controller.secretBusy || !controller.secretName.trim() || !secretReady} onclick={() => void controller.saveSecret()}>{controller.secretBusy ? "Storing…" : "Store secret"}</Button>{/snippet}
</Dialog>

<ConfirmDialog open={!!controller.deleteTarget} title={`Delete ${controller.deleteTarget?.kind ?? "item"}?`} message={`${controller.deleteTarget?.name ?? "This item"} will be removed permanently.`} confirmLabel="Delete" destructive busy={controller.deleteBusy} error={controller.deleteError} onConfirm={() => void controller.confirmManagementDelete()} onClose={() => !controller.deleteBusy && (controller.deleteTarget = null)} />
<ConfirmDialog open={!!controller.secretDeleteTarget} title="Delete secret?" message={`${controller.secretDeleteTarget?.name ?? "This secret"} will be removed from the platform credential store. Downloads or integrations that reference it may stop working.`} confirmLabel="Delete secret" destructive busy={controller.secretDeleteBusy} error={controller.secretDeleteError} onConfirm={() => void controller.deleteSecret()} onClose={() => !controller.secretDeleteBusy && (controller.secretDeleteTarget = null)} />
<ConfirmDialog open={controller.resetOpen} title="Reset backend settings?" message="All persisted backend settings will return to their defaults. Local appearance preferences are not affected." confirmLabel="Reset settings" destructive busy={controller.resetBusy} error={controller.resetError} onConfirm={() => void controller.resetSettings()} onClose={() => !controller.resetBusy && (controller.resetOpen = false)} />

<style>
  .dialog-form { display: flex; flex-direction: column; gap: var(--space-4); }
  .two-column { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); }
  .dropdown-field { display: flex; flex-direction: column; gap: var(--space-1); }
  .dropdown-field :global(.dropdown), .dropdown-field :global(select) { width: 100%; }
  .template-preview { margin: calc(var(--space-2) * -1) 0 0; color: var(--text-secondary); font-size: var(--text-caption); }
  .template-preview.error { color: var(--status-error); }
  .template-preview code { color: var(--text-primary); }
  @media (max-width: 580px) { .two-column { grid-template-columns: 1fr; } }
</style>
