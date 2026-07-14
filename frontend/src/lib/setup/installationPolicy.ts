import type { InstallationMode } from "../api/types";
import type {
  InstallationInfo,
  IntegrationRequest,
} from "../native/tauri";

export interface InstallationPreferences {
  startMenuShortcut: boolean;
  desktopShortcut: boolean;
  launchAtStartup: boolean;
}

/**
 * Return whether a Windows installer already owns the running executable.
 * In that case setup must not copy the binary or replace the installer's
 * uninstall registration, but it may still apply user-level preferences.
 */
export function isInstallerManaged(installation: InstallationInfo): boolean {
  return (
    installation.installed &&
    !installation.portable &&
    !installation.development
  );
}

/** Build the native integration request for the selected application mode. */
export function buildIntegrationRequest(
  mode: InstallationMode,
  installation: InstallationInfo,
  preferences: InstallationPreferences,
): IntegrationRequest | null {
  if (mode !== "installed") return null;

  const installerManaged = isInstallerManaged(installation);
  return {
    install_application: !installerManaged,
    register_installed_app: !installerManaged,
    // NSIS/MSI already creates the Start Menu entry it owns.
    start_menu_shortcut: installerManaged
      ? false
      : preferences.startMenuShortcut,
    desktop_shortcut: preferences.desktopShortcut,
    launch_at_startup: preferences.launchAtStartup,
  };
}
