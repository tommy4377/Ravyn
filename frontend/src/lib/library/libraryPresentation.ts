import type { LibraryEntry } from "../api/types";

export type LibraryMode = "files" | "trash" | "duplicates";
export type LibrarySortKey = "name" | "type" | "size" | "modified" | "source";
export type SortDirection = "asc" | "desc";

export interface DuplicateGroup {
  key: string;
  reason: "checksum" | "name and size";
  entries: LibraryEntry[];
  totalBytes: number;
}

export function libraryTypeLabel(entry: LibraryEntry): string {
  if (entry.mime_type) return entry.mime_type;
  const extension = entry.filename.split(".").at(-1);
  return extension && extension !== entry.filename ? extension.toUpperCase() : entry.category;
}

export function groupLibraryDuplicates(entries: LibraryEntry[]): DuplicateGroup[] {
  const buckets = new Map<string, { reason: DuplicateGroup["reason"]; entries: LibraryEntry[] }>();

  for (const entry of entries) {
    const checksum = entry.sha256?.trim().toLowerCase();
    const key = checksum
      ? `sha256:${checksum}`
      : entry.size_bytes !== null
        ? `fallback:${entry.filename.trim().toLowerCase()}:${entry.size_bytes}`
        : null;
    if (!key) continue;
    const bucket = buckets.get(key) ?? {
      reason: checksum ? "checksum" as const : "name and size" as const,
      entries: [],
    };
    bucket.entries.push(entry);
    buckets.set(key, bucket);
  }

  return [...buckets.entries()]
    .filter(([, bucket]) => bucket.entries.length > 1)
    .map(([key, bucket]) => ({
      key,
      reason: bucket.reason,
      entries: [...bucket.entries].sort((a, b) => a.path.localeCompare(b.path)),
      totalBytes: bucket.entries.reduce((sum, entry) => sum + (entry.size_bytes ?? 0), 0),
    }))
    .sort((a, b) => b.entries.length - a.entries.length || a.entries[0]!.filename.localeCompare(b.entries[0]!.filename));
}

export function sortLibraryEntries(
  entries: LibraryEntry[],
  key: LibrarySortKey,
  direction: SortDirection,
): LibraryEntry[] {
  const factor = direction === "asc" ? 1 : -1;
  return [...entries].sort((a, b) => {
    let result = 0;
    if (key === "name") result = a.filename.localeCompare(b.filename);
    else if (key === "type") result = libraryTypeLabel(a).localeCompare(libraryTypeLabel(b));
    else if (key === "size") result = (a.size_bytes ?? -1) - (b.size_bytes ?? -1);
    else if (key === "modified") result = Date.parse(a.updated_at) - Date.parse(b.updated_at);
    else result = a.source_url.localeCompare(b.source_url);
    return result * factor;
  });
}
