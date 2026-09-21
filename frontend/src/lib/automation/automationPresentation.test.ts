import { describe, expect, it } from "vitest";
import type { AutomationRule, ScheduleRecord } from "../api/types";
import {
  ruleDraftToInput,
  ruleToDraft,
  scheduleDraftToInput,
  scheduleToDraft,
} from "./automationPresentation";

const rule: AutomationRule = {
  id: "rule-1",
  name: "Videos",
  enabled: true,
  priority: 5,
  matcher: {
    domains: ["example.com"],
    extensions: ["mp4", "mkv"],
    mime_types: ["video/mp4"],
    url_regex: null,
  },
  actions: {
    destination: "C:/Videos",
    tags: ["video"],
    speed_limit_bps: 1_250_000,
    post_actions: [],
  },
};

const schedule: ScheduleRecord = {
  id: "schedule-1",
  enabled: true,
  source: "https://example.com/file.zip",
  kind: "http",
  destination: "C:/Downloads",
  mode: "download",
  interval_seconds: null,
  cron_expression: "30 9 * * 2",
  next_run_at: "2026-07-21T07:30:00.000Z",
  timezone_offset_minutes: 120,
  timezone_name: "Europe/Rome",
  overlap_policy: "queue",
  missed_run_policy: "run_once",
  max_catch_up_runs: 1,
  catch_up_runs: 0,
  paused_until: null,
  options: {},
  last_run_at: null,
  failure_count: 0,
  last_error: null,
  created_at: "2026-07-14T00:00:00.000Z",
  updated_at: "2026-07-14T00:00:00.000Z",
};

describe("automation presentation", () => {
  it("round-trips supported rule fields through the visual draft", () => {
    const input = ruleDraftToInput(ruleToDraft(rule));
    expect(input.name).toBe("Videos");
    expect(input.matcher.domains).toEqual(["example.com"]);
    expect(input.matcher.extensions).toEqual(["mp4", "mkv"]);
    expect(input.matcher.mime_types).toEqual(["video/mp4"]);
    expect(input.actions.destination).toBe("C:/Videos");
    expect(input.actions.tags).toEqual(["video"]);
    expect(input.actions.speed_limit_bps).toBe(1_250_000);
  });

  it("maps a weekly cron schedule to readable fields", () => {
    const draft = scheduleToDraft(schedule);
    expect(draft.cadence).toBe("weekly");
    expect(draft.timeOfDay).toBe("09:30");
    expect(draft.weekday).toBe("2");
  });

  it("builds a daily cron expression", () => {
    const draft = scheduleToDraft(null);
    draft.cadence = "daily";
    draft.timeOfDay = "18:45";
    const input = scheduleDraftToInput(draft, null);
    expect(input.cron_expression).toBe("45 18 * * *");
    expect(input.interval_seconds).toBeNull();
  });

  it("builds an interval schedule in seconds", () => {
    const draft = scheduleToDraft(null);
    draft.cadence = "interval";
    draft.intervalValue = "90";
    draft.intervalUnit = "minutes";
    const input = scheduleDraftToInput(draft, null);
    expect(input.interval_seconds).toBe(5_400);
    expect(input.cron_expression).toBeNull();
  });
});
