/** Shared, locale-aware formatting helpers for lists and detail views. */

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  // Round to 1 decimal below 10 units, whole numbers at or above — and let
  // number-to-string drop a trailing ".0" naturally (toFixed would keep it).
  const rounded = value >= 10 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${rounded} ${BYTE_UNITS[unitIndex]}`;
}

export function formatSpeed(bytesPerSecond: number | null | undefined): string {
  if (!bytesPerSecond || bytesPerSecond <= 0) return "—";
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatEta(
  downloadedBytes: number,
  totalBytes: number | null,
  bytesPerSecond: number | null | undefined,
): string {
  if (!totalBytes || !bytesPerSecond || bytesPerSecond <= 0) return "—";
  const remaining = totalBytes - downloadedBytes;
  if (remaining <= 0) return "—";
  const seconds = remaining / bytesPerSecond;
  return formatDuration(seconds);
}

export function formatDuration(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds) || totalSeconds < 0) return "—";
  const seconds = Math.round(totalSeconds);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${secs}s`;
  return `${secs}s`;
}

export function formatPercent(downloaded: number, total: number | null): string {
  if (!total || total <= 0) return "—";
  return `${Math.min(100, Math.round((downloaded / total) * 100))}%`;
}

const relativeFormatter = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
const DIVISIONS: { amount: number; unit: Intl.RelativeTimeFormatUnit }[] = [
  { amount: 60, unit: "seconds" },
  { amount: 60, unit: "minutes" },
  { amount: 24, unit: "hours" },
  { amount: 7, unit: "days" },
  { amount: 4.34524, unit: "weeks" },
  { amount: 12, unit: "months" },
  { amount: Number.POSITIVE_INFINITY, unit: "years" },
];

export function formatRelativeTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  let duration = (date.getTime() - Date.now()) / 1000;
  for (const division of DIVISIONS) {
    if (Math.abs(duration) < division.amount) {
      return relativeFormatter.format(Math.round(duration), division.unit);
    }
    duration /= division.amount;
  }
  return relativeFormatter.format(Math.round(duration), "years");
}

export function formatAbsoluteTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

/** Best-effort display name for a job: explicit filename, else the URL's last path segment. */
export function jobDisplayName(source: string, filename: string | null): string {
  if (filename) return filename;
  try {
    const url = new URL(source);
    const segments = url.pathname.split("/").filter(Boolean);
    if (segments.length > 0) return decodeURIComponent(segments[segments.length - 1]!);
    return url.hostname;
  } catch {
    return source;
  }
}
