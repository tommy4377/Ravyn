import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, SETTINGS_KEY } from "../../shared/settings";
import { RavynExtensionError } from "../../shared/errors";
import type { NativeClient } from "../native/client";
import type { NativeEvent } from "../../shared/contracts";
import {
  clearTrackedDownloads,
  downloadLabel,
  handleCompletionEvent,
  reconcileCompletions,
  trackBatchResult,
  trackDownload,
} from "./completion";

function statusEvent(
  jobId: string,
  status: string,
  error?: string,
): NativeEvent {
  return {
    type: "event",
    protocolVersion: 2,
    event: "job_status",
    payload: {
      type: "job_status",
      job_id: jobId,
      status,
      error: error ?? null,
    },
  };
}

function stubBrowser(settings: Partial<typeof DEFAULT_SETTINGS> = {}) {
  const stored: Record<string, unknown> = {
    [SETTINGS_KEY]: { ...DEFAULT_SETTINGS, ...settings },
  };
  const notificationsCreate = vi.fn().mockResolvedValue("note-id");
  vi.stubGlobal("browser", {
    storage: {
      local: {
        get: vi.fn().mockImplementation(() => Promise.resolve(stored)),
        set: vi.fn().mockImplementation((patch: Record<string, unknown>) => {
          Object.assign(stored, patch);
          return Promise.resolve();
        }),
      },
    },
    notifications: { create: notificationsCreate },
    runtime: { getURL: (path: string) => `moz-ext://${path}` },
  });
  return { notificationsCreate, stored };
}

describe("completion notifications", () => {
  beforeEach(async () => {
    stubBrowser();
    await clearTrackedDownloads();
  });

  it("notifies exactly once for a tracked completed job", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-1", "report.pdf");
    await handleCompletionEvent(statusEvent("job-1", "completed"));
    await handleCompletionEvent(statusEvent("job-1", "completed"));
    expect(notificationsCreate).toHaveBeenCalledTimes(1);
    expect(notificationsCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Download complete",
        message: "report.pdf",
      }),
    );
  });

  it("includes the error in failed notifications", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-2", "video.mp4");
    await handleCompletionEvent(statusEvent("job-2", "failed", "disk full"));
    expect(notificationsCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Download failed",
        message: "video.mp4 — disk full",
      }),
    );
  });

  it("stays silent for untracked jobs and non-terminal transitions", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-3", "file.zip");
    await handleCompletionEvent(statusEvent("unknown", "completed"));
    await handleCompletionEvent(statusEvent("job-3", "downloading"));
    expect(notificationsCreate).not.toHaveBeenCalled();
  });

  it("settles cancelled jobs silently and permanently", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-4", "file.zip");
    await handleCompletionEvent(statusEvent("job-4", "cancelled"));
    // Even a later terminal event must stay silent — the entry is gone.
    await handleCompletionEvent(statusEvent("job-4", "completed"));
    expect(notificationsCreate).not.toHaveBeenCalled();
  });

  it("respects the notifications setting", async () => {
    const { notificationsCreate } = stubBrowser({ notifications: false });
    await trackDownload("job-5", "file.zip");
    await handleCompletionEvent(statusEvent("job-5", "completed"));
    expect(notificationsCreate).not.toHaveBeenCalled();
  });

  it("reconciles jobs that finished while disconnected", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-6", "iso.img");
    await trackDownload("job-7", "still-going.bin");
    const request = vi
      .fn()
      .mockImplementation((_command, payload: { id: string }) => {
        if (payload.id === "job-6")
          return Promise.resolve({ status: "completed" });
        return Promise.resolve({ status: "downloading" });
      });
    await reconcileCompletions({ request } as unknown as NativeClient);
    expect(notificationsCreate).toHaveBeenCalledTimes(1);
    expect(notificationsCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Download complete",
        message: "iso.img",
      }),
    );
    // The still-running job stays tracked and notifies later.
    await handleCompletionEvent(statusEvent("job-7", "completed"));
    expect(notificationsCreate).toHaveBeenCalledTimes(2);
  });

  it("drops tracking for permanently missing jobs during reconcile", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-8", "gone.bin");
    const request = vi
      .fn()
      .mockRejectedValue(
        new RavynExtensionError("JOB_NOT_FOUND", "no such job", false),
      );
    await reconcileCompletions({ request } as unknown as NativeClient);
    await handleCompletionEvent(statusEvent("job-8", "completed"));
    expect(notificationsCreate).not.toHaveBeenCalled();
  });

  it("keeps tracking when reconcile fails transiently", async () => {
    const { notificationsCreate } = stubBrowser();
    await trackDownload("job-9", "later.bin");
    const request = vi
      .fn()
      .mockRejectedValue(
        new RavynExtensionError("BACKEND_UNAVAILABLE", "offline", true),
      );
    await reconcileCompletions({ request } as unknown as NativeClient);
    await handleCompletionEvent(statusEvent("job-9", "completed"));
    expect(notificationsCreate).toHaveBeenCalledTimes(1);
  });

  it("tracks accepted batch entries by their payload labels", async () => {
    const { notificationsCreate } = stubBrowser();
    trackBatchResult(
      {
        results: [
          { ok: true, job: { id: "job-a" } },
          {
            ok: false,
            error: { code: "X", message: "rejected", retryable: false },
          },
          { ok: true, job: { id: "job-b" } },
        ],
      },
      [
        { url: "https://example.com/a.zip", filename: "a.zip" },
        { url: "https://example.com/b.zip" },
        { url: "https://example.com/c.zip", filename: "c.zip" },
      ],
    );
    await handleCompletionEvent(statusEvent("job-a", "completed"));
    await handleCompletionEvent(statusEvent("job-b", "completed"));
    expect(notificationsCreate).toHaveBeenCalledTimes(2);
    expect(notificationsCreate).toHaveBeenCalledWith(
      expect.objectContaining({ message: "a.zip" }),
    );
    expect(notificationsCreate).toHaveBeenCalledWith(
      expect.objectContaining({ message: "c.zip" }),
    );
  });

  it("labels downloads by filename, hostname, then raw url", () => {
    expect(
      downloadLabel({ url: "https://example.com/x", filename: "x.bin" }),
    ).toBe("x.bin");
    expect(downloadLabel({ url: "https://example.com/x" })).toBe("example.com");
    expect(downloadLabel({ url: "not a url" })).toBe("not a url");
  });
});
