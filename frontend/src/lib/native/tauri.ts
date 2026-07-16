/**
 * Typed wrappers around the Ravyn desktop shell commands.
 * All native platform behavior stays behind this module.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export interface BackendInfo {
  base_url: string;
  api_token: string;
  data_dir: string;
  setup_completed: boolean;
}

export interface InstallationInfo {
  app_version: string;
  exe_path: string;
  installed: boolean;
  installed_version: string | null;
  install_dir: string | null;
  portable: boolean;
  development: boolean;
  exe_sha256: string | null;
}


export type AppUpdatePhase =
  | "disabled"
  | "idle"
  | "checking"
  | "up_to_date"
  | "downloading"
  | "cancelling"
  | "cancelled"
  | "ready"
  | "installing"
  | "error";

export interface AppUpdateResult {
  outcome: "succeeded" | "rolled_back" | "failed" | string;
  from_version: string;
  to_version: string;
  completed_at_unix_ms: number;
  message: string;
}

export interface AppUpdateStatus {
  configured: boolean;
  automatic: boolean;
  phase: AppUpdatePhase;
  current_version: string;
  available_version: string | null;
  downloaded_bytes: number;
  total_bytes: number | null;
  notes: string | null;
  last_error: string | null;
  install_on_exit: boolean;
  repair_mode: boolean;
  last_result: AppUpdateResult | null;
  last_checked_at_unix_ms: number | null;
  next_check_at_unix_ms: number | null;
  automatic_check_interval_secs: number | null;
}

export interface IntegrationRequest {
  install_application: boolean;
  register_installed_app: boolean;
  start_menu_shortcut: boolean;
  desktop_shortcut: boolean;
  launch_at_startup: boolean;
}

export interface IntegrationStepResult {
  step: string;
  applied: boolean;
  skipped_reason: string | null;
  error: string | null;
}

export interface IntegrationReport {
  steps: IntegrationStepResult[];
  install_dir: string | null;
  installed_exe: string | null;
  installed_version: string | null;
  installed_sha256: string | null;
  integration_completed: boolean;
  integration_errors: string[];
}

/** Wait for the embedded backend and return its base URL. */
export function backendInfo(): Promise<BackendInfo> {
  return invoke<BackendInfo>("backend_info");
}

export function setupInstallationInfo(): Promise<InstallationInfo> {
  return invoke<InstallationInfo>("setup_installation_info");
}

export function applyWindowsIntegration(
  request: IntegrationRequest,
): Promise<IntegrationReport> {
  return invoke<IntegrationReport>("apply_windows_integration", { request });
}

export function finishSetupHandoff(
  installedExe: string | undefined,
  launchAfterSetup: boolean,
): Promise<void> {
  return invoke("finish_setup_handoff", {
    installedExe: installedExe ?? null,
    launchAfterSetup,
  });
}

export function restartApplication(): Promise<void> {
  return invoke("restart_application");
}

export function mainWindowReady(): Promise<void> {
  return invoke("main_window_ready");
}

/** Register Ravyn as a torrent handler and open Windows Default Apps. */
export function promptTorrentDefaultApp(): Promise<void> {
  return invoke("prompt_torrent_default_app");
}

export function appUpdateStatus(): Promise<AppUpdateStatus> {
  return invoke<AppUpdateStatus>("app_update_status");
}

export function checkAppUpdate(): Promise<AppUpdateStatus> {
  return invoke<AppUpdateStatus>("check_app_update");
}

export function repairApplication(): Promise<AppUpdateStatus> {
  return invoke<AppUpdateStatus>("repair_application");
}

export function cancelAppUpdate(): Promise<AppUpdateStatus> {
  return invoke<AppUpdateStatus>("cancel_app_update");
}

export function installAppUpdateNow(): Promise<void> {
  return invoke("install_app_update_now");
}


export interface BrowserIntegrationStatus {
  supported: boolean;
  registered: boolean;
  host_name: string;
  extension_id: string;
  manifest_path: string | null;
  executable_path: string | null;
  installed_mode: boolean;
  error: string | null;
}

export interface BrowserAction {
  section: string | null;
  source_url: string | null;
}

export function browserIntegrationStatus(): Promise<BrowserIntegrationStatus> {
  return invoke<BrowserIntegrationStatus>("browser_integration_status");
}

export function repairBrowserIntegration(): Promise<BrowserIntegrationStatus> {
  return invoke<BrowserIntegrationStatus>("repair_browser_integration");
}

export function removeBrowserIntegration(): Promise<BrowserIntegrationStatus> {
  return invoke<BrowserIntegrationStatus>("remove_browser_integration");
}

export function takeBrowserAction(): Promise<BrowserAction | null> {
  return invoke<BrowserAction | null>("take_browser_action");
}

/** Native folder picker; returns the chosen absolute path or null. */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  const result = await open({
    directory: true,
    multiple: false,
    defaultPath,
    title: "Choose a folder",
  });
  return typeof result === "string" ? result : null;
}

/** Native executable picker; returns the chosen absolute path or null. */
export async function pickExecutable(defaultPath?: string): Promise<string | null> {
  const result = await open({
    directory: false,
    multiple: false,
    defaultPath,
    title: "Choose an executable",
    filters: [
      { name: "Executable files", extensions: ["exe", "cmd", "bat", "com"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  return typeof result === "string" ? result : null;
}

export type WallpaperPosition = "center" | "tile" | "stretch" | "fit" | "fill" | "span";

export interface DesktopAppearance {
  supported: boolean;
  wallpaper_path: string | null;
  wallpaper_revision: string | null;
  wallpaper_position: WallpaperPosition;
  plane_x: number;
  plane_y: number;
  plane_width: number;
  plane_height: number;
  window_x: number;
  window_y: number;
  frame_offset_x: number;
  frame_offset_y: number;
  scale_factor: number;
  accent_color: string | null;
  transparency_enabled: boolean;
}

export function desktopAppearance(): Promise<DesktopAppearance> {
  return invoke<DesktopAppearance>("desktop_appearance");
}

export function openNativePath(path: string): Promise<void> {
  return invoke("open_native_path", { path });
}

export function revealNativePath(path: string): Promise<void> {
  return invoke("reveal_native_path", { path });
}

/** Shows a native Windows notification for a download event. */
export function notifyNative(title: string, body?: string): Promise<void> {
  return invoke("notify_native", { title, body });
}

/** Opens the compact always-on-top download progress window. */
export function openCompactWindow(): Promise<void> {
  return invoke("open_compact_window");
}

/** Brings the main Ravyn window to the foreground. */
export function focusMainWindow(): Promise<void> {
  return invoke("focus_main_window");
}

export type TrayAction = "pause-all" | "resume-all";

/** Subscribes to actions triggered from the system tray menu. */
export function onTrayAction(handler: (action: TrayAction) => void): Promise<UnlistenFn> {
  return listen<TrayAction>("ravyn://tray-action", (event) => handler(event.payload));
}
