import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { JobsStore } from "./jobs.svelte";
import type { Job, JobPage } from "../api/types";
import type { JobsService } from "../services/jobs";

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
    downloaded_bytes: 0,
    speed_limit_bps: null,
    expected_sha256: null,
    error: null,
    transfer_mode: "segmented",
    options_json: {},
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    started_at: null,
    completed_at: null,
    ...overrides,
  };
}

function makeService(page: JobPage = { items: [], next_cursor: null }): JobsService {
  return {
    list: vi.fn().mockResolvedValue(page),
    get: vi.fn(),
  } as unknown as JobsService;
}

describe("JobsStore", () => {
  let store: JobsStore;

  beforeEach(() => {
    store = new JobsStore();
  });

  afterEach(() => {
    store.dispose();
  });

  it("upsert adds a new job to the front of the order and keyed collection", () => {
    store.upsert(makeJob({ id: "a" }));
    store.upsert(makeJob({ id: "b" }));
    expect(store.list.map((job) => job.id)).toEqual(["b", "a"]);
  });

  it("upsert replaces an existing job in place without duplicating it", () => {
    store.upsert(makeJob({ id: "a", status: "downloading" }));
    store.upsert(makeJob({ id: "a", status: "completed" }));
    expect(store.list).toHaveLength(1);
    expect(store.list[0]?.status).toBe("completed");
  });

  it("removeLocal drops the job and its live progress", () => {
    store.upsert(makeJob({ id: "a" }));
    store.liveProgress.set("a", { downloadedBytes: 1, totalBytes: 2, bytesPerSecond: 3, updatedAt: Date.now() });
    store.removeLocal("a");
    expect(store.list).toHaveLength(0);
    expect(store.liveProgress.has("a")).toBe(false);
  });

  it("applyEvent(job_status) patches status/error on a known job", () => {
    store.init(makeService());
    store.upsert(makeJob({ id: "a", status: "downloading", error: null }));
    store.applyEvent({ sequence: 1, type: "job_status", job_id: "a", status: "failed", error: "disk full" });
    expect(store.byId.get("a")?.status).toBe("failed");
    expect(store.byId.get("a")?.error).toBe("disk full");
  });

  it("applyEvent(job_status) for an unknown job triggers a refetch instead of throwing", async () => {
    const service = makeService();
    service.get = vi.fn().mockResolvedValue(makeJob({ id: "unknown", status: "queued" }));
    store.init(service);
    store.applyEvent({ sequence: 1, type: "job_status", job_id: "unknown", status: "queued", error: null });
    await vi.waitFor(() => expect(store.byId.has("unknown")).toBe(true));
  });

  it("coalesces progress events to a single flush instead of updating per event", () => {
    vi.useFakeTimers();
    try {
      store.init(makeService());
      store.upsert(makeJob({ id: "a" }));
      store.applyEvent({ sequence: 1, type: "progress", job_id: "a", downloaded_bytes: 10, total_bytes: 1000, bytes_per_second: 5 });
      store.applyEvent({ sequence: 2, type: "progress", job_id: "a", downloaded_bytes: 20, total_bytes: 1000, bytes_per_second: 8 });
      // Not applied yet — coalescing waits for the shared flush tick.
      expect(store.liveProgress.has("a")).toBe(false);
      vi.advanceTimersByTime(100);
      expect(store.liveProgress.get("a")?.downloadedBytes).toBe(20);
      expect(store.liveProgress.get("a")?.bytesPerSecond).toBe(8);
    } finally {
      vi.useRealTimers();
    }
  });

  it("applyEvent(queue_changed / resync_required) re-runs the last query", () => {
    const service = makeService();
    store.init(service);
    store.applyEvent({ sequence: 1, type: "queue_changed" });
    expect(service.list).toHaveBeenCalled();
    (service.list as ReturnType<typeof vi.fn>).mockClear();
    store.applyEvent({ sequence: 2, type: "resync_required", oldest_available: 1, newest_available: 5 });
    expect(service.list).toHaveBeenCalled();
  });

  describe("jobsFor", () => {
    beforeEach(() => {
      store.upsert(makeJob({ id: "queued", status: "queued" }));
      store.upsert(makeJob({ id: "downloading", status: "downloading" }));
      store.upsert(makeJob({ id: "paused", status: "paused" }));
      store.upsert(makeJob({ id: "completed", status: "completed" }));
      store.upsert(makeJob({ id: "partial", status: "partial" }));
      store.upsert(makeJob({ id: "failed", status: "failed" }));
      store.upsert(makeJob({ id: "cancelled", status: "cancelled" }));
    });

    it("'all' returns every loaded job", () => {
      expect(store.jobsFor("all")).toHaveLength(7);
    });

    it("'active' groups the non-terminal, non-queued statuses", () => {
      const ids = store.jobsFor("active").map((job) => job.id).sort();
      expect(ids).toEqual(["downloading", "paused"]);
    });

    it("'queued' is only the queued status", () => {
      expect(store.jobsFor("queued").map((job) => job.id)).toEqual(["queued"]);
    });

    it("'completed' groups completed and partial", () => {
      expect(store.jobsFor("completed").map((job) => job.id).sort()).toEqual(["completed", "partial"]);
    });

    it("'failed' groups failed and cancelled", () => {
      expect(store.jobsFor("failed").map((job) => job.id).sort()).toEqual(["cancelled", "failed"]);
    });
  });
});
