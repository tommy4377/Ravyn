/**
 * Setup flow controller.
 *
 * Owns the whole setup session: backend connection, detection, selections,
 * library preparation, Windows integration, component provisioning driven by
 * real backend events, and the deterministic handoff.
 */

import { RavynClient } from "../api/client";
import { describeError } from "../api/errors";
import { RavynEventClient } from "../api/events.svelte";
import type {
  ComponentId,
  ComponentOverview,
  ComponentState,
  FeatureId,
  InstallationMode,
  RavynEvent,
  SetupProfile,
  SetupState,
} from "../api/types";
import { buildIntegrationRequest } from "./installationPolicy";
import {
  applyWindowsIntegration,
  backendInfo,
  finishSetupHandoff,
  restartApplication,
  setupInstallationInfo,
  type InstallationInfo,
  type IntegrationRequest,
  type IntegrationReport,
} from "../native/tauri";

export type SetupStep =
  | "welcome"
  | "setup-type"
  | "features"
  | "library"
  | "preferences"
  | "install"
  | "done";

export type SetupMode = "install" | "update" | "repair" | "first-run";

export interface ComponentProgress {
  state: ComponentState;
  progressPct: number | null;
  bytesDownloaded: number | null;
  bytesTotal: number | null;
  message: string | null;
}

const PROFILE_FEATURES: Record<Exclude<SetupProfile, "custom">, FeatureId[]> = {
  minimal: ["standard_downloads"],
  recommended: [
    "standard_downloads",
    "video_extraction",
    "media_merging",
    "archive_extraction",
  ],
  full: [
    "standard_downloads",
    "video_extraction",
    "media_merging",
    "torrent_support",
    "archive_extraction",
  ],
};

export class SetupController {
  // Connection
  client = $state<RavynClient | null>(null);
  events: RavynEventClient | null = null;
  connectionError = $state<string | null>(null);
  loading = $state(true);

  // Detection
  installation = $state<InstallationInfo | null>(null);
  setupState = $state<SetupState | null>(null);
  mode = $state<SetupMode>("first-run");
  applicationMode = $state<InstallationMode>("installed");

  // Flow
  step = $state<SetupStep>("welcome");
  stepError = $state<string | null>(null);
  busy = $state(false);

  // Selections
  profile = $state<SetupProfile>("recommended");
  features = $state<Set<FeatureId>>(new Set(PROFILE_FEATURES.recommended));
  overview = $state<ComponentOverview | null>(null);

  // Library
  libraryPath = $state("");
  libraryError = $state<string | null>(null);
  availableBytes = $state<number | null>(null);
  libraryPrepared = $state(false);

  // Preferences
  desktopShortcut = $state(false);
  startMenuShortcut = $state(true);
  launchAtStartup = $state(false);
  launchAfterSetup = $state(true);
  autoOrganize = $state(true);
  autoProvision = $state(true);
  maxActive = $state("3");
  speedLimitMbps = $state("0");

  // Provisioning
  progress = $state<Map<ComponentId, ComponentProgress>>(new Map());
  integrationReport = $state<IntegrationReport | null>(null);
  installationReported = $state(false);
  installationReady = $state(false);
  provisioningStarted = $state(false);
  provisioningFinished = $state(false);

  /** Connect to the embedded backend and detect the installation. */
  async init(): Promise<void> {
    this.loading = true;
    this.connectionError = null;
    try {
      const [backend, installation] = await Promise.all([
        backendInfo(),
        setupInstallationInfo(),
      ]);
      this.installation = installation;
      this.client = new RavynClient(backend.base_url, backend.api_token);
      this.events = new RavynEventClient(backend.base_url, backend.api_token);
      this.events.connect();
      this.events.subscribe((event) => this.onEvent(event));

      const [setupState, overview, settings] = await Promise.all([
        this.client.getSetupState(),
        this.client.getComponents(),
        this.client.getSettings(),
      ]);
      this.setupState = setupState;
      this.overview = overview;
      this.autoOrganize = settings.values.library_auto_organize;
      this.autoProvision = settings.values.auto_provision;
      this.maxActive = String(settings.values.max_active);
      this.speedLimitMbps = String(
        Math.round(settings.values.global_speed_limit_bps / 125000 * 10) / 10,
      );
      this.applyDetection(setupState, installation);
      if (setupState.integration_consent) {
        const consent = setupState.integration_consent;
        this.applicationMode = installation.development
          ? "development"
          : consent.installation_mode;
        this.startMenuShortcut = consent.start_menu_shortcut;
        this.desktopShortcut = consent.desktop_shortcut;
        this.launchAtStartup = consent.launch_at_startup;
        this.launchAfterSetup = consent.launch_after_setup;
      }
      this.installationReported = setupState.installation !== null;
      this.installationReady =
        setupState.installation?.integration_completed === true;
      this.libraryPath =
        setupState.library_root ?? this.defaultLibraryPath();
      this.libraryPrepared = setupState.library_prepared;
      if (setupState.features_selected && setupState.setup_profile) {
        this.profile = setupState.setup_profile;
        this.features = new Set(
          overview.features.filter((f) => f.enabled).map((f) => f.feature),
        );
        this.features.add("standard_downloads");
      }
      if (setupState.features_selected && setupState.library_prepared) {
        this.step = "preferences";
      }
    } catch (error) {
      this.connectionError = describeError(error);
    } finally {
      this.loading = false;
    }
  }

  private applyDetection(
    state: SetupState,
    installation: InstallationInfo,
  ): void {
    this.applicationMode = installation.development
      ? "development"
      : (state.installation?.installation_mode ?? "installed");

    if (!installation.installed && !state.completed) {
      this.mode = "first-run";
    } else if (!state.completed) {
      this.mode = "repair";
    } else if (
      installation.installed_version &&
      installation.installed_version !== installation.app_version
    ) {
      this.mode = "update";
    } else {
      this.mode = "repair";
    }
  }

  private defaultLibraryPath(): string {
    // Fallback shown before the user picks; the backend validates the final
    // value. USERPROFILE is not readable from the webview, so derive it from
    // the backend data directory (…\AppData\Local\Ravyn → …\Downloads\Ravyn).
    const dataDir = this.setupState?.data_dir ?? "";
    const marker = "\\AppData\\Local\\";
    const index = dataDir.indexOf(marker);
    if (index > 0) {
      return `${dataDir.slice(0, index)}\\Downloads\\Ravyn`;
    }
    return "";
  }

  private onEvent(event: RavynEvent): void {
    if (event.type === "component") {
      const e = event as import("../api/types").ComponentEvent;
      const next = new Map(this.progress);
      next.set(e.component, {
        state: e.state,
        progressPct: e.progress_pct ?? null,
        bytesDownloaded: e.bytes_downloaded ?? null,
        bytesTotal: e.bytes_total ?? null,
        message: e.message ?? null,
      });
      this.progress = next;
      if (this.provisioningStarted) {
        this.updateProvisioningFinished();
      }
    } else if (event.type === "resync_required") {
      void this.refreshOverview();
    }
  }

  async refreshOverview(): Promise<void> {
    if (!this.client) return;
    try {
      this.overview = await this.client.getComponents();
    } catch (error) {
      this.stepError = describeError(error);
    }
  }

  /** Components required by the enabled features. */
  requiredComponents(): ComponentId[] {
    if (!this.overview) return [];
    const required = new Set<ComponentId>();
    for (const feature of this.overview.features) {
      if (this.features.has(feature.feature)) {
        for (const component of feature.required_components) {
          required.add(component);
        }
      }
    }
    return [...required];
  }

  applyProfile(profile: SetupProfile): void {
    this.profile = profile;
    if (profile !== "custom") {
      this.features = new Set(PROFILE_FEATURES[profile]);
    } else {
      this.features = new Set(this.features);
      this.features.add("standard_downloads");
    }
  }

  setApplicationMode(mode: InstallationMode): void {
    if (this.installation?.development) {
      this.applicationMode = "development";
      return;
    }
    this.applicationMode = mode;
    if (mode !== "installed") {
      this.startMenuShortcut = false;
      this.desktopShortcut = false;
      this.launchAtStartup = false;
    }
  }

  toggleFeature(feature: FeatureId, enabled: boolean): void {
    if (feature === "standard_downloads") return;
    const next = new Set(this.features);
    if (enabled) {
      next.add(feature);
    } else {
      next.delete(feature);
    }
    next.add("standard_downloads");
    this.features = next;
    this.profile = "custom";
  }

  /** Persist the feature selection (features stage → next). */
  async saveFeatures(): Promise<boolean> {
    if (!this.client || !this.overview) return false;
    this.busy = true;
    this.stepError = null;
    try {
      const selections = this.overview.features.map((f) => ({
        feature: f.feature,
        enabled: this.features.has(f.feature),
      }));
      this.overview = await this.client.saveFeatureSelections(
        this.profile,
        selections,
      );
      return true;
    } catch (error) {
      this.stepError = describeError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** Validate and create the library layout (library stage → next). */
  async prepareLibrary(): Promise<boolean> {
    if (!this.client) return false;
    this.busy = true;
    this.libraryError = null;
    try {
      const result = await this.client.prepareLibrary(this.libraryPath);
      this.libraryPath = result.path;
      this.availableBytes = result.available_bytes;
      this.libraryPrepared = true;
      return true;
    } catch (error) {
      this.libraryError = describeError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  async savePreferences(): Promise<boolean> {
    if (!this.client) return false;
    this.busy = true;
    this.stepError = null;
    try {
      const maxActive = Math.max(1, Math.round(Number(this.maxActive) || 3));
      const speedLimitMbps = Math.max(0, Number(this.speedLimitMbps) || 0);
      const patch = {
        library_auto_organize: this.autoOrganize,
        auto_provision: this.autoProvision,
        max_active: maxActive,
        global_speed_limit_bps: Math.round(speedLimitMbps * 125000),
      };
      const validation = await this.client.validateSettings(patch);
      if (!validation.valid) {
        this.stepError = validation.issues
          .map((issue) => `${issue.field}: ${issue.message}`)
          .join("\n");
        return false;
      }
      const response = await this.client.patchSettings(patch);
      if (response.restart_required) {
        this.setupState = await this.client.getSetupState();
      }
      return true;
    } catch (error) {
      this.stepError = describeError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** Run application setup and component provisioning. */
  async runInstallation(): Promise<void> {
    if (!this.client) return;
    this.provisioningStarted = true;
    this.provisioningFinished = false;
    this.stepError = null;

    await this.runApplicationInstallation();

    const pending = this.componentsToInstall();
    for (const component of pending) {
      try {
        await this.client.installComponent(component);
      } catch (error) {
        const next = new Map(this.progress);
        next.set(component, {
          state: "failed",
          progressPct: null,
          bytesDownloaded: null,
          bytesTotal: null,
          message: describeError(error),
        });
        this.progress = next;
      }
    }
    this.updateProvisioningFinished();
  }

  private async runApplicationInstallation(): Promise<void> {
    if (!this.client || !this.installation) return;
    this.installationReported = false;
    this.installationReady = false;

    try {
      const request: IntegrationRequest =
        this.applicationMode === "installed"
          ? (buildIntegrationRequest(
              this.applicationMode,
              this.installation,
              {
                startMenuShortcut: this.startMenuShortcut,
                desktopShortcut: this.desktopShortcut,
                launchAtStartup: this.launchAtStartup,
              },
            ) ?? (() => {
              throw new Error(
                "installed mode did not produce an integration request",
              );
            })())
          : {
              install_application: false,
              register_installed_app: false,
              start_menu_shortcut: false,
              desktop_shortcut: false,
              launch_at_startup: false,
            };
      const previousConsentId = this.setupState?.integration_consent?.id ?? null;
      this.setupState = await this.client.saveIntegrationConsent({
        installation_mode: this.applicationMode,
        ...request,
        launch_after_setup: this.launchAfterSetup,
      });
      const consentUnchanged =
        previousConsentId !== null &&
        previousConsentId === this.setupState.integration_consent?.id;
      const persistedInstallation = this.setupState.installation;

      if (
        consentUnchanged &&
        (persistedInstallation?.integration_completed === true ||
          this.integrationReport?.integration_completed === true)
      ) {
        if (!this.integrationReport?.integration_completed && persistedInstallation) {
          this.integrationReport = {
            steps: [
              {
                step: "restore_persisted_integration",
                applied: false,
                skipped_reason:
                  "the consented integration was already verified before restart",
                error: null,
              },
            ],
            install_dir: this.installation.install_dir,
            installed_exe: persistedInstallation.installed_exe,
            installed_version: persistedInstallation.installed_version,
            installed_sha256: persistedInstallation.installed_sha256,
            integration_completed: true,
            integration_errors: [],
          };
        }
      } else if (this.applicationMode === "installed") {
        this.integrationReport = await applyWindowsIntegration(request);
      } else {
        const reason =
          this.applicationMode === "development"
            ? "development build runs in place"
            : "portable mode selected";
        const verified =
          this.installation.exe_path.length > 0 &&
          this.installation.exe_sha256 !== null;
        this.integrationReport = {
          steps: [
            {
              step: "install_application",
              applied: false,
              skipped_reason: reason,
              error: verified ? null : "the running executable could not be verified",
            },
          ],
          install_dir: this.installation.install_dir,
          installed_exe: this.installation.exe_path || null,
          installed_version: this.installation.app_version,
          installed_sha256: this.installation.exe_sha256,
          integration_completed: verified,
          integration_errors: verified
            ? []
            : ["the running executable could not be verified"],
        };
      }

      if (!this.integrationReport) {
        throw new Error("Ravyn did not produce an installation report");
      }

      this.setupState = await this.client.reportInstallation({
        installation_mode: this.applicationMode,
        installed_exe: this.integrationReport.installed_exe,
        installed_version: this.integrationReport.installed_version,
        installed_sha256: this.integrationReport.installed_sha256,
        integration_completed: this.integrationReport.integration_completed,
        integration_errors: this.integrationReport.integration_errors,
        relaunch_pending:
          this.applicationMode === "installed" && this.launchAfterSetup,
      });
      this.installationReported = true;
      this.installationReady =
        this.setupState.installation?.integration_completed === true;
      if (!this.installationReady) {
        this.stepError =
          this.integrationReport.integration_errors[0] ??
          "Ravyn could not verify the selected application mode.";
      }
    } catch (error) {
      this.stepError = describeError(error);
      this.installationReported = false;
      this.installationReady = false;
    }
  }

  async retryApplicationInstallation(): Promise<void> {
    this.stepError = null;
    await this.runApplicationInstallation();
  }

  componentsToInstall(): ComponentId[] {
    if (!this.overview) return [];
    const required = new Set(this.requiredComponents());
    return this.overview.components
      .filter(
        (c) =>
          required.has(c.component) &&
          c.state !== "installed" &&
          c.state !== "custom_path",
      )
      .map((c) => c.component);
  }

  /** Current provisioning display state for a component. */
  componentProgress(component: ComponentId): ComponentProgress {
    const live = this.progress.get(component);
    if (live) return live;
    const status = this.overview?.components.find(
      (c) => c.component === component,
    );
    return {
      state: status?.state ?? "not_installed",
      progressPct: null,
      bytesDownloaded: null,
      bytesTotal: null,
      message: status?.error_message ?? null,
    };
  }

  private updateProvisioningFinished(): void {
    const busyStates: ComponentState[] = [
      "queued",
      "downloading",
      "verifying",
      "installing",
    ];
    const stillBusy = this.componentsToInstall().some((component) =>
      busyStates.includes(this.componentProgress(component).state),
    );
    if (!stillBusy && this.provisioningStarted) {
      this.provisioningFinished = true;
      void this.refreshOverview();
    }
  }

  async retryComponent(component: ComponentId): Promise<void> {
    if (!this.client) return;
    try {
      await this.client.installComponent(component, true);
    } catch (error) {
      const next = new Map(this.progress);
      next.set(component, {
        state: "failed",
        progressPct: null,
        bytesDownloaded: null,
        bytesTotal: null,
        message: describeError(error),
      });
      this.progress = next;
    }
  }

  async cancelComponent(component: ComponentId): Promise<void> {
    if (!this.client) return;
    try {
      await this.client.cancelComponentInstallation(component);
    } catch (error) {
      this.stepError = describeError(error);
    }
  }

  canCompleteSetup(): boolean {
    return (
      this.provisioningFinished &&
      this.installationReported &&
      this.installationReady &&
      this.setupState?.restart_required !== true &&
      this.setupState?.integration_consent != null
    );
  }

  /** Commit setup completion (done stage entry). */
  async completeSetup(): Promise<boolean> {
    if (!this.client) return false;
    if (!this.canCompleteSetup()) {
      this.stepError = this.setupState?.restart_required
        ? "Ravyn must restart its background service before setup can be completed with the selected library."
        : "The application installation must be verified before setup can be completed.";
      return false;
    }
    this.busy = true;
    try {
      this.setupState = await this.client.completeSetup();
      return true;
    } catch (error) {
      this.stepError = describeError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  async restartForPendingSettings(): Promise<void> {
    this.stepError = null;
    try {
      await restartApplication();
    } catch (error) {
      this.stepError = describeError(error);
    }
  }

  /** Deterministic handoff to the main window. */
  async openRavyn(): Promise<void> {
    this.stepError = null;
    try {
      await finishSetupHandoff(
        this.applicationMode === "installed"
          ? (this.integrationReport?.installed_exe ?? undefined)
          : undefined,
        this.launchAfterSetup,
      );
      // Installed mode exits this portable/setup process after launching the
      // installed copy. Portable mode creates the current-process main window.
    } catch (error) {
      this.stepError = describeError(error);
    }
  }
}
