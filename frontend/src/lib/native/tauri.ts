/**
 * Typed wrappers around the Ravyn desktop shell commands.
 * All native platform behavior stays behind this module.
 */

import { invoke } from "@tauri-apps/api/core";
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
  | "ready"
  | "installing"
  | "error";

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

export function appUpdateStatus(): Promise<AppUpdateStatus> {
  return invoke<AppUpdateStatus>("app_update_status");
}

export function checkAppUpdate(): Promise<AppUpdateStatus> {
  return invoke<AppUpdateStatus>("check_app_update");
}

/** Native folder picker; returns the chosen absolute path or null. */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  const result = await open({
    directory: true,
    multiple: false,
    defaultPath,
    title: "Choose the Ravyn library folder",
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
