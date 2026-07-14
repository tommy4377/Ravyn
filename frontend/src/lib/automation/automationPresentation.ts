import type {
  AutomationRule,
  JobKind,
  RuleActions,
  RuleInput,
  RuleMatcher,
  ScheduleInput,
  ScheduleRecord,
} from "../api/types";

export type RuleConditionKind = "domain" | "extension" | "mime" | "url_pattern";
export type RuleActionKind = "destination" | "tags" | "speed_limit";

export interface RuleConditionDraft {
  id: string;
  kind: RuleConditionKind;
  value: string;
}

export interface RuleActionDraft {
  id: string;
  kind: RuleActionKind;
  value: string;
}

export interface RuleDraft {
  name: string;
  enabled: boolean;
  priority: string;
  conditions: RuleConditionDraft[];
  actions: RuleActionDraft[];
}

export type ScheduleCadence = "once" | "interval" | "daily" | "weekly" | "advanced";
export type IntervalUnit = "minutes" | "hours";

export interface ScheduleDraft {
  enabled: boolean;
  source: string;
  destination: string;
  kind: JobKind;
  cadence: ScheduleCadence;
  intervalValue: string;
  intervalUnit: IntervalUnit;
  onceAt: string;
  timeOfDay: string;
  weekday: string;
  cronExpression: string;
}

let draftId = 0;

function nextId(prefix: string): string {
  draftId += 1;
  return `${prefix}-${draftId}`;
}

export function createRuleCondition(kind: RuleConditionKind = "domain", value = ""): RuleConditionDraft {
  return { id: nextId("condition"), kind, value };
}

export function createRuleAction(kind: RuleActionKind = "destination", value = ""): RuleActionDraft {
  return { id: nextId("action"), kind, value };
}

function splitValues(value: string): string[] {
  return value
    .split(/[,\n]/)
    .map((part) => part.trim())
    .filter(Boolean);
}

export function ruleToDraft(rule: AutomationRule | null): RuleDraft {
  if (!rule) {
    return {
      name: "",
      enabled: true,
      priority: "0",
      conditions: [createRuleCondition("domain")],
      actions: [createRuleAction("destination")],
    };
  }

  const conditions: RuleConditionDraft[] = [];
  if (rule.matcher.domains.length) conditions.push(createRuleCondition("domain", rule.matcher.domains.join(", ")));
  if (rule.matcher.extensions.length) conditions.push(createRuleCondition("extension", rule.matcher.extensions.join(", ")));
  if (rule.matcher.mime_types.length) conditions.push(createRuleCondition("mime", rule.matcher.mime_types.join(", ")));
  if (rule.matcher.url_regex) conditions.push(createRuleCondition("url_pattern", rule.matcher.url_regex));
  if (!conditions.length) conditions.push(createRuleCondition("domain"));

  const actions: RuleActionDraft[] = [];
  if (rule.actions.destination) actions.push(createRuleAction("destination", rule.actions.destination));
  if (rule.actions.tags.length) actions.push(createRuleAction("tags", rule.actions.tags.join(", ")));
  if (rule.actions.speed_limit_bps) {
    actions.push(createRuleAction("speed_limit", String(Math.round(rule.actions.speed_limit_bps / 125000 * 10) / 10)));
  }
  if (!actions.length) actions.push(createRuleAction("destination"));

  return {
    name: rule.name,
    enabled: rule.enabled,
    priority: String(rule.priority),
    conditions,
    actions,
  };
}

export function ruleDraftToInput(draft: RuleDraft): RuleInput {
  const matcher: RuleMatcher = {
    domains: [],
    extensions: [],
    mime_types: [],
    url_regex: null,
  };
  const actions: RuleActions = {
    destination: null,
    tags: [],
    speed_limit_bps: null,
    post_actions: [],
  };

  for (const condition of draft.conditions) {
    if (!condition.value.trim()) continue;
    if (condition.kind === "domain") matcher.domains.push(...splitValues(condition.value));
    if (condition.kind === "extension") {
      matcher.extensions.push(...splitValues(condition.value).map((value) => value.replace(/^\./, "")));
    }
    if (condition.kind === "mime") matcher.mime_types.push(...splitValues(condition.value));
    if (condition.kind === "url_pattern") matcher.url_regex = condition.value.trim();
  }

  for (const action of draft.actions) {
    if (!action.value.trim()) continue;
    if (action.kind === "destination") actions.destination = action.value.trim();
    if (action.kind === "tags") actions.tags.push(...splitValues(action.value));
    if (action.kind === "speed_limit") {
      const megabits = Number(action.value);
      actions.speed_limit_bps = Number.isFinite(megabits) && megabits > 0 ? Math.round(megabits * 125000) : null;
    }
  }

  return {
    name: draft.name.trim(),
    enabled: draft.enabled,
    priority: Math.round(Number(draft.priority) || 0),
    matcher,
    actions,
  };
}

function localDateTimeValue(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}T${hours}:${minutes}`;
}

function cronTimeParts(cron: string | null): { minute: string; hour: string; weekday: string } | null {
  if (!cron) return null;
  const parts = cron.trim().split(/\s+/);
  if (parts.length !== 5) return null;
  const minute = parts[0];
  const hour = parts[1];
  const dayOfMonth = parts[2];
  const month = parts[3];
  const weekday = parts[4];
  if (!minute || !hour || !dayOfMonth || !month || !weekday) return null;
  if (!/^\d+$/.test(minute) || !/^\d+$/.test(hour) || dayOfMonth !== "*" || month !== "*") return null;
  return { minute, hour, weekday };
}

export function detectScheduleCadence(schedule: ScheduleRecord): ScheduleCadence {
  if (schedule.cron_expression) {
    const parts = cronTimeParts(schedule.cron_expression);
    if (parts?.weekday === "*") return "daily";
    if (parts && /^\d$/.test(parts.weekday)) return "weekly";
    return "advanced";
  }
  if (schedule.interval_seconds) return "interval";
  return "once";
}

export function scheduleToDraft(schedule: ScheduleRecord | null): ScheduleDraft {
  const now = new Date(Date.now() + 60 * 60 * 1000);
  now.setSeconds(0, 0);
  if (!schedule) {
    return {
      enabled: true,
      source: "",
      destination: "",
      kind: "http",
      cadence: "interval",
      intervalValue: "1",
      intervalUnit: "hours",
      onceAt: localDateTimeValue(now),
      timeOfDay: "09:00",
      weekday: "1",
      cronExpression: "",
    };
  }

  const cadence = detectScheduleCadence(schedule);
  const cronParts = cronTimeParts(schedule.cron_expression);
  const intervalSeconds = schedule.interval_seconds ?? 3600;
  const useHours = intervalSeconds % 3600 === 0;

  return {
    enabled: schedule.enabled,
    source: schedule.source,
    destination: schedule.destination,
    kind: schedule.kind,
    cadence,
    intervalValue: String(useHours ? intervalSeconds / 3600 : Math.max(1, Math.round(intervalSeconds / 60))),
    intervalUnit: useHours ? "hours" : "minutes",
    onceAt: schedule.next_run_at ? localDateTimeValue(new Date(schedule.next_run_at)) : localDateTimeValue(now),
    timeOfDay: cronParts ? `${cronParts.hour.padStart(2, "0")}:${cronParts.minute.padStart(2, "0")}` : "09:00",
    weekday: cronParts?.weekday && cronParts.weekday !== "*" ? cronParts.weekday : "1",
    cronExpression: cadence === "advanced" ? schedule.cron_expression ?? "" : "",
  };
}

function timeToCron(value: string): { hour: number; minute: number } {
  const [rawHour, rawMinute] = value.split(":");
  const hour = Math.min(23, Math.max(0, Number(rawHour) || 0));
  const minute = Math.min(59, Math.max(0, Number(rawMinute) || 0));
  return { hour, minute };
}

export function scheduleDraftToInput(draft: ScheduleDraft, existing: ScheduleRecord | null): ScheduleInput {
  let intervalSeconds: number | null = null;
  let cronExpression: string | null = null;
  let nextRunAt: string | null = null;

  if (draft.cadence === "interval") {
    const value = Math.max(1, Number(draft.intervalValue) || 1);
    intervalSeconds = Math.round(value * (draft.intervalUnit === "hours" ? 3600 : 60));
  } else if (draft.cadence === "once") {
    const date = new Date(draft.onceAt);
    nextRunAt = Number.isNaN(date.getTime()) ? null : date.toISOString();
  } else if (draft.cadence === "daily") {
    const { hour, minute } = timeToCron(draft.timeOfDay);
    cronExpression = `${minute} ${hour} * * *`;
  } else if (draft.cadence === "weekly") {
    const { hour, minute } = timeToCron(draft.timeOfDay);
    const weekday = Math.min(6, Math.max(0, Number(draft.weekday) || 0));
    cronExpression = `${minute} ${hour} * * ${weekday}`;
  } else {
    cronExpression = draft.cronExpression.trim() || null;
  }

  return {
    enabled: draft.enabled,
    source: draft.source.trim(),
    kind: draft.kind,
    destination: draft.destination.trim(),
    mode: existing?.mode ?? "download",
    automation: null,
    interval_seconds: intervalSeconds,
    cron_expression: cronExpression,
    next_run_at: nextRunAt,
    timezone_offset_minutes: existing?.timezone_offset_minutes ?? -new Date().getTimezoneOffset(),
    timezone_name: existing?.timezone_name ?? (Intl.DateTimeFormat().resolvedOptions().timeZone || null),
    overlap_policy: existing?.overlap_policy ?? "queue",
    missed_run_policy: existing?.missed_run_policy ?? "run_once",
    max_catch_up_runs: existing?.max_catch_up_runs ?? 1,
    paused_until: existing?.paused_until ?? null,
    options: existing?.options ?? {},
  };
}

export function describeRuleCondition(condition: RuleConditionDraft): string {
  if (condition.kind === "domain") return "Domain is or contains";
  if (condition.kind === "extension") return "File extension is";
  if (condition.kind === "mime") return "MIME type is";
  return "URL matches pattern";
}

export function describeRuleAction(action: RuleActionDraft): string {
  if (action.kind === "destination") return "Set destination";
  if (action.kind === "tags") return "Add tags";
  return "Set speed limit";
}
