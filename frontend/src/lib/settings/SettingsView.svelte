<script lang="ts">
  import { describeError } from "../api/errors";
  import { systemAppearance } from "../appearance/systemAppearance.svelte";
  import type { DownloadPreset, PersistentSettings, PersistentSettingsPatch, SecretReference, SecretType, UserProfile } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import Surface from "../components/Surface.svelte";
  import SecretValueField from "../components/SecretValueField.svelte";
  import TextField from "../components/TextField.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import { connection } from "../stores/connection.svelte";
  import { navigation, type Density, type MaterialPreference, type ThemePreference } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import {
    appUpdateStatus as readAppUpdateStatus,
    checkAppUpdate,
    repairApplication,
    type AppUpdateStatus,
  } from "../native/tauri";

  let values = $state<PersistentSettings | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let resetOpen = $state(false);
  let resetBusy = $state(false);
  let resetError = $state<string | null>(null);
  let presets = $state<DownloadPreset[]>([]);
  let profiles = $state<UserProfile[]>([]);
  let presetOpen = $state(false);
  let presetName = $state("");
  let presetDestination = $state("");
  let presetTemplate = $state("");
  let presetPriority = $state("0");
  let presetSpeed = $state("0");
  let presetBusy = $state(false);
  let profileOpen = $state(false);
  let profileName = $state("");
  let profileMaxActive = $state("3");
  let profileSpeed = $state("0");
  let profilePresetId = $state("");
  let profileBusy = $state(false);
  let deleteTarget = $state<{ kind: "preset" | "profile"; id: string; name: string } | null>(null);
  let deleteBusy = $state(false);
  let deleteError = $state<string | null>(null);
  let secrets = $state<SecretReference[]>([]);
  let secretOpen = $state(false);
  let secretName = $state("");
  let secretType = $state<SecretType>("api_token");
  let secretValue = $state("");
  let secretBusy = $state(false);
  let secretError = $state<string | null>(null);
  let secretDeleteTarget = $state<SecretReference | null>(null);
  let secretDeleteBusy = $state(false);
  let secretDeleteError = $state<string | null>(null);
  let updateStatus = $state<AppUpdateStatus | null>(null);
  let updateBusy = $state(false);
  let repairBusy = $state(false);

  let downloadDir = $state("");
  let libraryRoot = $state("");
  let autoOrganize = $state(true);
  let maxActive = $state("3");
  let maxSegments = $state("8");
  let maxConnections = $state("8");
  let speedLimitMbps = $state("0");
  let maxRetries = $state("4");
  let connectTimeout = $state("15");
  let readTimeout = $state("60");
  let autoProvision = $state(true);
  let ytdlpPath = $state("yt-dlp");
  let ffmpegPath = $state("ffmpeg");
  let rqbitPath = $state("rqbit");
  let rqbitApi = $state("http://127.0.0.1:3030");
  let rqbitCredentialsSecretId = $state("");
  let sevenZipPath = $state("7z");
  let backdropImageDraft = $state("");
  let intensityDraft = $state("76");

  const presetOptions = $derived<DropdownOption[]>([
    { value: "", label: "No default preset" },
    ...presets.map((preset) => ({ value: preset.id, label: preset.name })),
  ]);
  const rqbitCredentialOptions = $derived<DropdownOption[]>([
    { value: "", label: "No rqbit credentials" },
    ...secrets
      .filter((secret) => secret.secret_type === "rqbit_credentials")
      .map((secret) => ({ value: secret.id, label: secret.name })),
  ]);
  const secretTypeOptions: DropdownOption[] = [
    { value: "api_token", label: "API token" },
    { value: "proxy_credentials", label: "Proxy credentials" },
    { value: "rqbit_credentials", label: "rqbit credentials" },
    { value: "cookies", label: "Cookie JSON" },
    { value: "authentication_header", label: "Authorization header" },
    { value: "tls_certificate", label: "TLS certificate" },
    { value: "private_key", label: "Private key" },
  ];

  function sync(settings: PersistentSettings): void {
    values = settings;
    downloadDir = settings.download_dir ?? "";
    libraryRoot = settings.library_root ?? "";
    autoOrganize = settings.library_auto_organize;
    maxActive = String(settings.max_active);
    maxSegments = String(settings.max_segments);
    maxConnections = String(settings.max_connections_per_host);
    speedLimitMbps = String(Math.round(settings.global_speed_limit_bps / 125000 * 10) / 10);
    maxRetries = String(settings.max_retries);
    connectTimeout = String(settings.connect_timeout_secs);
    readTimeout = String(settings.read_timeout_secs);
    autoProvision = settings.auto_provision;
    ytdlpPath = settings.ytdlp;
    ffmpegPath = settings.ffmpeg;
    rqbitPath = settings.rqbit;
    rqbitApi = settings.rqbit_api;
    rqbitCredentialsSecretId = settings.rqbit_credentials_secret_id ?? "";
    sevenZipPath = settings.seven_zip;
    backdropImageDraft = navigation.backdropImage;
    intensityDraft = String(navigation.materialIntensity);
  }

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const [response, loadedPresets, loadedProfiles, loadedSecrets] = await Promise.all([
        connection.client.getSettings(),
        connection.client.listPresets(),
        connection.client.listProfiles(),
        connection.client.listSecrets({ limit: 100 }),
      ]);
      sync(response.values);
      presets = loadedPresets;
      profiles = loadedProfiles;
      secrets = loadedSecrets.items;
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  $effect(() => { void load(); });

  async function loadUpdateStatus(): Promise<void> {
    try {
      updateStatus = await readAppUpdateStatus();
    } catch {
      // Browser-only development does not expose native update commands.
      updateStatus = null;
    }
  }

  async function recheckApplicationUpdate(): Promise<void> {
    if (updateBusy) return;
    updateBusy = true;
    try {
      updateStatus = await checkAppUpdate();
      if (updateStatus.phase === "ready") {
        notifications.success(
          `Ravyn ${updateStatus.available_version ?? "update"} is ready`,
          "It will install silently when you close Ravyn.",
        );
      } else if (updateStatus.phase === "up_to_date") {
        notifications.info("Ravyn is up to date");
      }
    } catch (cause) {
      notifications.error("Couldn't check for an app update", describeError(cause));
      await loadUpdateStatus();
    } finally {
      updateBusy = false;
    }
  }

  async function repairInstalledApplication(): Promise<void> {
    if (updateBusy || repairBusy) return;
    repairBusy = true;
    try {
      updateStatus = await repairApplication();
      if (updateStatus.phase === "ready") {
        notifications.success(
          updateStatus.repair_mode ? "Repair package is ready" : `Ravyn ${updateStatus.available_version ?? "update"} is ready`,
          "The signed installer will run after you close Ravyn.",
        );
      }
    } catch (cause) {
      notifications.error("Couldn't prepare application repair", describeError(cause));
      await loadUpdateStatus();
    } finally {
      repairBusy = false;
    }
  }

  function updateHeading(status: AppUpdateStatus): string {
    if (status.repair_mode && status.phase === "downloading") return "Downloading repair package…";
    if (status.repair_mode && status.phase === "ready") return `Ravyn ${status.current_version} repair is ready`;
    if (status.repair_mode && status.phase === "installing") return "Repairing Ravyn…";
    switch (status.phase) {
      case "checking": return "Checking for updates…";
      case "downloading": return `Downloading Ravyn ${status.available_version ?? "update"}…`;
      case "ready": return `Ravyn ${status.available_version ?? "update"} is ready`;
      case "installing": return "Installing update…";
      case "up_to_date": return `Ravyn ${status.current_version} is up to date`;
      case "error": return "Update check failed";
      case "disabled": return "Application updates are unavailable";
      default: return `Ravyn ${status.current_version}`;
    }
  }

  function updateResultHeading(status: AppUpdateStatus): string {
    const result = status.last_result;
    if (!result) return "";
    if (result.outcome === "succeeded" && result.from_version === result.to_version) return `Repaired Ravyn ${result.to_version}`;
    if (result.outcome === "succeeded") return `Updated to Ravyn ${result.to_version}`;
    if (result.outcome === "rolled_back") return `Ravyn ${result.to_version} was rolled back`;
    return `Ravyn ${result.to_version} could not be installed`;
  }

  function updateResultDescription(status: AppUpdateStatus): string {
    const result = status.last_result;
    if (!result) return "";
    const completed = new Date(result.completed_at_unix_ms);
    const timestamp = Number.isNaN(completed.getTime()) ? "" : ` · ${completed.toLocaleString()}`;
    return `${result.message}${timestamp}`;
  }

  function updateDescription(status: AppUpdateStatus): string {
    if (status.phase === "ready") {
      return status.repair_mode
        ? "The signed installer for the current version has been verified and will reinstall Ravyn after a normal close."
        : "The signed installer has been verified and will run silently after you close Ravyn.";
    }
    if (status.phase === "downloading") {
      const total = status.total_bytes ?? 0;
      const percent = total > 0 ? Math.min(100, Math.round(status.downloaded_bytes / total * 100)) : 0;
      return total > 0 ? `${percent}% downloaded` : "Downloading and verifying the signed installer.";
    }
    if (status.last_error) return status.last_error;
    if (!status.automatic) return "Automatic updates require an installed Windows build.";
    return "Ravyn checks in the background and installs downloaded updates only when the app closes.";
  }

  $effect(() => {
    void loadUpdateStatus();
    const timer = window.setInterval(() => void loadUpdateStatus(), 2000);
    return () => window.clearInterval(timer);
  });

  function positive(value: string, fallback: number): number {
    const parsed = Number(value);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
  }

  function buildPatch(): PersistentSettingsPatch {
    return {
      download_dir: downloadDir.trim() || null,
      library_root: libraryRoot.trim() || null,
      library_auto_organize: autoOrganize,
      max_active: Math.max(1, Math.round(positive(maxActive, values?.max_active ?? 3))),
      max_segments: Math.max(1, Math.round(positive(maxSegments, values?.max_segments ?? 8))),
      max_connections_per_host: Math.max(1, Math.round(positive(maxConnections, values?.max_connections_per_host ?? 8))),
      global_speed_limit_bps: Math.round(positive(speedLimitMbps, 0) * 125000),
      max_retries: Math.round(positive(maxRetries, values?.max_retries ?? 4)),
      connect_timeout_secs: Math.max(1, Math.round(positive(connectTimeout, values?.connect_timeout_secs ?? 15))),
      read_timeout_secs: Math.max(1, Math.round(positive(readTimeout, values?.read_timeout_secs ?? 60))),
      ytdlp: ytdlpPath.trim() || "yt-dlp",
      ffmpeg: ffmpegPath.trim() || "ffmpeg",
      rqbit: rqbitPath.trim() || "rqbit",
      rqbit_api: rqbitApi.trim() || "http://127.0.0.1:3030",
      rqbit_credentials_secret_id: rqbitCredentialsSecretId || null,
      seven_zip: sevenZipPath.trim() || "7z",
      auto_provision: autoProvision,
    };
  }

  async function save(): Promise<void> {
    if (!connection.client || !values) return;
    saving = true;
    try {
      const patch = buildPatch();
      const validation = await connection.client.validateSettings(patch);
      if (!validation.valid) {
        notifications.error("Some settings are invalid", validation.issues.map((issue) => `${issue.field}: ${issue.message}`).join("\n"));
        return;
      }
      const response = await connection.client.patchSettings(patch);
      sync(response.values);
      navigation.setBackdropImage(backdropImageDraft);
      navigation.setMaterialIntensity(Number(intensityDraft) || 76);
      notifications.success("Settings saved", response.restart_required || validation.restart_required ? "Restart the backend to apply every change." : "Changes are active now.");
    } catch (cause) {
      notifications.error("Couldn't save settings", describeError(cause));
    } finally {
      saving = false;
    }
  }

  async function resetSettings(): Promise<void> {
    if (!connection.client) return;
    resetBusy = true;
    resetError = null;
    try {
      const response = await connection.client.resetSettings();
      sync(response.values);
      resetOpen = false;
      notifications.info("Backend settings reset", "A backend restart is required.");
    } catch (cause) {
      resetError = describeError(cause);
    } finally {
      resetBusy = false;
    }
  }


  async function createPreset(): Promise<void> {
    if (!connection.client || !presetName.trim() || presetBusy) return;
    presetBusy = true;
    try {
      const created = await connection.client.createPreset({
        name: presetName.trim(),
        payload: {
          destination: presetDestination.trim() || null,
          filename_template: presetTemplate.trim() || null,
          priority: Math.round(Number(presetPriority) || 0),
          speed_limit_bps: Math.round(Math.max(0, Number(presetSpeed) || 0) * 125000),
        },
      });
      presets = [...presets, created].sort((a, b) => a.name.localeCompare(b.name));
      presetOpen = false;
      presetName = "";
      notifications.success("Preset created");
    } catch (cause) {
      notifications.error("Couldn't create the preset", describeError(cause));
    } finally {
      presetBusy = false;
    }
  }

  async function createProfile(): Promise<void> {
    if (!connection.client || !profileName.trim() || profileBusy) return;
    profileBusy = true;
    try {
      const created = await connection.client.createProfile({
        name: profileName.trim(),
        default_preset_id: profilePresetId || null,
        settings_patch: {
          max_active: Math.max(1, Math.round(Number(profileMaxActive) || 3)),
          global_speed_limit_bps: Math.round(Math.max(0, Number(profileSpeed) || 0) * 125000),
        },
      });
      profiles = [...profiles, created].sort((a, b) => Number(b.active) - Number(a.active) || a.name.localeCompare(b.name));
      profileOpen = false;
      profileName = "";
      notifications.success("Profile created");
    } catch (cause) {
      notifications.error("Couldn't create the profile", describeError(cause));
    } finally {
      profileBusy = false;
    }
  }

  async function activateProfile(profile: UserProfile): Promise<void> {
    if (!connection.client || profileBusy) return;
    profileBusy = true;
    try {
      const response = await connection.client.activateProfile(profile.id);
      profiles = profiles.map((item) => ({ ...item, active: item.id === response.profile.id }));
      const settings = await connection.client.getSettings();
      sync(settings.values);
      notifications.success("Profile activated", response.restart_required ? "Restart the backend to apply every profile setting." : "Profile settings are active now.");
    } catch (cause) {
      notifications.error("Couldn't activate the profile", describeError(cause));
    } finally {
      profileBusy = false;
    }
  }

  async function confirmManagementDelete(): Promise<void> {
    if (!connection.client || !deleteTarget || deleteBusy) return;
    deleteBusy = true;
    deleteError = null;
    try {
      if (deleteTarget.kind === "preset") {
        await connection.client.deletePreset(deleteTarget.id);
        presets = presets.filter((item) => item.id !== deleteTarget!.id);
      } else {
        await connection.client.deleteProfile(deleteTarget.id);
        profiles = profiles.filter((item) => item.id !== deleteTarget!.id);
      }
      notifications.info(`${deleteTarget.kind === "preset" ? "Preset" : "Profile"} deleted`);
      deleteTarget = null;
    } catch (cause) {
      deleteError = describeError(cause);
    } finally {
      deleteBusy = false;
    }
  }

  function secretTypeLabel(type: SecretType): string {
    return secretTypeOptions.find((option) => option.value === type)?.label ?? type;
  }

  function resetSecretEditor(): void {
    secretOpen = false;
    secretName = "";
    secretType = "api_token";
    secretValue = "";
    secretError = null;
  }

  async function saveSecret(): Promise<void> {
    if (!connection.client || secretBusy || !secretName.trim() || !secretValue) return;
    secretBusy = true;
    secretError = null;
    try {
      const reference = await connection.client.putSecret({
        name: secretName.trim(),
        secret_type: secretType,
        secret: secretValue,
      });
      secrets = [reference, ...secrets.filter((item) => item.id !== reference.id && item.name !== reference.name)]
        .sort((left, right) => left.name.localeCompare(right.name));
      resetSecretEditor();
      notifications.success("Secret stored", "The value was written to the platform credential store.");
    } catch (cause) {
      secretError = describeError(cause);
    } finally {
      secretBusy = false;
    }
  }

  function editSecret(reference: SecretReference): void {
    secretName = reference.name;
    secretType = reference.secret_type;
    secretValue = "";
    secretError = null;
    secretOpen = true;
  }

  async function deleteSecret(): Promise<void> {
    if (!connection.client || !secretDeleteTarget || secretDeleteBusy) return;
    secretDeleteBusy = true;
    secretDeleteError = null;
    try {
      await connection.client.deleteSecret(secretDeleteTarget.id);
      const deletedId = secretDeleteTarget.id;
      secrets = secrets.filter((item) => item.id !== deletedId);
      if (rqbitCredentialsSecretId === deletedId) rqbitCredentialsSecretId = "";
      secretDeleteTarget = null;
      notifications.info("Secret removed");
    } catch (cause) {
      secretDeleteError = describeError(cause);
    } finally {
      secretDeleteBusy = false;
    }
  }

  function chooseTheme(theme: ThemePreference): void { navigation.setTheme(theme); }
  function chooseDensity(density: Density): void { navigation.setDensity(density); }
  function chooseMaterial(material: MaterialPreference): void { navigation.setMaterial(material); }
</script>

<div class="page">
  <PageHeader title="Settings" description="Appearance changes are immediate. Download engine changes are validated by the backend before they are saved.">
    {#snippet actions()}
      <Button onclick={() => (resetOpen = true)}>Reset backend settings</Button>
      <Button variant="accent" disabled={saving || !values} onclick={() => void save()}><Icon name="save" size={16} /> {saving ? "Saving…" : "Save changes"}</Button>
    {/snippet}
  </PageHeader>

  <div class="content">
    {#if error}
      <InlineError title="Couldn't load settings" message={error} retry={() => void load()} />
    {:else if loading || !values}
      <Surface><p class="muted">Loading settings…</p></Surface>
    {:else}
      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="palette" size={20} /></span><div><h2>Appearance</h2><p>Use a consistent synthetic Fluent material on both Windows 10 and Windows 11.</p></div></div>
        <Surface padding="none" class="settings-card">
          <div class="setting-row"><div><strong>App theme</strong><span>System follows the current Windows light or dark preference.</span></div><div class="choice-group" aria-label="App theme"><button class:active={navigation.theme === "system"} onclick={() => chooseTheme("system")}>System</button><button class:active={navigation.theme === "light"} onclick={() => chooseTheme("light")}><Icon name="sun" size={14} /> Light</button><button class:active={navigation.theme === "dark"} onclick={() => chooseTheme("dark")}><Icon name="moon" size={14} /> Dark</button></div></div>
          <div class="setting-row"><div><strong>Window material</strong><span>Synthetic material is always rendered and does not depend on native Mica or Acrylic support.</span></div><div class="choice-group"><button class:active={navigation.material === "synthetic"} onclick={() => chooseMaterial("synthetic")}>Synthetic</button><button class:active={navigation.material === "solid"} onclick={() => chooseMaterial("solid")}>Solid</button></div></div>
          <div class="setting-row"><div><strong>Windows desktop backdrop</strong><span>{systemAppearance.wallpaperAvailable ? `Using the current Windows wallpaper (${systemAppearance.wallpaperPosition}). It stays aligned while Ravyn moves.` : systemAppearance.supported ? "Windows did not expose a usable wallpaper, so Ravyn is using its generated backdrop." : "Available automatically in the installed Windows application."}</span>{#if systemAppearance.lastError}<small class="setting-warning">{systemAppearance.lastError}</small>{/if}</div><Button disabled={systemAppearance.refreshing} onclick={() => void systemAppearance.refresh()}>{systemAppearance.refreshing ? "Refreshing…" : "Refresh"}</Button></div>
          <div class="setting-row align-start"><div><strong>Material intensity</strong><span>Controls the wallpaper, tint, glow, and texture strength.</span></div><div class="range-control"><input type="range" min="0" max="100" bind:value={intensityDraft} oninput={() => navigation.setMaterialIntensity(Number(intensityDraft))} /><output>{intensityDraft}%</output></div></div>
          <div class="setting-row align-start"><div><strong>Custom backdrop override</strong><span>Optional image URL used instead of the Windows wallpaper. Leave empty to follow the desktop automatically.</span></div><div class="inline-field"><input type="text" bind:value={backdropImageDraft} placeholder="https://… or asset URI" /><Button onclick={() => navigation.setBackdropImage(backdropImageDraft)}>Apply</Button></div></div>
          <div class="setting-row"><div><strong>Content density</strong><span>Compact fits more rows; comfortable provides larger targets.</span></div><div class="choice-group"><button class:active={navigation.density === "comfortable"} onclick={() => chooseDensity("comfortable")}>Comfortable</button><button class:active={navigation.density === "compact"} onclick={() => chooseDensity("compact")}><Icon name="compact" size={14} /> Compact</button></div></div>
        </Surface>
      </section>

      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="folder-open" size={20} /></span><div><h2>Downloads and library</h2><p>Choose where files are stored and how completed downloads enter the library.</p></div></div>
        <Surface padding="normal" class="form-card">
          <div class="two-column"><PathPicker bind:value={downloadDir} label="Default download folder" placeholder="Use the backend default" /><PathPicker bind:value={libraryRoot} label="Library root" placeholder="Use the configured library" /></div>
          <ToggleSwitch bind:checked={autoOrganize} label="Organize completed downloads automatically" description="Move completed items into category folders under the library root." />
          <ToggleSwitch bind:checked={autoProvision} label="Install required components automatically" description="Allow Ravyn to provision enabled tools when a feature needs them." />
        </Surface>
      </section>

      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="wrench" size={20} /></span><div><h2>Components and tools</h2><p>Override managed or system executables without changing environment variables.</p></div></div>
        <Surface padding="normal" class="form-card">
          <div class="two-column">
            <PathPicker mode="executable" bind:value={ytdlpPath} label="yt-dlp executable" placeholder="yt-dlp" hint="Use a command name for PATH resolution or select a specific executable." />
            <PathPicker mode="executable" bind:value={ffmpegPath} label="FFmpeg executable" placeholder="ffmpeg" hint="Used for media probing, merging, and conversion." />
            <PathPicker mode="executable" bind:value={rqbitPath} label="rqbit executable" placeholder="rqbit" hint="Used when Ravyn provisions or launches the local torrent engine." />
            <PathPicker mode="executable" bind:value={sevenZipPath} label="7-Zip executable" placeholder="7z" hint="Ravyn 0.2 uses an existing system or custom 7z/7za executable." />
          </div>
          <div class="two-column">
            <TextField bind:value={rqbitApi} label="rqbit API URL" placeholder="http://127.0.0.1:3030" hint="Keep this on loopback unless the endpoint is intentionally remote." />
            <label class="select-field">
              <span>rqbit credentials</span>
              <Dropdown bind:value={rqbitCredentialsSecretId} options={rqbitCredentialOptions} label="rqbit credentials" />
              <small>Select a stored rqbit credentials secret. The value is never returned to this page.</small>
            </label>
          </div>
          <p class="form-note"><Icon name="info" size={15} /> Executable and API changes are validated now and take effect after the backend restarts.</p>
        </Surface>
      </section>

      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="shield" size={20} /></span><div><h2>Credentials and secrets</h2><p>Store tokens, cookies, certificates, and credentials in the operating-system keyring.</p></div></div>
        <Surface padding="none" class="management-card">
          <div class="management-heading">
            <div><strong>Secret references</strong><span>Ravyn stores only metadata in its database. Existing values are never returned to the frontend.</span></div>
            <Button onclick={() => { secretError = null; secretOpen = true; }}><Icon name="add" size={15} /> New secret</Button>
          </div>
          {#if secrets.length === 0}
            <p class="management-empty">No secrets stored.</p>
          {:else}
            {#each secrets as secret (secret.id)}
              <div class="management-row">
                <span class="management-icon"><Icon name="shield" size={17} /></span>
                <div>
                  <strong>{secret.name}</strong>
                  <span>{secretTypeLabel(secret.secret_type)} · Updated {new Date(secret.updated_at).toLocaleString()}</span>
                </div>
                <Button variant="subtle" onclick={() => editSecret(secret)}><Icon name="edit" size={14} /> Replace value</Button>
                <IconButton icon="trash" label={`Delete ${secret.name}`} variant="subtle" onclick={() => { secretDeleteError = null; secretDeleteTarget = secret; }} />
              </div>
            {/each}
          {/if}
        </Surface>
      </section>

      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="speed" size={20} /></span><div><h2>Performance</h2><p>Control concurrency, segmented downloads, and the global transfer limit.</p></div></div>
        <Surface padding="normal" class="form-card">
          <div class="field-grid"><TextField bind:value={maxActive} label="Active downloads" /><TextField bind:value={maxSegments} label="Maximum segments" /><TextField bind:value={maxConnections} label="Connections per host" /><TextField bind:value={speedLimitMbps} label="Global speed limit (Mbit/s)" placeholder="0 for unlimited" /></div>
        </Surface>
      </section>

      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="cloud" size={20} /></span><div><h2>Network and recovery</h2><p>Timeouts and retry behavior for unstable connections.</p></div></div>
        <Surface padding="normal" class="form-card">
          <div class="field-grid three"><TextField bind:value={maxRetries} label="Maximum retries" /><TextField bind:value={connectTimeout} label="Connect timeout (seconds)" /><TextField bind:value={readTimeout} label="Read timeout (seconds)" /></div>
        </Surface>
      </section>

      {#if updateStatus}
        <section class="settings-section">
          <div class="section-copy"><span class="section-icon"><Icon name="download" size={20} /></span><div><h2>Application updates</h2><p>Signed releases download in the background and install after a normal close.</p></div></div>
          <Surface padding="none" class="settings-card">
            <div class="setting-row update-row">
              <div>
                <strong>{updateHeading(updateStatus)}</strong>
                <span>{updateDescription(updateStatus)}</span>
                {#if updateStatus.notes && updateStatus.phase === "ready"}<span class="update-notes">{updateStatus.notes}</span>{/if}
                {#if updateStatus.phase === "downloading"}
                  <div class="update-progress" aria-label="Application update download progress">
                    <span style={`width: ${updateStatus.total_bytes ? Math.min(100, updateStatus.downloaded_bytes / updateStatus.total_bytes * 100) : 8}%`}></span>
                  </div>
                {/if}
              </div>
              <div class="update-actions">
                <Button
                  disabled={updateBusy || repairBusy || updateStatus.phase === "checking" || updateStatus.phase === "downloading" || updateStatus.phase === "installing" || !updateStatus.configured || !updateStatus.automatic}
                  onclick={() => void recheckApplicationUpdate()}
                >
                  <Icon name={updateBusy || (updateStatus.phase === "checking" && !repairBusy) ? "spinner" : "refresh"} size={15} />
                  {updateBusy ? "Checking…" : "Check now"}
                </Button>
                <Button
                  variant="subtle"
                  disabled={updateBusy || repairBusy || updateStatus.phase === "checking" || updateStatus.phase === "downloading" || updateStatus.phase === "installing" || !updateStatus.configured || !updateStatus.automatic}
                  onclick={() => void repairInstalledApplication()}
                >
                  <Icon name={repairBusy ? "spinner" : "restore"} size={15} />
                  {repairBusy ? "Preparing…" : "Repair"}
                </Button>
              </div>
            </div>
            {#if updateStatus.last_result}
              <div class="setting-row update-result" class:warning={updateStatus.last_result.outcome !== "succeeded"}>
                <span class="result-icon"><Icon name={updateStatus.last_result.outcome === "succeeded" ? "check-circle" : "warning"} size={16} /></span>
                <div>
                  <strong>{updateResultHeading(updateStatus)}</strong>
                  <span>{updateResultDescription(updateStatus)}</span>
                </div>
              </div>
            {/if}
          </Surface>
        </section>
      {/if}

      <section class="settings-section">
        <div class="section-copy"><span class="section-icon"><Icon name="tag" size={20} /></span><div><h2>Presets and profiles</h2><p>Reuse download destinations and switch between performance configurations.</p></div></div>
        <div class="management-stack">
          <Surface padding="none" class="management-card">
            <div class="management-heading"><div><strong>Download presets</strong><span>Reusable destination, naming, priority and speed settings.</span></div><Button onclick={() => (presetOpen = true)}><Icon name="add" size={15} /> New preset</Button></div>
            {#if presets.length === 0}<p class="management-empty">No presets created.</p>{:else}{#each presets as preset (preset.id)}<div class="management-row"><span class="management-icon"><Icon name="download" size={17} /></span><div><strong>{preset.name}</strong><span>{preset.payload.destination ?? "Default destination"}{preset.payload.speed_limit_bps ? ` · ${Math.round(preset.payload.speed_limit_bps / 125000 * 10) / 10} Mbit/s` : " · unlimited"}</span></div><IconButton icon="trash" label="Delete preset" variant="subtle" onclick={() => { deleteError = null; deleteTarget = { kind: "preset", id: preset.id, name: preset.name }; }} /></div>{/each}{/if}
          </Surface>
          <Surface padding="none" class="management-card">
            <div class="management-heading"><div><strong>Settings profiles</strong><span>Switch download concurrency, bandwidth, and default preset together.</span></div><Button onclick={() => (profileOpen = true)}><Icon name="add" size={15} /> New profile</Button></div>
            {#if profiles.length === 0}<p class="management-empty">No profiles created.</p>{:else}{#each profiles as profile (profile.id)}<div class="management-row"><span class="management-icon"><Icon name="settings" size={17} /></span><div><strong>{profile.name}{profile.active ? " · Active" : ""}</strong><span>{profile.settings_patch.max_active ?? "Default"} active downloads{profile.default_preset_id ? ` · ${presets.find((preset) => preset.id === profile.default_preset_id)?.name ?? "Preset"}` : ""}</span></div>{#if !profile.active}<Button variant="subtle" disabled={profileBusy} onclick={() => void activateProfile(profile)}>Activate</Button>{/if}<IconButton icon="trash" label="Delete profile" variant="subtle" disabled={profile.active} onclick={() => { deleteError = null; deleteTarget = { kind: "profile", id: profile.id, name: profile.name }; }} /></div>{/each}{/if}
          </Surface>
        </div>
      </section>
    {/if}
  </div>
</div>

<Dialog open={presetOpen} title="New download preset" onClose={() => !presetBusy && (presetOpen = false)} preventClose={presetBusy}>
  <div class="dialog-form"><TextField bind:value={presetName} label="Preset name" placeholder="Fast downloads" /><PathPicker bind:value={presetDestination} label="Destination" placeholder="Use the default destination" /><TextField bind:value={presetTemplate} label="Filename template" placeholder="Leave empty to keep the original name" /><div class="two-column"><TextField bind:value={presetPriority} inputmode="numeric" label="Priority" /><TextField bind:value={presetSpeed} inputmode="decimal" label="Speed limit (Mbit/s)" placeholder="0 for unlimited" /></div></div>
  {#snippet footer()}<Button disabled={presetBusy} onclick={() => (presetOpen = false)}>Cancel</Button><Button variant="accent" disabled={presetBusy || !presetName.trim()} onclick={() => void createPreset()}>{presetBusy ? "Creating…" : "Create preset"}</Button>{/snippet}
</Dialog>

<Dialog open={profileOpen} title="New settings profile" onClose={() => !profileBusy && (profileOpen = false)} preventClose={profileBusy}>
  <div class="dialog-form"><TextField bind:value={profileName} label="Profile name" placeholder="Limited bandwidth" /><div class="two-column"><TextField bind:value={profileMaxActive} inputmode="numeric" label="Active downloads" /><TextField bind:value={profileSpeed} inputmode="decimal" label="Global speed limit (Mbit/s)" placeholder="0 for unlimited" /></div><div class="dropdown-field"><span>Default download preset</span><Dropdown options={presetOptions} bind:value={profilePresetId} label="Default download preset" /></div></div>
  {#snippet footer()}<Button disabled={profileBusy} onclick={() => (profileOpen = false)}>Cancel</Button><Button variant="accent" disabled={profileBusy || !profileName.trim()} onclick={() => void createProfile()}>{profileBusy ? "Creating…" : "Create profile"}</Button>{/snippet}
</Dialog>

<Dialog open={secretOpen} title={secrets.some((item) => item.name === secretName) ? "Replace secret value" : "Store a secret"} onClose={() => !secretBusy && resetSecretEditor()} preventClose={secretBusy}>
  <div class="dialog-form">
    <TextField bind:value={secretName} label="Reference name" placeholder="Work proxy" disabled={secretBusy} hint="Names are unique. Reusing a name replaces its value without exposing the old one." />
    <div class="dropdown-field">
      <span>Secret type</span>
      <Dropdown options={secretTypeOptions} bind:value={secretType} label="Secret type" />
    </div>
    <SecretValueField bind:value={secretValue} label="Secret value" disabled={secretBusy} hint="The value is sent once to the local backend and stored through the operating-system credential manager." />
    {#if secretError}<InlineError title="Couldn't store the secret" message={secretError} />{/if}
  </div>
  {#snippet footer()}
    <Button disabled={secretBusy} onclick={resetSecretEditor}>Cancel</Button>
    <Button variant="accent" disabled={secretBusy || !secretName.trim() || !secretValue} onclick={() => void saveSecret()}>{secretBusy ? "Storing…" : "Store secret"}</Button>
  {/snippet}
</Dialog>

<ConfirmDialog open={!!deleteTarget} title={`Delete ${deleteTarget?.kind ?? "item"}?`} message={`${deleteTarget?.name ?? "This item"} will be removed permanently.`} confirmLabel="Delete" destructive busy={deleteBusy} error={deleteError} onConfirm={() => void confirmManagementDelete()} onClose={() => !deleteBusy && (deleteTarget = null)} />

<ConfirmDialog open={!!secretDeleteTarget} title="Delete secret?" message={`${secretDeleteTarget?.name ?? "This secret"} will be removed from the platform credential store. Downloads or integrations that reference it may stop working.`} confirmLabel="Delete secret" destructive busy={secretDeleteBusy} error={secretDeleteError} onConfirm={() => void deleteSecret()} onClose={() => !secretDeleteBusy && (secretDeleteTarget = null)} />

<ConfirmDialog open={resetOpen} title="Reset backend settings?" message="All persisted backend settings will return to their defaults. Local appearance preferences are not affected." confirmLabel="Reset settings" destructive busy={resetBusy} error={resetError} onConfirm={() => void resetSettings()} onClose={() => !resetBusy && (resetOpen = false)} />

<style>
  .page { height: 100%; display: flex; flex-direction: column; }
  .content { flex: 1; min-height: 0; overflow: auto; padding: 0 var(--page-padding) var(--page-padding); display: flex; flex-direction: column; gap: var(--space-7); }
  .muted { color: var(--text-secondary); }
  .settings-section { display: grid; grid-template-columns: minmax(210px, 280px) minmax(0, 1fr); gap: var(--space-6); align-items: start; }
  .section-copy { display: flex; gap: var(--space-3); position: sticky; top: 0; }
  .section-icon { display: grid; place-items: center; width: 38px; height: 38px; flex: none; border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  h2 { margin: 0; font-size: var(--text-body-strong); }
  .section-copy p { margin: var(--space-1) 0 0; color: var(--text-secondary); font-size: var(--text-caption); }
  :global(.settings-card) { overflow: visible; }
  .setting-row { display: flex; align-items: center; justify-content: space-between; gap: var(--space-6); min-height: 72px; padding: var(--space-3) var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .setting-row:last-child { border-bottom: 0; }
  .setting-row > div:first-child { display: flex; flex-direction: column; max-width: 560px; }
  .setting-row strong { font-weight: 500; }
  .setting-row span { color: var(--text-secondary); font-size: var(--text-caption); }
  .update-row > div:first-child { flex: 1; }
  .update-actions { display: flex; align-items: center; gap: var(--space-2); flex: none; }
  .update-result { justify-content: flex-start; min-height: 58px; }
  .update-result > div { max-width: none; }
  .result-icon { display: grid; place-items: center; width: 28px; height: 28px; flex: none; border-radius: 50%; color: var(--success-text); background: var(--success-subtle); }
  .update-result.warning .result-icon { color: var(--warning-text); background: var(--warning-subtle); }
  .update-notes { margin-top: var(--space-2); white-space: pre-line; }
  .update-progress { width: min(440px, 100%); height: 3px; margin-top: var(--space-3); overflow: hidden; border-radius: 99px; background: var(--stroke-divider); }
  .update-progress span { display: block; height: 100%; min-width: 8px; border-radius: inherit; background: var(--accent-default); transition: width 180ms linear; }
  .align-start { align-items: flex-start; }
  .choice-group { display: inline-flex; padding: 2px; border: 1px solid var(--stroke-control); border-radius: var(--radius-medium); background: var(--bg-subtle); flex: none; }
  .choice-group button { display: flex; align-items: center; gap: 5px; height: 30px; padding: 0 var(--space-3); border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; cursor: default; }
  .choice-group button.active { color: var(--text-primary); background: var(--surface-card-hover); box-shadow: 0 1px 2px rgba(0,0,0,.08); }
  .range-control { display: flex; align-items: center; gap: var(--space-3); min-width: 240px; }
  .range-control input { flex: 1; accent-color: var(--accent-default); }
  .range-control output { width: 40px; color: var(--text-secondary); text-align: right; }
  .inline-field { display: flex; gap: var(--space-2); width: min(460px, 50%); }
  .inline-field input { flex: 1; min-width: 0; height: var(--control-default); padding: 0 var(--space-3); border: 1px solid var(--stroke-control); border-bottom-color: var(--stroke-control-strong); border-radius: var(--radius-control); color: var(--text-primary); background: var(--bg-control); }
  :global(.form-card) { display: flex; flex-direction: column; gap: var(--space-5); overflow: visible; }
  .two-column { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-4); }
  .field-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--space-4); }
  .field-grid.three { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .management-stack { display: flex; flex-direction: column; gap: var(--space-4); }
  .management-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); min-height: 70px; padding: var(--space-3) var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .management-heading > div, .management-row > div { display: flex; min-width: 0; flex-direction: column; }
  .management-heading span, .management-row span, .management-empty { color: var(--text-secondary); font-size: var(--text-caption); }
  .management-row { display: grid; grid-template-columns: 36px minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); min-height: 60px; padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .management-row:last-child { border-bottom: 0; }
  .management-icon { display: grid; place-items: center; width: 32px; height: 32px; border-radius: var(--radius-medium); color: var(--accent-text) !important; background: var(--accent-subtle); }
  .management-row strong, .management-row span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .management-empty { margin: 0; padding: var(--space-5); }
  .dialog-form { display: flex; flex-direction: column; gap: var(--space-4); }
  .dropdown-field { display: flex; flex-direction: column; gap: var(--space-1); }
  .select-field {
    display: grid;
    gap: var(--space-2);
    align-content: start;
  }
  .select-field > span {
    color: var(--text-primary);
    font-size: var(--text-body);
    font-weight: 600;
  }
  .select-field :global(.dropdown),
  .select-field :global(select) {
    width: 100%;
  }
  .select-field small {
    color: var(--text-tertiary);
    font-size: var(--text-caption);
    line-height: 1.45;
  }
  .form-note { display: flex; align-items: flex-start; gap: var(--space-2); margin: 0; padding-top: var(--space-1); color: var(--text-secondary); font-size: var(--text-caption); line-height: 1.45; }
  .form-note :global(.icon) { margin-top: 1px; color: var(--accent-text); }
  @media (max-width: 1000px) { .settings-section { grid-template-columns: 1fr; gap: var(--space-3); } .section-copy { position: static; } .field-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  @media (max-width: 700px) { .setting-row { align-items: stretch; flex-direction: column; gap: var(--space-3); } .choice-group, .range-control, .inline-field, .update-actions { width: 100%; } .update-actions :global(button) { flex: 1; } .two-column, .field-grid, .field-grid.three { grid-template-columns: 1fr; } }
  .setting-warning { display: block; margin-top: var(--space-1); color: var(--status-warning); }
</style>
