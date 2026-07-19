/**
 * Raises in-app and native Windows notifications for terminal download
 * transitions observed on the live event stream. Deduplication is keyed on
 * the (job, status) pair instead of comparing against the jobs store, so a
 * replayed event after an SSE reconnect stays silent, jobs outside the
 * currently loaded page still notify, and the result no longer depends on
 * subscriber ordering.
 */

import { isTauri } from "@tauri-apps/api/core";
import type { JobStatusEvent, RavynEvent } from "../api/types";
import { notifyNative } from "../native/tauri";
import { jobsStore } from "../stores/jobs.svelte";
import { notifications } from "../stores/notifications.svelte";
import { jobDisplayName } from "../util/format";

const MAX_NOTIFIED = 500;
const notified = new Set<string>();

function markNotified(key: string): boolean {
  if (notified.has(key)) return false;
  notified.add(key);
  if (notified.size > MAX_NOTIFIED) {
    const oldest = notified.values().next().value;
    if (oldest !== undefined) notified.delete(oldest);
  }
  return true;
}

export function notifyDownloadEvent(event: RavynEvent): void {
  if (event.type !== "job_status") return;
  const statusEvent = event as JobStatusEvent;
  if (
    statusEvent.status !== "completed" &&
    statusEvent.status !== "failed" &&
    statusEvent.status !== "partial"
  ) {
    return;
  }
  if (!markNotified(`${statusEvent.job_id}:${statusEvent.status}`)) return;

  // A job outside the loaded page has no display name yet — the store
  // refetches it right after this event, but the notification should not
  // wait on that round trip.
  const job = jobsStore.byId.get(statusEvent.job_id);
  const name = job ? jobDisplayName(job.source, job.filename) : undefined;
  let title: string;
  if (statusEvent.status === "completed") {
    title = "Download complete";
    notifications.success(title, name);
  } else if (statusEvent.status === "partial") {
    title = "Download partially completed";
    notifications.warning(title, name);
  } else {
    title = "Download failed";
    const detail =
      name && statusEvent.error
        ? `${name} — ${statusEvent.error}`
        : (name ?? statusEvent.error ?? undefined);
    notifications.error(title, detail);
  }

  const windowFocused = typeof document !== "undefined" && document.hasFocus();
  if (isTauri() && !windowFocused) {
    void notifyNative(title, name).catch(() => undefined);
  }
}
