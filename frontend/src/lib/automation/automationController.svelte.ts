import { describeError } from "../api/errors";
import type {
  AutomationRule,
  RulePreview,
  ScheduleExecutionRecord,
  ScheduleRecord,
} from "../api/types";
import { connection } from "../stores/connection.svelte";
import { notifications } from "../stores/notifications.svelte";
import {
  createRuleAction,
  createRuleCondition,
  ruleDraftToInput,
  ruleToDraft,
  scheduleDraftToInput,
  scheduleToDraft,
  type RuleActionKind,
  type RuleConditionKind,
  type RuleDraft,
  type ScheduleDraft,
} from "./automationPresentation";

export type AutomationTab = "rules" | "schedules";
export type DeleteKind = "rule" | "schedule";

export class AutomationController {
  tab = $state<AutomationTab>("rules");
  search = $state("");
  rules = $state<AutomationRule[]>([]);
  schedules = $state<ScheduleRecord[]>([]);
  loading = $state(true);
  error = $state<string | null>(null);

  ruleOpen = $state(false);
  editingRule = $state<AutomationRule | null>(null);
  ruleDraft = $state<RuleDraft>(ruleToDraft(null));
  ruleBusy = $state(false);

  scheduleOpen = $state(false);
  editingSchedule = $state<ScheduleRecord | null>(null);
  scheduleDraft = $state<ScheduleDraft>(scheduleToDraft(null));
  scheduleBusy = $state(false);

  historySchedule = $state<ScheduleRecord | null>(null);
  executions = $state<ScheduleExecutionRecord[]>([]);
  selectedExecution = $state<ScheduleExecutionRecord | null>(null);
  historyLoading = $state(false);
  historyError = $state<string | null>(null);
  executionBusy = $state<string | null>(null);

  previewOpen = $state(false);
  previewUrl = $state("");
  previewExtension = $state("");
  previewMime = $state("");
  previewBusy = $state(false);
  previewResult = $state<RulePreview | null>(null);
  previewError = $state<string | null>(null);

  deleteKind = $state<DeleteKind | null>(null);
  deleteId = $state<string | null>(null);
  deleteBusy = $state(false);
  deleteError = $state<string | null>(null);

  get visibleRules(): AutomationRule[] {
    const query = this.search.trim().toLowerCase();
    if (!query) return this.rules;
    return this.rules.filter((rule) =>
      `${rule.name} ${rule.matcher.domains.join(" ")} ${rule.matcher.extensions.join(" ")} ${rule.matcher.mime_types.join(" ")}`
        .toLowerCase()
        .includes(query),
    );
  }

  get visibleSchedules(): ScheduleRecord[] {
    const query = this.search.trim().toLowerCase();
    if (!query) return this.schedules;
    return this.schedules.filter((schedule) =>
      `${schedule.source} ${schedule.destination} ${schedule.kind}`.toLowerCase().includes(query),
    );
  }

  async load(): Promise<void> {
    if (!connection.client) return;
    this.loading = true;
    this.error = null;
    try {
      const [rulePage, schedulePage] = await Promise.all([
        connection.client.listRules({ limit: 250 }),
        connection.client.listSchedules({ limit: 250 }),
      ]);
      this.rules = rulePage.items;
      this.schedules = schedulePage.items;
    } catch (cause) {
      this.error = describeError(cause);
    } finally {
      this.loading = false;
    }
  }

  openRule(rule: AutomationRule | null = null): void {
    this.editingRule = rule;
    this.ruleDraft = ruleToDraft(rule);
    this.ruleOpen = true;
  }

  closeRule(): void {
    if (this.ruleBusy) return;
    this.ruleOpen = false;
    this.editingRule = null;
  }

  addCondition(kind: RuleConditionKind = "domain"): void {
    this.ruleDraft.conditions.push(createRuleCondition(kind));
  }

  removeCondition(id: string): void {
    if (this.ruleDraft.conditions.length === 1) {
      const onlyCondition = this.ruleDraft.conditions[0];
      if (onlyCondition) onlyCondition.value = "";
      return;
    }
    this.ruleDraft.conditions = this.ruleDraft.conditions.filter((condition) => condition.id !== id);
  }

  addAction(kind: RuleActionKind = "destination"): void {
    this.ruleDraft.actions.push(createRuleAction(kind));
  }

  removeAction(id: string): void {
    if (this.ruleDraft.actions.length === 1) {
      const onlyAction = this.ruleDraft.actions[0];
      if (onlyAction) onlyAction.value = "";
      return;
    }
    this.ruleDraft.actions = this.ruleDraft.actions.filter((action) => action.id !== id);
  }

  async saveRule(): Promise<void> {
    if (!connection.client || !this.ruleDraft.name.trim() || this.ruleBusy) return;
    this.ruleBusy = true;
    try {
      const input = ruleDraftToInput(this.ruleDraft);
      if (this.editingRule) await connection.client.updateRule(this.editingRule.id, input);
      else await connection.client.createRule(input);
      notifications.success(this.editingRule ? "Rule updated" : "Rule created");
      this.ruleOpen = false;
      this.editingRule = null;
      await this.load();
    } catch (cause) {
      notifications.error("Couldn't save the rule", describeError(cause));
    } finally {
      this.ruleBusy = false;
    }
  }

  async toggleRule(rule: AutomationRule): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.updateRule(rule.id, { ...rule, enabled: !rule.enabled });
      await this.load();
    } catch (cause) {
      notifications.error("Couldn't update the rule", describeError(cause));
    }
  }

  openSchedule(schedule: ScheduleRecord | null = null): void {
    this.editingSchedule = schedule;
    this.scheduleDraft = scheduleToDraft(schedule);
    this.scheduleOpen = true;
  }

  closeSchedule(): void {
    if (this.scheduleBusy) return;
    this.scheduleOpen = false;
    this.editingSchedule = null;
  }

  async saveSchedule(): Promise<void> {
    if (!connection.client || !this.scheduleDraft.source.trim() || !this.scheduleDraft.destination.trim() || this.scheduleBusy) return;
    this.scheduleBusy = true;
    try {
      const input = scheduleDraftToInput(this.scheduleDraft, this.editingSchedule);
      if (this.editingSchedule) await connection.client.updateSchedule(this.editingSchedule.id, input);
      else await connection.client.createSchedule(input);
      notifications.success(this.editingSchedule ? "Schedule updated" : "Schedule created");
      this.scheduleOpen = false;
      this.editingSchedule = null;
      await this.load();
    } catch (cause) {
      notifications.error(this.editingSchedule ? "Couldn't update the schedule" : "Couldn't create the schedule", describeError(cause));
    } finally {
      this.scheduleBusy = false;
    }
  }

  async toggleSchedule(schedule: ScheduleRecord): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.setScheduleEnabled(schedule.id, !schedule.enabled);
      await this.load();
    } catch (cause) {
      notifications.error("Couldn't update the schedule", describeError(cause));
    }
  }

  async runNow(schedule: ScheduleRecord): Promise<void> {
    if (!connection.client) return;
    try {
      const execution = await connection.client.runScheduleNow(schedule.id);
      notifications.success("Schedule started");
      if (this.historySchedule?.id === schedule.id) {
        this.executions = [execution, ...this.executions.filter((item) => item.id !== execution.id)];
      }
      await this.load();
    } catch (cause) {
      notifications.error("Couldn't run the schedule", describeError(cause));
    }
  }

  async openHistory(schedule: ScheduleRecord): Promise<void> {
    if (!connection.client) return;
    this.historySchedule = schedule;
    this.selectedExecution = null;
    this.historyLoading = true;
    this.historyError = null;
    this.executions = [];
    try {
      const page = await connection.client.listScheduleExecutions(schedule.id, { limit: 100 });
      this.executions = page.items;
    } catch (cause) {
      this.historyError = describeError(cause);
    } finally {
      this.historyLoading = false;
    }
  }

  closeHistory(): void {
    this.historySchedule = null;
    this.selectedExecution = null;
  }

  async refreshSelectedExecution(): Promise<void> {
    if (!connection.client || !this.selectedExecution) return;
    try {
      const updated = await connection.client.getScheduleExecution(this.selectedExecution.id);
      this.selectedExecution = updated;
      this.executions = this.executions.map((item) => item.id === updated.id ? updated : item);
    } catch (cause) {
      notifications.error("Couldn't refresh the execution", describeError(cause));
    }
  }

  async cancelExecution(execution: ScheduleExecutionRecord): Promise<void> {
    if (!connection.client || this.executionBusy) return;
    this.executionBusy = execution.id;
    try {
      const updated = await connection.client.cancelScheduleExecution(execution.id);
      this.executions = this.executions.map((item) => item.id === updated.id ? updated : item);
      if (this.selectedExecution?.id === updated.id) this.selectedExecution = updated;
      notifications.info("Scheduled execution cancelled");
    } catch (cause) {
      notifications.error("Couldn't cancel the execution", describeError(cause));
    } finally {
      this.executionBusy = null;
    }
  }

  openRulePreview(): void {
    this.previewResult = null;
    this.previewError = null;
    this.previewOpen = true;
  }

  async runRulePreview(): Promise<void> {
    if (!connection.client || !this.previewUrl.trim() || this.previewBusy) return;
    this.previewBusy = true;
    this.previewError = null;
    try {
      this.previewResult = await connection.client.previewRules({
        request: { kind: "http", source: this.previewUrl.trim() },
        mime: this.previewMime.trim() || null,
        extension: this.previewExtension.trim().replace(/^\./, "") || null,
      });
    } catch (cause) {
      this.previewError = describeError(cause);
      this.previewResult = null;
    } finally {
      this.previewBusy = false;
    }
  }

  requestDelete(kind: DeleteKind, id: string): void {
    this.deleteKind = kind;
    this.deleteId = id;
    this.deleteError = null;
  }

  async confirmDelete(): Promise<void> {
    if (!connection.client || !this.deleteKind || !this.deleteId) return;
    this.deleteBusy = true;
    this.deleteError = null;
    try {
      if (this.deleteKind === "rule") await connection.client.deleteRule(this.deleteId);
      else await connection.client.deleteSchedule(this.deleteId);
      notifications.info(this.deleteKind === "rule" ? "Rule deleted" : "Schedule deleted");
      this.deleteKind = null;
      this.deleteId = null;
      await this.load();
    } catch (cause) {
      this.deleteError = describeError(cause);
    } finally {
      this.deleteBusy = false;
    }
  }
}

export function executionSeverity(state: string): "neutral" | "info" | "success" | "warning" | "error" {
  if (state === "completed" || state === "succeeded") return "success";
  if (state === "failed") return "error";
  if (state === "cancelled" || state === "skipped") return "warning";
  if (state === "running" || state === "claimed") return "info";
  return "neutral";
}

export function executionCanCancel(state: string): boolean {
  return ["queued", "claimed", "running"].includes(state);
}
