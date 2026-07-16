import { describe, expect, it } from "vitest";
import {
  createBandwidthWindowDraft,
  draftsToSchedule,
  minuteToTime,
  scheduleToDrafts,
  timeToMinute,
} from "./bandwidthSchedule";

describe("bandwidth schedule helpers", () => {
  it("converts minutes and local time values", () => {
    expect(minuteToTime(0)).toBe("00:00");
    expect(minuteToTime(1439)).toBe("23:59");
    expect(timeToMinute("09:45", 0)).toBe(585);
    expect(timeToMinute("25:00", 120)).toBe(120);
  });

  it("round-trips schedule windows without changing limits", () => {
    const schedule = {
      timezone: "Europe/Rome",
      windows: [{ weekdays: [5, 1, 3], start_minute: 480, end_minute: 1020, limit_bps: 2_500_000 }],
    };
    const drafts = scheduleToDrafts(schedule);
    expect(drafts[0]).toEqual({ weekdays: [1, 3, 5], startTime: "08:00", endTime: "17:00", limitMbps: "20" });
    expect(draftsToSchedule("Europe/Rome", drafts, schedule)).toEqual({
      timezone: "Europe/Rome",
      windows: [{ weekdays: [1, 3, 5], start_minute: 480, end_minute: 1020, limit_bps: 2_500_000 }],
    });
  });

  it("creates a useful weekday default", () => {
    expect(createBandwidthWindowDraft()).toEqual({
      weekdays: [1, 2, 3, 4, 5],
      startTime: "09:00",
      endTime: "17:00",
      limitMbps: "10",
    });
  });
});
