import type { BandwidthSchedule, BandwidthWindow } from "../api/types";

export interface BandwidthWindowDraft {
  weekdays: number[];
  startTime: string;
  endTime: string;
  limitMbps: string;
}

export function minuteToTime(value: number): string {
  const minute = Math.max(0, Math.min(1439, Math.round(value)));
  const hours = Math.floor(minute / 60);
  const minutes = minute % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

export function timeToMinute(value: string, fallback: number): number {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
  if (!match) return fallback;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (!Number.isInteger(hours) || !Number.isInteger(minutes) || hours > 23 || minutes > 59) {
    return fallback;
  }
  return hours * 60 + minutes;
}

export function scheduleToDrafts(schedule: BandwidthSchedule): BandwidthWindowDraft[] {
  return schedule.windows.map((window) => ({
    weekdays: [...window.weekdays].sort((a, b) => a - b),
    startTime: minuteToTime(window.start_minute),
    endTime: minuteToTime(window.end_minute),
    limitMbps: String(Math.round(window.limit_bps / 125000 * 10) / 10),
  }));
}

export function draftsToSchedule(
  timezone: string,
  drafts: BandwidthWindowDraft[],
  fallback: BandwidthSchedule,
): BandwidthSchedule {
  const windows: BandwidthWindow[] = drafts.map((draft, index) => {
    const fallbackWindow = fallback.windows[index];
    return {
      weekdays: [...new Set(draft.weekdays)]
        .filter((weekday) => Number.isInteger(weekday) && weekday >= 1 && weekday <= 7)
        .sort((a, b) => a - b),
      start_minute: timeToMinute(draft.startTime, fallbackWindow?.start_minute ?? 0),
      end_minute: timeToMinute(draft.endTime, fallbackWindow?.end_minute ?? 60),
      limit_bps: Math.round(Math.max(0, Number(draft.limitMbps) || 0) * 125000),
    };
  });
  return {
    timezone: timezone.trim() || fallback.timezone || "UTC",
    windows,
  };
}

export function createBandwidthWindowDraft(): BandwidthWindowDraft {
  return {
    weekdays: [1, 2, 3, 4, 5],
    startTime: "09:00",
    endTime: "17:00",
    limitMbps: "10",
  };
}
