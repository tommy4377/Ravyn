import { describe, expect, it } from "vitest";
import type { InstallationInfo } from "../native/tauri";
import {
  buildIntegrationRequest,
  isInstallerManaged,
} from "./installationPolicy";

function installation(
  overrides: Partial<InstallationInfo> = {},
): InstallationInfo {
  return {
    app_version: "0.2.0",
    exe_path: String.raw`C:\Users\Test\Downloads\Ravyn.exe`,
    installed: false,
    installed_version: null,
    install_dir: null,
    portable: true,
    development: false,
    exe_sha256: "a".repeat(64),
    ...overrides,
  };
}

const preferences = {
  startMenuShortcut: true,
  desktopShortcut: true,
  launchAtStartup: true,
};

describe("installation policy", () => {
  it("recognizes an installer-managed executable", () => {
    expect(
      isInstallerManaged(
        installation({
          installed: true,
          portable: false,
          install_dir: String.raw`C:\Users\Test\AppData\Local\Ravyn`,
        }),
      ),
    ).toBe(true);
  });

  it("does not treat a development build as installer managed", () => {
    expect(
      isInstallerManaged(
        installation({ installed: true, portable: false, development: true }),
      ),
    ).toBe(false);
  });

  it("preserves the installer-owned binary and uninstall registration", () => {
    const request = buildIntegrationRequest(
      "installed",
      installation({ installed: true, portable: false }),
      preferences,
    );

    expect(request).toEqual({
      install_application: false,
      register_installed_app: false,
      start_menu_shortcut: false,
      desktop_shortcut: true,
      launch_at_startup: true,
    });
  });

  it("self-installs a downloaded portable executable", () => {
    const request = buildIntegrationRequest(
      "installed",
      installation(),
      preferences,
    );

    expect(request).toEqual({
      install_application: true,
      register_installed_app: true,
      start_menu_shortcut: true,
      desktop_shortcut: true,
      launch_at_startup: true,
    });
  });

  it("does not request native installation for portable mode", () => {
    expect(
      buildIntegrationRequest("portable", installation(), preferences),
    ).toBeNull();
  });
});
