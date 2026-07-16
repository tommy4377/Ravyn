/**
 * Raises in-app and native Windows notifications for terminal download
 * transitions observed on the live event stream. Must run before the jobs
 * store applies the event so the previous status is still available.
 */

import { isTauri } from "@tauri-apps/api/core";
import type { JobStatusEvent, RavynEvent } from "../api/types";
import { notifyNative } from "../native/tauri";
import { jobsStore } from "../stores/jobs.svelte";
import { notifications } from "../stores/notifications.svelte";
import { jobDisplayName } from "../util/format";

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
  const job = jobsStore.byId.get(statusEvent.job_id);
  // Unknown jobs (not loaded yet) and repeated terminal events stay silent.
  if (!job || job.status === statusEvent.status) return;

  const name = jobDisplayName(job.source, job.filename);
  let title: string;
  if (statusEvent.status === "completed") {
    title = "Download complete";
    notifications.success(title, name);
  } else if (statusEvent.status === "partial") {
    title = "Download partially completed";
    notifications.warning(title, name);
  } else {
    title = "Download failed";
    notifications.error(title, statusEvent.error ? `${name} — ${statusEvent.error}` : name);
  }

  const windowFocused = typeof document !== "undefined" && document.hasFocus();
  if (isTauri() && !windowFocused) {
    void notifyNative(title, name).catch(() => undefined);
  }
}
