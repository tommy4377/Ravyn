import { toExtensionError } from "../../shared/errors";
import { logger } from "../../shared/logger";
import { loadSettings } from "../../shared/settings";
import { notify } from "../notifications";
import type { NativeClient } from "../native/client";
import type { NativeEvent } from "../../shared/contracts";

/**
 * Completion notifications for downloads this browser delegated to Ravyn.
 *
 * Jobs are tracked in extension storage at handoff time and settled exactly
 * once: the tracked entry is removed atomically before notifying, so a
 * replayed or duplicated `job_status` event can never notify twice. Because
 * the native event stream is replay-aware, while `reconcileCompletions` remains
 * the safety net for reconnects that fall outside the backend replay window or
 * for browser event-page suspension.
 */

const STORAGE_KEY = "ravyn.trackedDownloads";
const MAX_TRACKED = 500;
const MAX_AGE_MS = 24 * 60 * 60 * 1_000;

interface TrackedDownload {
  label: string;
  trackedAt: number;
}

type TrackedMap = Record<string, TrackedDownload>;

const NOTIFICATION_TITLES: Record<string, string> = {
  completed: "Download complete",
  partial: "Download partially completed",
  failed: "Download failed",
};

/** Statuses after which a job can never notify again. */
const SETTLED_STATUSES = new Set([
  "completed",
  "partial",
  "failed",
  "cancelled",
]);

// storage.session survives MV3 event-page suspensions but not browser
// restarts — a completion can only be announced while the session that
// started the download is still alive anyway.
function storageArea(): browser.storage.StorageArea {
  const storage = browser.storage as typeof browser.storage & {
    session?: browser.storage.StorageArea;
  };
  return storage.session ?? storage.local;
}

// Serializes every read-modify-write of the tracked map, mirroring the
// interceptor's pending-id queue: concurrent handoffs or events would
// otherwise interleave get/set and drop entries.
let trackedQueue: Promise<unknown> = Promise.resolve();

function withTracked<T>(
  mutate: (map: TrackedMap) => T | Promise<T>,
): Promise<T> {
  const run = trackedQueue.then(async () => {
    const stored = await storageArea().get(STORAGE_KEY);
    const map = (stored[STORAGE_KEY] as TrackedMap | undefined) ?? {};
    const result = await mutate(map);
    await storageArea().set({ [STORAGE_KEY]: map });
    return result;
  });
  trackedQueue = run.catch(() => undefined);
  return run;
}

/** Human-readable notification label for a download payload. */
export function downloadLabel(payload: {
  url: string;
  filename?: string;
}): string {
  if (payload.filename) return payload.filename;
  try {
    return new URL(payload.url).hostname || payload.url;
  } catch {
    return payload.url;
  }
}

/** Records a delegated Ravyn job so its terminal transition raises a notification. */
export async function trackDownload(
  jobId: unknown,
  label: string,
): Promise<void> {
  if (typeof jobId !== "string" || !jobId) return;
  await withTracked((map) => {
    prune(map);
    map[jobId] = { label, trackedAt: Date.now() };
  }).catch((error) =>
    logger.warn(
      "Failed to track a download for completion notifications",
      error,
    ),
  );
}

/** Tracks every accepted download of a `create_batch` native result. */
export function trackBatchResult(
  result: unknown,
  downloads: Array<{ url: string; filename?: string }>,
): void {
  const results =
    result && typeof result === "object"
      ? (result as { results?: unknown }).results
      : undefined;
  if (!Array.isArray(results)) return;
  results.forEach((entry, index) => {
    if (!entry || typeof entry !== "object") return;
    const record = entry as {
      ok?: unknown;
      jobId?: unknown;
      job?: { id?: unknown };
    };
    if (record.ok !== true) return;
    const download = downloads[index];
    void trackDownload(
      record.jobId ?? record.job?.id,
      download ? downloadLabel(download) : "Download",
    );
  });
}

/** Settles tracked jobs from the proxied backend event stream. */
export async function handleCompletionEvent(event: NativeEvent): Promise<void> {
  if (event.event !== "job_status") return;
  const payload =
    event.payload && typeof event.payload === "object"
      ? (event.payload as {
          job_id?: unknown;
          status?: unknown;
          error?: unknown;
        })
      : undefined;
  const jobId =
    typeof payload?.job_id === "string" ? payload.job_id : undefined;
  const status =
    typeof payload?.status === "string" ? payload.status : undefined;
  if (!jobId || !status) return;
  await settleJob(
    jobId,
    status,
    typeof payload?.error === "string" ? payload.error : undefined,
  );
}

/**
 * Re-checks every outstanding tracked job against the backend. Call when the
 * backend connection is established, covering any gap older than the replay
 * buffer and keeping completion delivery eventually consistent.
 */
export async function reconcileCompletions(
  native: NativeClient,
): Promise<void> {
  const ids = await withTracked((map) => Object.keys(map)).catch(
    () => [] as string[],
  );
  for (const jobId of ids) {
    try {
      const job = await native.request<{
        status?: string;
        error?: string | null;
      }>("get_job", {
        id: jobId,
      });
      if (typeof job?.status === "string") {
        await settleJob(jobId, job.status, job.error ?? undefined);
      }
    } catch (error) {
      // A permanent failure (job deleted, request rejected) has nothing left
      // to announce; a retryable one is retried on the next reconnect.
      if (!toExtensionError(error).retryable) {
        await withTracked((map) => {
          delete map[jobId];
        }).catch(() => undefined);
      }
    }
  }
}

/** Drops all tracked jobs; used when the user clears extension data. */
export async function clearTrackedDownloads(): Promise<void> {
  await withTracked((map) => {
    for (const key of Object.keys(map)) delete map[key];
  }).catch(() => undefined);
}

async function settleJob(
  jobId: string,
  status: string,
  error?: string,
): Promise<void> {
  if (!SETTLED_STATUSES.has(status)) return;
  // Remove-then-notify: the entry can be taken exactly once, so duplicate
  // events for the same terminal transition stay silent.
  const entry = await withTracked((map) => {
    const tracked = map[jobId];
    delete map[jobId];
    return tracked;
  }).catch(() => undefined);
  if (!entry) return;
  const title = NOTIFICATION_TITLES[status];
  if (!title) return; // cancelled settles silently
  const settings = await loadSettings().catch(() => undefined);
  if (!settings?.notifications) return;
  const message =
    status === "failed" && error ? `${entry.label} — ${error}` : entry.label;
  await notify(title, message).catch(() => undefined);
}

function prune(map: TrackedMap): void {
  const cutoff = Date.now() - MAX_AGE_MS;
  for (const [jobId, entry] of Object.entries(map)) {
    if (entry.trackedAt < cutoff) delete map[jobId];
  }
  const entries = Object.entries(map);
  if (entries.length < MAX_TRACKED) return;
  entries
    .sort(([, left], [, right]) => left.trackedAt - right.trackedAt)
    .slice(0, entries.length - MAX_TRACKED + 1)
    .forEach(([jobId]) => delete map[jobId]);
}
