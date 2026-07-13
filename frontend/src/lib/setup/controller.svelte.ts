/**
 * Setup flow controller.
 *
 * Owns the whole setup session: backend connection, detection, selections,
 * library preparation, Windows integration, component provisioning driven by
 * real backend events, and the deterministic handoff.
 */

import { ApiError, RavynClient } from "../api/client";
import { RavynEventClient } from "../api/events.svelte";
import type {
  ComponentId,
  ComponentOverview,
  ComponentState,
  FeatureId,
  RavynEvent,
  SetupProfile,
  SetupState,
} from "../api/types";
import {
  applyWindowsIntegration,
  backendInfo,
  finishSetupHandoff,
  setupInstallationInfo,
  type InstallationInfo,
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

  // Provisioning
  progress = $state<Map<ComponentId, ComponentProgress>>(new Map());
  integrationReport = $state<IntegrationReport | null>(null);
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

      const [setupState, overview] = await Promise.all([
        this.client.getSetupState(),
        this.client.getComponents(),
      ]);
      this.setupState = setupState;
      this.overview = overview;
      this.applyDetection(setupState, installation);
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

  /** Run installation + provisioning (install stage). */
  async runInstallation(): Promise<void> {
    if (!this.client) return;
    this.provisioningStarted = true;
    this.provisioningFinished = false;
    this.stepError = null;

    // 1. Windows integration (application install, shortcuts, registration).
    try {
      this.integrationReport = await applyWindowsIntegration({
        install_application: true,
        register_installed_app: true,
        start_menu_shortcut: this.startMenuShortcut,
        desktop_shortcut: this.desktopShortcut,
        launch_at_startup: this.launchAtStartup,
      });
    } catch (error) {
      this.stepError = describeError(error);
    }

    // 2. Component provisioning for enabled features.
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

  /** Commit setup completion (done stage entry). */
  async completeSetup(): Promise<boolean> {
    if (!this.client) return false;
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

  /** Deterministic handoff to the main window. */
  async openRavyn(): Promise<void> {
    this.stepError = null;
    try {
      await finishSetupHandoff(this.integrationReport?.installed_exe ?? undefined);
      // Installed mode exits this portable/setup process after launching the
      // installed copy. Portable mode creates the current-process main window.
    } catch (error) {
      this.stepError = describeError(error);
    }
  }
}

export function describeError(error: unknown): string {
  if (error instanceof ApiError) {
    return `${error.message} (${error.code})`;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
