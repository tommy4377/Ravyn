import type { Job, MediaItemOutputRecord, MediaItemRecord } from "../api/types";

export type MediaDetailTab = "overview" | "items" | "files" | "activity";

export function mediaProgress(job: Job): number {
  if (!job.total_bytes || job.total_bytes <= 0) return job.status === "completed" ? 100 : 0;
  return Math.max(0, Math.min(100, job.downloaded_bytes / job.total_bytes * 100));
}

export function uniqueProducedFiles(records: MediaItemOutputRecord[]): MediaItemOutputRecord[] {
  const byPath = new Map<string, MediaItemOutputRecord>();
  for (const record of records) {
    const key = record.output.current_path.trim().toLowerCase();
    const previous = byPath.get(key);
    if (!previous || Date.parse(previous.created_at) < Date.parse(record.created_at)) byPath.set(key, record);
  }
  return [...byPath.values()].sort((a, b) => a.output.relative_path.localeCompare(b.output.relative_path));
}

export function mediaActivity(items: MediaItemRecord[]): MediaItemRecord[] {
  return [...items].sort((a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at));
}
