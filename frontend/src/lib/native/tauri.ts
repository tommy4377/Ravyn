/**
 * Typed wrappers around the Ravyn desktop shell commands.
 * All native platform behavior stays behind this module.
 */

import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface BackendInfo {
  base_url: string;
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

export function finishSetupHandoff(): Promise<void> {
  return invoke("finish_setup_handoff");
}

export function mainWindowReady(): Promise<void> {
  return invoke("main_window_ready");
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
