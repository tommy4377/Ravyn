import { describeError } from "../api/errors";
import type {
  CleanupPolicies,
  CleanupReport,
  DownloadPreset,
  PersistentSettings,
  PersistentSettingsPatch,
  SecretReference,
  SecretType,
  TagRecord,
  UserProfile,
} from "../api/types";
import {
  appUpdateStatus as readAppUpdateStatus,
  backendInfo,
  checkAppUpdate,
  repairApplication,
  setupInstallationInfo,
  type AppUpdateStatus,
  type BackendInfo,
  type InstallationInfo,
} from "../native/tauri";
import { connection } from "../stores/connection.svelte";
import { navigation } from "../stores/navigation.svelte";
import { notifications } from "../stores/notifications.svelte";

export type SettingsCategory =
  | "general"
  | "downloads"
  | "storage"
  | "appearance"
  | "tools"
  | "network"
  | "updates"
  | "privacy"
  | "troubleshooting"
  | "about";

export interface ManagementDeleteTarget {
  kind: "preset" | "profile";
  id: string;
  name: string;
}

export class SettingsController {
  category = $state<SettingsCategory>("general");
  values = $state<PersistentSettings | null>(null);
  loading = $state(true);
  error = $state<string | null>(null);
  saving = $state(false);
  restartRequired = $state(false);
  baselinePatch = $state("");

  resetOpen = $state(false);
  resetBusy = $state(false);
  resetError = $state<string | null>(null);

  presets = $state<DownloadPreset[]>([]);
  profiles = $state<UserProfile[]>([]);
  tags = $state<TagRecord[]>([]);
  secrets = $state<SecretReference[]>([]);

  presetOpen = $state(false);
  editingPreset = $state<DownloadPreset | null>(null);
  presetName = $state("");
  presetDestination = $state("");
  presetTemplate = $state("");
  presetPriority = $state("0");
  presetSpeed = $state("0");
  presetBusy = $state(false);
  templatePreview = $state<string | null>(null);
  templatePreviewMissing = $state<string[]>([]);
  templatePreviewError = $state<string | null>(null);
  templatePreviewTimer: ReturnType<typeof setTimeout> | null = null;

  profileOpen = $state(false);
  editingProfile = $state<UserProfile | null>(null);
  profileName = $state("");
  profileMaxActive = $state("3");
  profileSpeed = $state("0");
  profilePresetId = $state("");
  profileBusy = $state(false);

  deleteTarget = $state<ManagementDeleteTarget | null>(null);
  deleteBusy = $state(false);
  deleteError = $state<string | null>(null);
  tagDeleteBusy = $state<number | null>(null);

  secretOpen = $state(false);
  secretName = $state("");
  secretType = $state<SecretType>("api_token");
  secretValue = $state("");
  secretUsername = $state("");
  secretPassword = $state("");
  secretBusy = $state(false);
  secretError = $state<string | null>(null);
  secretDeleteTarget = $state<SecretReference | null>(null);
  secretDeleteBusy = $state(false);
  secretDeleteError = $state<string | null>(null);

  updateStatus = $state<AppUpdateStatus | null>(null);
  updateBusy = $state(false);
  repairBusy = $state(false);

  cleanupPolicies = $state<CleanupPolicies | null>(null);
  cleanupBusy = $state(false);
  cleanupReport = $state<CleanupReport | null>(null);

  backend = $state<BackendInfo | null>(null);
  installation = $state<InstallationInfo | null>(null);

  downloadDir = $state("");
  libraryRoot = $state("");
  autoOrganize = $state(true);
  maxActive = $state("3");
  maxSegments = $state("8");
  maxConnections = $state("8");
  speedLimitMbps = $state("0");
  maxRetries = $state("4");
  connectTimeout = $state("15");
  readTimeout = $state("60");
  autoProvision = $state(true);
  ytdlpPath = $state("yt-dlp");
  ffmpegPath = $state("ffmpeg");
  rqbitPath = $state("rqbit");
  rqbitApi = $state("http://127.0.0.1:3030");
  rqbitCredentialsSecretId = $state("");
  sevenZipPath = $state("7z");

  get isDirty(): boolean {
    return !!this.values && JSON.stringify(this.buildPatch()) !== this.baselinePatch;
  }

  get presetOptions(): { value: string; label: string }[] {
    return [
      { value: "", label: "No default preset" },
      ...this.presets.map((preset) => ({ value: preset.id, label: preset.name })),
    ];
  }

  get rqbitCredentialOptions(): { value: string; label: string }[] {
    return [
      { value: "", label: "No rqbit credentials" },
      ...this.secrets
        .filter((secret) => secret.secret_type === "rqbit_credentials")
        .map((secret) => ({ value: secret.id, label: secret.name })),
    ];
  }

  sync(settings: PersistentSettings): void {
    this.values = settings;
    this.downloadDir = settings.download_dir ?? "";
    this.libraryRoot = settings.library_root ?? "";
    this.autoOrganize = settings.library_auto_organize;
    this.maxActive = String(settings.max_active);
    this.maxSegments = String(settings.max_segments);
    this.maxConnections = String(settings.max_connections_per_host);
    this.speedLimitMbps = String(Math.round(settings.global_speed_limit_bps / 125000 * 10) / 10);
    this.maxRetries = String(settings.max_retries);
    this.connectTimeout = String(settings.connect_timeout_secs);
    this.readTimeout = String(settings.read_timeout_secs);
    this.autoProvision = settings.auto_provision;
    this.ytdlpPath = settings.ytdlp;
    this.ffmpegPath = settings.ffmpeg;
    this.rqbitPath = settings.rqbit;
    this.rqbitApi = settings.rqbit_api;
    this.rqbitCredentialsSecretId = settings.rqbit_credentials_secret_id ?? "";
    this.sevenZipPath = settings.seven_zip;
    this.baselinePatch = JSON.stringify(this.buildPatch());
  }

  async load(): Promise<void> {
    if (!connection.client) return;
    this.loading = true;
    this.error = null;
    try {
      const [response, presets, profiles, secrets, tags, cleanupPolicies] = await Promise.all([
        connection.client.getSettings(),
        connection.client.listPresets(),
        connection.client.listProfiles(),
        connection.client.listSecrets({ limit: 100 }),
        connection.client.listTags({ limit: 250 }).catch(() => null),
        connection.client.getCleanupPolicies().catch(() => null),
      ]);
      this.sync(response.values);
      this.presets = presets;
      this.profiles = profiles;
      this.secrets = secrets.items;
      this.tags = tags?.items ?? [];
      this.cleanupPolicies = cleanupPolicies;
      this.restartRequired = response.restart_required;
    } catch (cause) {
      this.error = describeError(cause);
    } finally {
      this.loading = false;
    }
  }

  async loadNativeInfo(): Promise<void> {
    try {
      const [backend, installation] = await Promise.all([backendInfo(), setupInstallationInfo()]);
      this.backend = backend;
      this.installation = installation;
    } catch {
      this.backend = null;
      this.installation = null;
    }
  }

  async loadUpdateStatus(): Promise<void> {
    try {
      this.updateStatus = await readAppUpdateStatus();
    } catch {
      this.updateStatus = null;
    }
  }

  startUpdatePolling(): () => void {
    void this.loadUpdateStatus();
    const timer = window.setInterval(() => void this.loadUpdateStatus(), 2000);
    return () => window.clearInterval(timer);
  }

  positive(value: string, fallback: number): number {
    const parsed = Number(value);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
  }

  buildPatch(): PersistentSettingsPatch {
    return {
      download_dir: this.downloadDir.trim() || null,
      library_root: this.libraryRoot.trim() || null,
      library_auto_organize: this.autoOrganize,
      max_active: Math.max(1, Math.round(this.positive(this.maxActive, this.values?.max_active ?? 3))),
      max_segments: Math.max(1, Math.round(this.positive(this.maxSegments, this.values?.max_segments ?? 8))),
      max_connections_per_host: Math.max(1, Math.round(this.positive(this.maxConnections, this.values?.max_connections_per_host ?? 8))),
      global_speed_limit_bps: Math.round(this.positive(this.speedLimitMbps, 0) * 125000),
      max_retries: Math.round(this.positive(this.maxRetries, this.values?.max_retries ?? 4)),
      connect_timeout_secs: Math.max(1, Math.round(this.positive(this.connectTimeout, this.values?.connect_timeout_secs ?? 15))),
      read_timeout_secs: Math.max(1, Math.round(this.positive(this.readTimeout, this.values?.read_timeout_secs ?? 60))),
      ytdlp: this.ytdlpPath.trim() || "yt-dlp",
      ffmpeg: this.ffmpegPath.trim() || "ffmpeg",
      rqbit: this.rqbitPath.trim() || "rqbit",
      rqbit_api: this.rqbitApi.trim() || "http://127.0.0.1:3030",
      rqbit_credentials_secret_id: this.rqbitCredentialsSecretId || null,
      seven_zip: this.sevenZipPath.trim() || "7z",
      auto_provision: this.autoProvision,
    };
  }

  async save(): Promise<boolean> {
    if (!connection.client || !this.values || !this.isDirty) return true;
    this.saving = true;
    try {
      const patch = this.buildPatch();
      const validation = await connection.client.validateSettings(patch);
      if (!validation.valid) {
        notifications.error("Some settings are invalid", validation.issues.map((issue) => `${issue.field}: ${issue.message}`).join("\n"));
        return false;
      }
      const response = await connection.client.patchSettings(patch);
      this.sync(response.values);
      this.restartRequired = response.restart_required || validation.restart_required;
      notifications.success("Settings saved", this.restartRequired ? "Restart the backend to apply every change." : "Changes are active now.");
      return true;
    } catch (cause) {
      notifications.error("Couldn't save settings", describeError(cause));
      return false;
    } finally {
      this.saving = false;
    }
  }

  discardChanges(): void {
    if (this.values) this.sync(this.values);
  }

  async resetSettings(): Promise<void> {
    if (!connection.client) return;
    this.resetBusy = true;
    this.resetError = null;
    try {
      const response = await connection.client.resetSettings();
      this.sync(response.values);
      this.restartRequired = true;
      this.resetOpen = false;
      notifications.info("Backend settings reset", "A backend restart is required.");
    } catch (cause) {
      this.resetError = describeError(cause);
    } finally {
      this.resetBusy = false;
    }
  }

  async recheckApplicationUpdate(): Promise<void> {
    if (this.updateBusy) return;
    this.updateBusy = true;
    try {
      this.updateStatus = await checkAppUpdate();
      if (this.updateStatus.phase === "ready") {
        notifications.success(`Ravyn ${this.updateStatus.available_version ?? "update"} is ready`, "It will install silently when you close Ravyn.");
      } else if (this.updateStatus.phase === "up_to_date") {
        notifications.info("Ravyn is up to date");
      }
    } catch (cause) {
      notifications.error("Couldn't check for an app update", describeError(cause));
      await this.loadUpdateStatus();
    } finally {
      this.updateBusy = false;
    }
  }

  async repairInstalledApplication(): Promise<void> {
    if (this.updateBusy || this.repairBusy) return;
    this.repairBusy = true;
    try {
      this.updateStatus = await repairApplication();
      if (this.updateStatus.phase === "ready") {
        notifications.success(this.updateStatus.repair_mode ? "Repair package is ready" : `Ravyn ${this.updateStatus.available_version ?? "update"} is ready`, "The signed installer will run after you close Ravyn.");
      }
    } catch (cause) {
      notifications.error("Couldn't prepare application repair", describeError(cause));
      await this.loadUpdateStatus();
    } finally {
      this.repairBusy = false;
    }
  }

  updateHeading(status: AppUpdateStatus): string {
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

  updateDescription(status: AppUpdateStatus): string {
    if (status.phase === "ready") return status.repair_mode ? "The verified installer will reinstall Ravyn after a normal close." : "The verified installer will run silently after you close Ravyn.";
    if (status.phase === "downloading") {
      const total = status.total_bytes ?? 0;
      const percent = total > 0 ? Math.min(100, Math.round(status.downloaded_bytes / total * 100)) : 0;
      return total > 0 ? `${percent}% downloaded` : "Downloading and verifying the signed installer.";
    }
    if (status.last_error) return status.last_error;
    if (!status.automatic) return "Automatic updates require an installed Windows build.";
    return "Ravyn checks in the background and installs downloaded updates only when the app closes.";
  }

  updateResultHeading(status: AppUpdateStatus): string {
    const result = status.last_result;
    if (!result) return "";
    if (result.outcome === "succeeded" && result.from_version === result.to_version) return `Repaired Ravyn ${result.to_version}`;
    if (result.outcome === "succeeded") return `Updated to Ravyn ${result.to_version}`;
    if (result.outcome === "rolled_back") return `Ravyn ${result.to_version} was rolled back`;
    return `Ravyn ${result.to_version} could not be installed`;
  }

  updateResultDescription(status: AppUpdateStatus): string {
    const result = status.last_result;
    if (!result) return "";
    const completed = new Date(result.completed_at_unix_ms);
    const timestamp = Number.isNaN(completed.getTime()) ? "" : ` · ${completed.toLocaleString()}`;
    return `${result.message}${timestamp}`;
  }

  async saveCleanupPolicies(): Promise<void> {
    if (!connection.client || !this.cleanupPolicies || this.cleanupBusy) return;
    this.cleanupBusy = true;
    try {
      this.cleanupPolicies = await connection.client.updateCleanupPolicies(this.cleanupPolicies);
      notifications.success("Cleanup policy saved");
    } catch (cause) {
      notifications.error("Couldn't save the cleanup policy", describeError(cause));
    } finally {
      this.cleanupBusy = false;
    }
  }

  async runCleanup(): Promise<void> {
    if (!connection.client || this.cleanupBusy) return;
    this.cleanupBusy = true;
    try {
      this.cleanupReport = await connection.client.runLibraryCleanup();
      notifications.info("Library cleanup completed");
    } catch (cause) {
      notifications.error("Couldn't run library cleanup", describeError(cause));
    } finally {
      this.cleanupBusy = false;
    }
  }

  scheduleTemplatePreview(): void {
    if (this.templatePreviewTimer) clearTimeout(this.templatePreviewTimer);
    const template = this.presetTemplate.trim();
    if (!template) {
      this.templatePreview = null;
      this.templatePreviewMissing = [];
      this.templatePreviewError = null;
      return;
    }
    this.templatePreviewTimer = setTimeout(() => void this.runTemplatePreview(template), 350);
  }

  async runTemplatePreview(template: string): Promise<void> {
    if (!connection.client) return;
    const now = new Date();
    try {
      const preview = await connection.client.previewTemplate({
        template,
        variables: {
          filename: "example-file.zip",
          stem: "example-file",
          extension: "zip",
          host: "example.com",
          year: String(now.getFullYear()),
          month: String(now.getMonth() + 1).padStart(2, "0"),
          day: String(now.getDate()).padStart(2, "0"),
        },
      });
      if (this.presetTemplate.trim() !== template) return;
      this.templatePreview = preview.rendered;
      this.templatePreviewMissing = preview.missing_variables;
      this.templatePreviewError = null;
    } catch (cause) {
      if (this.presetTemplate.trim() !== template) return;
      this.templatePreview = null;
      this.templatePreviewMissing = [];
      this.templatePreviewError = describeError(cause);
    }
  }

  openPresetEditor(preset: DownloadPreset | null): void {
    this.editingPreset = preset;
    this.presetName = preset?.name ?? "";
    this.presetDestination = preset?.payload.destination ?? "";
    this.presetTemplate = preset?.payload.filename_template ?? "";
    this.presetPriority = String(preset?.payload.priority ?? 0);
    this.presetSpeed = String(preset?.payload.speed_limit_bps ? Math.round(preset.payload.speed_limit_bps / 125000 * 10) / 10 : 0);
    this.templatePreview = null;
    this.templatePreviewMissing = [];
    this.templatePreviewError = null;
    this.scheduleTemplatePreview();
    this.presetOpen = true;
  }

  async savePreset(): Promise<void> {
    if (!connection.client || !this.presetName.trim() || this.presetBusy) return;
    this.presetBusy = true;
    try {
      const changes = {
        destination: this.presetDestination.trim() || null,
        filename_template: this.presetTemplate.trim() || null,
        priority: Math.round(Number(this.presetPriority) || 0),
        speed_limit_bps: Math.round(Math.max(0, Number(this.presetSpeed) || 0) * 125000),
      };
      const saved = this.editingPreset
        ? await connection.client.updatePreset(this.editingPreset.id, { name: this.presetName.trim(), payload: { ...this.editingPreset.payload, ...changes } })
        : await connection.client.createPreset({ name: this.presetName.trim(), payload: changes });
      this.presets = [...this.presets.filter((preset) => preset.id !== saved.id), saved].sort((a, b) => a.name.localeCompare(b.name));
      this.presetOpen = false;
      this.presetName = "";
      notifications.success(this.editingPreset ? "Preset updated" : "Preset created");
      this.editingPreset = null;
    } catch (cause) {
      notifications.error(this.editingPreset ? "Couldn't update the preset" : "Couldn't create the preset", describeError(cause));
    } finally {
      this.presetBusy = false;
    }
  }

  openProfileEditor(profile: UserProfile | null): void {
    this.editingProfile = profile;
    this.profileName = profile?.name ?? "";
    this.profileMaxActive = String(profile?.settings_patch.max_active ?? 3);
    this.profileSpeed = String(profile?.settings_patch.global_speed_limit_bps ? Math.round(profile.settings_patch.global_speed_limit_bps / 125000 * 10) / 10 : 0);
    this.profilePresetId = profile?.default_preset_id ?? "";
    this.profileOpen = true;
  }

  async saveProfile(): Promise<void> {
    if (!connection.client || !this.profileName.trim() || this.profileBusy) return;
    this.profileBusy = true;
    try {
      const input = {
        name: this.profileName.trim(),
        default_preset_id: this.profilePresetId || null,
        settings_patch: {
          ...(this.editingProfile?.settings_patch ?? {}),
          max_active: Math.max(1, Math.round(Number(this.profileMaxActive) || 3)),
          global_speed_limit_bps: Math.round(Math.max(0, Number(this.profileSpeed) || 0) * 125000),
        },
      };
      const saved = this.editingProfile
        ? await connection.client.updateProfile(this.editingProfile.id, input)
        : await connection.client.createProfile(input);
      this.profiles = [...this.profiles.filter((profile) => profile.id !== saved.id), saved].sort((a, b) => Number(b.active) - Number(a.active) || a.name.localeCompare(b.name));
      this.profileOpen = false;
      this.profileName = "";
      notifications.success(this.editingProfile ? "Profile updated" : "Profile created");
      this.editingProfile = null;
    } catch (cause) {
      notifications.error(this.editingProfile ? "Couldn't update the profile" : "Couldn't create the profile", describeError(cause));
    } finally {
      this.profileBusy = false;
    }
  }

  async activateProfile(profile: UserProfile): Promise<void> {
    if (!connection.client || this.profileBusy) return;
    this.profileBusy = true;
    try {
      const response = await connection.client.activateProfile(profile.id);
      this.profiles = this.profiles.map((item) => ({ ...item, active: item.id === response.profile.id }));
      const settings = await connection.client.getSettings();
      this.sync(settings.values);
      this.restartRequired = response.restart_required;
      notifications.success("Profile activated", response.restart_required ? "Restart the backend to apply every profile setting." : "Profile settings are active now.");
    } catch (cause) {
      notifications.error("Couldn't activate the profile", describeError(cause));
    } finally {
      this.profileBusy = false;
    }
  }

  async deleteTag(tag: TagRecord): Promise<void> {
    if (!connection.client || this.tagDeleteBusy !== null) return;
    this.tagDeleteBusy = tag.id;
    try {
      await connection.client.deleteTag(tag.id);
      this.tags = this.tags.filter((item) => item.id !== tag.id);
      notifications.info(`Tag "${tag.name}" deleted`);
    } catch (cause) {
      notifications.error("Couldn't delete the tag", describeError(cause));
    } finally {
      this.tagDeleteBusy = null;
    }
  }

  async confirmManagementDelete(): Promise<void> {
    if (!connection.client || !this.deleteTarget || this.deleteBusy) return;
    this.deleteBusy = true;
    this.deleteError = null;
    const target = this.deleteTarget;
    try {
      if (target.kind === "preset") {
        await connection.client.deletePreset(target.id);
        this.presets = this.presets.filter((item) => item.id !== target.id);
      } else {
        await connection.client.deleteProfile(target.id);
        this.profiles = this.profiles.filter((item) => item.id !== target.id);
      }
      notifications.info(`${target.kind === "preset" ? "Preset" : "Profile"} deleted`);
      this.deleteTarget = null;
    } catch (cause) {
      this.deleteError = describeError(cause);
    } finally {
      this.deleteBusy = false;
    }
  }

  secretTypeLabel(type: SecretType): string {
    const labels: Record<SecretType, string> = {
      api_token: "API token",
      proxy_credentials: "Proxy credentials",
      rqbit_credentials: "rqbit credentials",
      cookies: "Cookies",
      authentication_header: "Authorization header",
      tls_certificate: "TLS certificate",
      private_key: "Private key",
    };
    return labels[type];
  }

  openSecretEditor(reference: SecretReference | null = null): void {
    this.secretName = reference?.name ?? "";
    this.secretType = reference?.secret_type ?? "api_token";
    this.secretValue = "";
    this.secretUsername = "";
    this.secretPassword = "";
    this.secretError = null;
    this.secretOpen = true;
  }

  resetSecretEditor(): void {
    this.secretOpen = false;
    this.secretName = "";
    this.secretType = "api_token";
    this.secretValue = "";
    this.secretUsername = "";
    this.secretPassword = "";
    this.secretError = null;
  }

  secretPayload(): string {
    if (this.secretType === "proxy_credentials" || this.secretType === "rqbit_credentials") {
      return JSON.stringify({ username: this.secretUsername, password: this.secretPassword });
    }
    return this.secretValue;
  }

  async saveSecret(): Promise<void> {
    const payload = this.secretPayload();
    if (!connection.client || this.secretBusy || !this.secretName.trim() || !payload) return;
    this.secretBusy = true;
    this.secretError = null;
    try {
      const reference = await connection.client.putSecret({ name: this.secretName.trim(), secret_type: this.secretType, secret: payload });
      this.secrets = [reference, ...this.secrets.filter((item) => item.id !== reference.id && item.name !== reference.name)].sort((a, b) => a.name.localeCompare(b.name));
      this.resetSecretEditor();
      notifications.success("Secret stored", "The value was written to the platform credential store.");
    } catch (cause) {
      this.secretError = describeError(cause);
    } finally {
      this.secretBusy = false;
    }
  }

  async deleteSecret(): Promise<void> {
    if (!connection.client || !this.secretDeleteTarget || this.secretDeleteBusy) return;
    this.secretDeleteBusy = true;
    this.secretDeleteError = null;
    const target = this.secretDeleteTarget;
    try {
      await connection.client.deleteSecret(target.id);
      this.secrets = this.secrets.filter((item) => item.id !== target.id);
      if (this.rqbitCredentialsSecretId === target.id) this.rqbitCredentialsSecretId = "";
      this.secretDeleteTarget = null;
      notifications.info("Secret removed");
    } catch (cause) {
      this.secretDeleteError = describeError(cause);
    } finally {
      this.secretDeleteBusy = false;
    }
  }

  applyAppearanceBackdrop(value: string): void {
    navigation.setBackdropImage(value);
  }
}
