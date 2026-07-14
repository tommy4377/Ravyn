import { describe, expect, it } from "vitest";
import type { LibraryMoveStatus } from "../api/types";
import {
  isLibraryMoveRunning,
  libraryMoveDescription,
  libraryMoveProgress,
  libraryMoveTitle,
} from "./libraryMovePresentation";

function status(overrides: Partial<LibraryMoveStatus> = {}): LibraryMoveStatus {
  return {
    run_id: "00000000-0000-4000-8000-000000000001",
    state: "running",
    source_root: "C:\\Old",
    destination_root: "D:\\New",
    conflict_policy: "fail",
    total_files: 4,
    total_bytes: 1_000,
    copied_files: 2,
    copied_bytes: 500,
    verified_files: 2,
    reused_files: 0,
    missing_files: 0,
    external_entries: 0,
    conflict_files: 0,
    cancel_requested: false,
    restart_required: false,
    error: null,
    started_at: null,
    updated_at: null,
    completed_at: null,
    ...overrides,
  };
}

describe("library move presentation", () => {
  it("uses copied bytes for progress when byte totals are available", () => {
    expect(libraryMoveProgress(status())).toBe(50);
  });

  it("falls back to verified files and clamps progress", () => {
    expect(libraryMoveProgress(status({ total_bytes: 0, copied_bytes: 0, verified_files: 8 }))).toBe(100);
    expect(libraryMoveProgress(status({ total_bytes: 0, total_files: 0 }))).toBeNull();
  });

  it("treats running and cancelling as active states", () => {
    expect(isLibraryMoveRunning(status())).toBe(true);
    expect(isLibraryMoveRunning(status({ state: "cancelling" }))).toBe(true);
    expect(isLibraryMoveRunning(status({ state: "restart_required" }))).toBe(false);
  });

  it("prioritizes backend errors over generic state copy", () => {
    const failed = status({ state: "failed", error: "Destination is unavailable" });
    expect(libraryMoveTitle(failed)).toBe("Move stopped");
    expect(libraryMoveDescription(failed)).toBe("Destination is unavailable");
  });
});
