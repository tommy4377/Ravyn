import { describe, expect, it, vi } from "vitest";
import { buildJobMenuItems, type JobRowActions } from "./jobActions";
import { permittedActions } from "./jobPresentation";
import type { Job } from "../api/types";

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    kind: "http",
    source: "https://example.com/file.zip",
    destination: "C:\\Downloads",
    filename: "file.zip",
    status: "downloading",
    priority: 0,
    total_bytes: 1000,
    downloaded_bytes: 100,
    speed_limit_bps: null,
    expected_sha256: null,
    error: null,
    transfer_mode: "segmented",
    options_json: {},
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    started_at: null,
    completed_at: null,
    ...overrides,
  };
}

function makeActions(): JobRowActions {
  return {
    onOpenDetails: vi.fn(),
    onPause: vi.fn(),
    onResume: vi.fn(),
    onRetry: vi.fn(),
    onCancel: vi.fn(),
    onRemove: vi.fn(),
  };
}

describe("buildJobMenuItems", () => {
  it("never labels the delete route with a bare 'Delete' — DELETE /v1/jobs/{id} does not remove files on disk", () => {
    const job = makeJob({ status: "completed" });
    const actions = makeActions();
    const items = buildJobMenuItems(job, permittedActions(job.status, job.kind), actions);
    const removeItem = items.find((item) => item.id === "remove");
    expect(removeItem?.label).toBe("Remove from list");
    expect(removeItem?.danger).toBe(true);
    expect(items.some((item) => item.label === "Delete")).toBe(false);
  });

  it("invokes the matching callback and calls onClose semantics via onSelect", () => {
    const job = makeJob({ status: "failed" });
    const actions = makeActions();
    const items = buildJobMenuItems(job, permittedActions(job.status, job.kind), actions);
    items.find((item) => item.id === "retry")?.onSelect?.();
    expect(actions.onRetry).toHaveBeenCalledWith(job);
  });

  it("only offers actions the current status permits", () => {
    const job = makeJob({ status: "downloading" });
    const actions = makeActions();
    const items = buildJobMenuItems(job, permittedActions(job.status, job.kind), actions);
    const ids = items.map((item) => item.id);
    expect(ids).toContain("pause");
    expect(ids).toContain("cancel");
    expect(ids).not.toContain("resume");
    expect(ids).not.toContain("retry");
    // Downloading is not terminal/paused/seeding, so remove is not offered.
    expect(ids).not.toContain("remove");
  });

  it("always offers 'View details' first regardless of status", () => {
    const job = makeJob({ status: "cancelled" });
    const actions = makeActions();
    const items = buildJobMenuItems(job, permittedActions(job.status, job.kind), actions);
    expect(items[0]?.id).toBe("details");
  });
});
