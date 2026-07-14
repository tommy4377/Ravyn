import { describe, expect, it } from "vitest";
import type { AppUpdateStatus } from "../native/tauri";
import {
  appUpdateDescription,
  appUpdateHeading,
  canCancelAppUpdate,
  canInstallAppUpdateNow,
} from "./appUpdatePresentation";

function status(overrides: Partial<AppUpdateStatus> = {}): AppUpdateStatus {
  return {
    configured: true,
    automatic: true,
    phase: "idle",
    current_version: "0.2.0",
    available_version: null,
    downloaded_bytes: 0,
    total_bytes: null,
    notes: null,
    last_error: null,
    install_on_exit: false,
    repair_mode: false,
    last_result: null,
    last_checked_at_unix_ms: null,
    next_check_at_unix_ms: null,
    automatic_check_interval_secs: 21600,
    ...overrides,
  };
}

describe("application update presentation", () => {
  it("describes cooperative cancellation without treating it as an error", () => {
    const cancelling = status({ phase: "cancelling" });
    expect(appUpdateHeading(cancelling)).toBe("Stopping update download…");
    expect(appUpdateDescription(cancelling)).toContain("removing partial update files");
  });

  it("enables immediate installation only for a verified staged installer", () => {
    expect(canInstallAppUpdateNow(status({ phase: "ready", install_on_exit: true }))).toBe(true);
    expect(canInstallAppUpdateNow(status({ phase: "downloading", install_on_exit: false }))).toBe(false);
  });

  it("allows cancellation for checks, downloads, and staged updates", () => {
    for (const phase of ["checking", "downloading", "ready"] as const) {
      expect(canCancelAppUpdate(status({ phase }))).toBe(true);
    }
    expect(canCancelAppUpdate(status({ phase: "cancelling" }))).toBe(false);
  });
});
