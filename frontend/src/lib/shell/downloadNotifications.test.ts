import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false }));
vi.mock("../native/tauri", () => ({ notifyNative: vi.fn().mockResolvedValue(undefined) }));

import type { Job, JobStatusEvent } from "../api/types";
import { jobsStore } from "../stores/jobs.svelte";
import { notifications } from "../stores/notifications.svelte";
import { notifyDownloadEvent } from "./downloadNotifications";

function terminalEvent(jobId: string, status: JobStatusEvent["status"], error: string | null = null): JobStatusEvent {
  return { sequence: 1, type: "job_status", job_id: jobId, status, error };
}

function seedJob(id: string): void {
  jobsStore.upsert({
    id,
    source: "https://example.com/file.zip",
    filename: "file.zip",
    status: "downloading",
  } as Job);
}

describe("notifyDownloadEvent", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("notifies a known job once and dedupes replayed events", () => {
    const success = vi.spyOn(notifications, "success").mockReturnValue("id");
    seedJob("job-n1");
    notifyDownloadEvent(terminalEvent("job-n1", "completed"));
    notifyDownloadEvent(terminalEvent("job-n1", "completed"));
    expect(success).toHaveBeenCalledTimes(1);
    expect(success).toHaveBeenCalledWith("Download complete", "file.zip");
  });

  it("still notifies jobs that are not in the loaded page", () => {
    const success = vi.spyOn(notifications, "success").mockReturnValue("id");
    notifyDownloadEvent(terminalEvent("job-n2", "completed"));
    expect(success).toHaveBeenCalledTimes(1);
    expect(success).toHaveBeenCalledWith("Download complete", undefined);
  });

  it("does not depend on the jobs store applying the event first", () => {
    const success = vi.spyOn(notifications, "success").mockReturnValue("id");
    seedJob("job-n3");
    // Simulate a subscriber ordering where the store already applied the
    // terminal status before the notifier ran.
    jobsStore.applyEvent(terminalEvent("job-n3", "completed"));
    notifyDownloadEvent(terminalEvent("job-n3", "completed"));
    expect(success).toHaveBeenCalledTimes(1);
  });

  it("includes the error detail for failures", () => {
    const error = vi.spyOn(notifications, "error").mockReturnValue("id");
    seedJob("job-n4");
    notifyDownloadEvent(terminalEvent("job-n4", "failed", "disk full"));
    expect(error).toHaveBeenCalledWith("Download failed", "file.zip — disk full");
  });

  it("ignores non-terminal transitions", () => {
    const success = vi.spyOn(notifications, "success").mockReturnValue("id");
    const warning = vi.spyOn(notifications, "warning").mockReturnValue("id");
    notifyDownloadEvent(terminalEvent("job-n5", "downloading"));
    notifyDownloadEvent(terminalEvent("job-n5", "paused"));
    expect(success).not.toHaveBeenCalled();
    expect(warning).not.toHaveBeenCalled();
  });
});
