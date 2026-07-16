import { describe, expect, it } from "vitest";
import {
  SetupController,
  restoredSetupStep,
} from "./controller.svelte";

describe("setup restoration", () => {
  it("resumes the install stage after consent and installation were persisted", () => {
    expect(
      restoredSetupStep({
        features_selected: true,
        library_prepared: true,
        integration_consent: {} as never,
        installation: {} as never,
      }),
    ).toBe("install");
  });

  it("returns to preferences before installation consent exists", () => {
    expect(
      restoredSetupStep({
        features_selected: true,
        library_prepared: true,
        integration_consent: null,
        installation: null,
      }),
    ).toBe("preferences");
  });

  it("does not restore a later stage before the library is prepared", () => {
    expect(
      restoredSetupStep({
        features_selected: true,
        library_prepared: false,
        integration_consent: null,
        installation: null,
      }),
    ).toBeNull();
  });

  it("clears unavailable Windows integration choices in development mode", () => {
    const controller = new SetupController();
    controller.installation = {
      app_version: "0.2.0",
      exe_path: "C:\\Ravyn\\ravyn-desktop.exe",
      installed: false,
      installed_version: null,
      install_dir: null,
      portable: true,
      development: true,
      exe_sha256: "00",
    };
    controller.startMenuShortcut = true;
    controller.desktopShortcut = true;
    controller.launchAtStartup = true;

    controller.setApplicationMode("development");

    expect(controller.applicationMode).toBe("development");
    expect(controller.startMenuShortcut).toBe(false);
    expect(controller.desktopShortcut).toBe(false);
    expect(controller.launchAtStartup).toBe(false);
  });
});
