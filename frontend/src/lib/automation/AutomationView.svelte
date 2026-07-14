<script lang="ts">
  import { describeError } from "../api/errors";
  import type { AutomationRule, JobKind, RuleInput, ScheduleExecutionRecord, ScheduleInput, ScheduleRecord } from "../api/types";
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import InlineError from "../components/InlineError.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import Surface from "../components/Surface.svelte";
  import TextField from "../components/TextField.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import { connection } from "../stores/connection.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import { formatAbsoluteTime } from "../util/format";

  type AutomationTab = "rules" | "schedules";
  let tab = $state<AutomationTab>("rules");
  let search = $state("");
  let rules = $state<AutomationRule[]>([]);
  let schedules = $state<ScheduleRecord[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let ruleOpen = $state(false);
  let editingRule = $state<AutomationRule | null>(null);
  let ruleName = $state("");
  let rulePriority = $state("0");
  let ruleEnabled = $state(true);
  let ruleDomains = $state("");
  let ruleExtensions = $state("");
  let ruleDestination = $state("");
  let ruleTags = $state("");
  let ruleBusy = $state(false);

  let scheduleOpen = $state(false);
  let scheduleSource = $state("");
  let scheduleDestination = $state("");
  let scheduleKind = $state("http");
  let scheduleMinutes = $state("60");
  let scheduleEnabled = $state(true);
  let scheduleBusy = $state(false);

  let historySchedule = $state<ScheduleRecord | null>(null);
  let executions = $state<ScheduleExecutionRecord[]>([]);
  let historyLoading = $state(false);
  let historyError = $state<string | null>(null);
  let executionBusy = $state<string | null>(null);

  let deleteKind = $state<"rule" | "schedule" | null>(null);
  let deleteId = $state<string | null>(null);
  let deleteBusy = $state(false);
  let deleteError = $state<string | null>(null);

  const kindOptions: DropdownOption[] = [
    { value: "http", label: "Direct download" },
    { value: "media", label: "Media" },
    { value: "torrent", label: "Torrent" },
  ];

  const visibleRules = $derived(search.trim() ? rules.filter((rule) => `${rule.name} ${rule.matcher.domains.join(" ")} ${rule.matcher.extensions.join(" ")}`.toLowerCase().includes(search.toLowerCase())) : rules);
  const visibleSchedules = $derived(search.trim() ? schedules.filter((schedule) => `${schedule.source} ${schedule.destination}`.toLowerCase().includes(search.toLowerCase())) : schedules);

  async function load(): Promise<void> {
    if (!connection.client) return;
    loading = true;
    error = null;
    try {
      const [rulePage, schedulePage] = await Promise.all([
        connection.client.listRules({ limit: 250 }),
        connection.client.listSchedules({ limit: 250 }),
      ]);
      rules = rulePage.items;
      schedules = schedulePage.items;
    } catch (cause) {
      error = describeError(cause);
    } finally {
      loading = false;
    }
  }

  $effect(() => { void load(); });

  function csv(value: string): string[] {
    return value.split(",").map((part) => part.trim()).filter(Boolean);
  }

  function openRule(rule: AutomationRule | null = null): void {
    editingRule = rule;
    ruleName = rule?.name ?? "";
    rulePriority = String(rule?.priority ?? 0);
    ruleEnabled = rule?.enabled ?? true;
    ruleDomains = rule?.matcher.domains.join(", ") ?? "";
    ruleExtensions = rule?.matcher.extensions.join(", ") ?? "";
    ruleDestination = rule?.actions.destination ?? "";
    ruleTags = rule?.actions.tags.join(", ") ?? "";
    ruleOpen = true;
  }

  function ruleInput(): RuleInput {
    return {
      name: ruleName.trim(),
      enabled: ruleEnabled,
      priority: Number(rulePriority) || 0,
      matcher: { domains: csv(ruleDomains), extensions: csv(ruleExtensions), mime_types: [], url_regex: null },
      actions: { destination: ruleDestination.trim() || null, tags: csv(ruleTags), speed_limit_bps: null, post_actions: [] },
    };
  }

  async function saveRule(): Promise<void> {
    if (!connection.client || !ruleName.trim()) return;
    ruleBusy = true;
    try {
      if (editingRule) await connection.client.updateRule(editingRule.id, ruleInput());
      else await connection.client.createRule(ruleInput());
      notifications.success(editingRule ? "Rule updated" : "Rule created");
      ruleOpen = false;
      await load();
    } catch (cause) {
      notifications.error("Couldn't save the rule", describeError(cause));
    } finally {
      ruleBusy = false;
    }
  }

  async function toggleRule(rule: AutomationRule): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.updateRule(rule.id, { name: rule.name, enabled: !rule.enabled, priority: rule.priority, matcher: rule.matcher, actions: rule.actions });
      await load();
    } catch (cause) {
      notifications.error("Couldn't update the rule", describeError(cause));
    }
  }

  async function createSchedule(): Promise<void> {
    if (!connection.client || !scheduleSource.trim() || !scheduleDestination.trim()) return;
    scheduleBusy = true;
    try {
      const minutes = Math.max(1, Number(scheduleMinutes) || 60);
      const input: ScheduleInput = {
        enabled: scheduleEnabled,
        source: scheduleSource.trim(),
        kind: scheduleKind as JobKind,
        destination: scheduleDestination.trim(),
        mode: "download",
        automation: null,
        interval_seconds: minutes * 60,
        cron_expression: null,
        next_run_at: null,
        timezone_offset_minutes: -new Date().getTimezoneOffset(),
        timezone_name: Intl.DateTimeFormat().resolvedOptions().timeZone || null,
        overlap_policy: "queue",
        missed_run_policy: "run_once",
        max_catch_up_runs: 1,
        paused_until: null,
        options: {},
      };
      await connection.client.createSchedule(input);
      notifications.success("Schedule created");
      scheduleOpen = false;
      scheduleSource = "";
      await load();
    } catch (cause) {
      notifications.error("Couldn't create the schedule", describeError(cause));
    } finally {
      scheduleBusy = false;
    }
  }

  async function toggleSchedule(schedule: ScheduleRecord): Promise<void> {
    if (!connection.client) return;
    try {
      await connection.client.setScheduleEnabled(schedule.id, !schedule.enabled);
      await load();
    } catch (cause) {
      notifications.error("Couldn't update the schedule", describeError(cause));
    }
  }

  async function runNow(schedule: ScheduleRecord): Promise<void> {
    if (!connection.client) return;
    try {
      const execution = await connection.client.runScheduleNow(schedule.id);
      notifications.success("Schedule started");
      if (historySchedule?.id === schedule.id) executions = [execution, ...executions.filter((item) => item.id !== execution.id)];
      await load();
    } catch (cause) {
      notifications.error("Couldn't run the schedule", describeError(cause));
    }
  }


  async function openHistory(schedule: ScheduleRecord): Promise<void> {
    if (!connection.client) return;
    historySchedule = schedule;
    historyLoading = true;
    historyError = null;
    executions = [];
    try {
      const page = await connection.client.listScheduleExecutions(schedule.id, { limit: 100 });
      executions = page.items;
    } catch (cause) {
      historyError = describeError(cause);
    } finally {
      historyLoading = false;
    }
  }

  async function cancelExecution(execution: ScheduleExecutionRecord): Promise<void> {
    if (!connection.client || executionBusy) return;
    executionBusy = execution.id;
    try {
      const updated = await connection.client.cancelScheduleExecution(execution.id);
      executions = executions.map((item) => item.id === updated.id ? updated : item);
      notifications.info("Scheduled execution cancelled");
    } catch (cause) {
      notifications.error("Couldn't cancel the execution", describeError(cause));
    } finally {
      executionBusy = null;
    }
  }

  function executionSeverity(state: string): "neutral" | "info" | "success" | "warning" | "error" {
    if (state === "completed" || state === "succeeded") return "success";
    if (state === "failed") return "error";
    if (state === "cancelled" || state === "skipped") return "warning";
    if (state === "running" || state === "claimed") return "info";
    return "neutral";
  }

  function executionCanCancel(state: string): boolean {
    return ["queued", "claimed", "running"].includes(state);
  }

  function requestDelete(kind: "rule" | "schedule", id: string): void {
    deleteKind = kind;
    deleteId = id;
    deleteError = null;
  }

  async function confirmDelete(): Promise<void> {
    if (!connection.client || !deleteKind || !deleteId) return;
    deleteBusy = true;
    deleteError = null;
    try {
      if (deleteKind === "rule") await connection.client.deleteRule(deleteId);
      else await connection.client.deleteSchedule(deleteId);
      notifications.info(deleteKind === "rule" ? "Rule deleted" : "Schedule deleted");
      deleteKind = null;
      deleteId = null;
      await load();
    } catch (cause) {
      deleteError = describeError(cause);
    } finally {
      deleteBusy = false;
    }
  }
</script>

<div class="page">
  <PageHeader title="Automation" description="Organize incoming downloads with rules and run recurring tasks on a schedule.">
    {#snippet actions()}
      <Button variant="accent" onclick={() => tab === "rules" ? openRule() : (scheduleOpen = true)}><Icon name="add" size={16} /> {tab === "rules" ? "New rule" : "New schedule"}</Button>
    {/snippet}
  </PageHeader>

  <div class="controls">
    <div class="segments" role="tablist" aria-label="Automation section">
      <button type="button" role="tab" aria-selected={tab === "rules"} onclick={() => (tab = "rules")}><Icon name="rule" size={16} /> Rules <span>{rules.length}</span></button>
      <button type="button" role="tab" aria-selected={tab === "schedules"} onclick={() => (tab = "schedules")}><Icon name="calendar" size={16} /> Schedules <span>{schedules.length}</span></button>
    </div>
    <SearchBox bind:value={search} label="Search automation" placeholder={`Search ${tab}`} />
    <IconButton icon="refresh" label="Refresh automation" onclick={() => void load()} />
  </div>

  <div class="content">
    <Surface padding="none" class="automation-surface">
      {#if error}
        <div class="state"><InlineError title="Couldn't load automation" message={error} retry={() => void load()} /></div>
      {:else if loading}
        <div class="state muted">Loading automation…</div>
      {:else if tab === "rules"}
        {#if visibleRules.length === 0}
          <EmptyState icon="rule" title="No rules" message={search ? "No rules match the current search." : "Rules can select a destination, add tags, and apply options based on a URL or file type."}><Button variant="accent" onclick={() => openRule()}>Create a rule</Button></EmptyState>
        {:else}
          <div class="item-list">
            {#each visibleRules as rule (rule.id)}
              <article class="automation-item">
                <span class="item-icon"><Icon name="rule" size={19} /></span>
                <div class="item-copy"><div><strong>{rule.name}</strong><StatusBadge label={rule.enabled ? "Enabled" : "Disabled"} severity={rule.enabled ? "success" : "neutral"} /></div><span>{rule.matcher.domains.length ? `Domains: ${rule.matcher.domains.join(", ")}` : "Any domain"} · {rule.matcher.extensions.length ? `Extensions: ${rule.matcher.extensions.join(", ")}` : "Any extension"}</span><small>Priority {rule.priority}{rule.actions.destination ? ` · ${rule.actions.destination}` : ""}</small></div>
                <div class="item-actions"><Button variant="subtle" onclick={() => void toggleRule(rule)}>{rule.enabled ? "Disable" : "Enable"}</Button><IconButton icon="edit" label="Edit rule" variant="subtle" onclick={() => openRule(rule)} /><IconButton icon="trash" label="Delete rule" variant="subtle" onclick={() => requestDelete("rule", rule.id)} /></div>
              </article>
            {/each}
          </div>
        {/if}
      {:else}
        {#if visibleSchedules.length === 0}
          <EmptyState icon="calendar" title="No schedules" message={search ? "No schedules match the current search." : "Create a schedule to start a download automatically at a recurring interval."}><Button variant="accent" onclick={() => (scheduleOpen = true)}>Create a schedule</Button></EmptyState>
        {:else}
          <div class="item-list">
            {#each visibleSchedules as schedule (schedule.id)}
              <article class="automation-item">
                <span class="item-icon"><Icon name="calendar" size={19} /></span>
                <div class="item-copy"><div><strong>{schedule.source}</strong><StatusBadge label={schedule.enabled ? "Enabled" : "Disabled"} severity={schedule.enabled ? "success" : "neutral"} /></div><span>{schedule.kind} · {schedule.destination}</span><small>Next run {formatAbsoluteTime(schedule.next_run_at)}{schedule.interval_seconds ? ` · every ${Math.round(schedule.interval_seconds / 60)} min` : schedule.cron_expression ? ` · ${schedule.cron_expression}` : ""}</small>{#if schedule.last_error}<small class="last-error">{schedule.last_error}</small>{/if}</div>
                <div class="item-actions"><Button variant="subtle" onclick={() => void openHistory(schedule)}><Icon name="clock" size={14} /> History</Button><Button variant="subtle" onclick={() => void runNow(schedule)}><Icon name="play" size={14} /> Run now</Button><Button variant="subtle" onclick={() => void toggleSchedule(schedule)}>{schedule.enabled ? "Disable" : "Enable"}</Button><IconButton icon="trash" label="Delete schedule" variant="subtle" onclick={() => requestDelete("schedule", schedule.id)} /></div>
              </article>
            {/each}
          </div>
        {/if}
      {/if}
    </Surface>
  </div>
</div>

<Dialog open={ruleOpen} title={editingRule ? "Edit rule" : "New rule"} onClose={() => !ruleBusy && (ruleOpen = false)} preventClose={ruleBusy}>
  <div class="form">
    <TextField bind:value={ruleName} label="Name" placeholder="Organize video downloads" />
    <div class="two-column"><TextField bind:value={ruleDomains} label="Domains" placeholder="youtube.com, example.com" /><TextField bind:value={ruleExtensions} label="Extensions" placeholder="mp4, mkv" /></div>
    <PathPicker bind:value={ruleDestination} label="Destination" placeholder="Leave empty to keep the current destination" />
    <div class="two-column"><TextField bind:value={ruleTags} label="Tags" placeholder="video, archive" /><TextField bind:value={rulePriority} label="Priority" placeholder="0" /></div>
    <ToggleSwitch bind:checked={ruleEnabled} label="Enabled" description="Disabled rules are kept but do not affect new downloads." />
  </div>
  {#snippet footer()}<Button disabled={ruleBusy} onclick={() => (ruleOpen = false)}>Cancel</Button><Button variant="accent" disabled={ruleBusy || !ruleName.trim()} onclick={() => void saveRule()}>{ruleBusy ? "Saving…" : "Save rule"}</Button>{/snippet}
</Dialog>

<Dialog open={scheduleOpen} title="New schedule" onClose={() => !scheduleBusy && (scheduleOpen = false)} preventClose={scheduleBusy}>
  <div class="form">
    <TextField bind:value={scheduleSource} label="Source URL" placeholder="https://example.com/file.zip" />
    <PathPicker bind:value={scheduleDestination} label="Destination" placeholder="Choose a download folder" />
    <div class="two-column"><div class="field"><span>Download type</span><Dropdown options={kindOptions} bind:value={scheduleKind} label="Download type" /></div><TextField bind:value={scheduleMinutes} label="Repeat every (minutes)" placeholder="60" /></div>
    <ToggleSwitch bind:checked={scheduleEnabled} label="Enable immediately" description="The schedule can be disabled later without deleting it." />
  </div>
  {#snippet footer()}<Button disabled={scheduleBusy} onclick={() => (scheduleOpen = false)}>Cancel</Button><Button variant="accent" disabled={scheduleBusy || !scheduleSource.trim() || !scheduleDestination.trim()} onclick={() => void createSchedule()}>{scheduleBusy ? "Creating…" : "Create schedule"}</Button>{/snippet}
</Dialog>

<Dialog open={!!historySchedule} title="Schedule history" size="large" onClose={() => (historySchedule = null)}>
  {#if historySchedule}
    <div class="history-heading"><span class="item-icon"><Icon name="calendar" size={18} /></span><div><strong>{historySchedule.source}</strong><span>{historySchedule.destination}</span></div><Button disabled={historyLoading} onclick={() => { if (historySchedule) void openHistory(historySchedule); }}><Icon name="refresh" size={15} /> Refresh</Button></div>
  {/if}
  {#if historyError}
    <InlineError title="Couldn't load execution history" message={historyError} retry={() => historySchedule && void openHistory(historySchedule)} />
  {:else if historyLoading}
    <p class="muted history-state">Loading execution history…</p>
  {:else if executions.length === 0}
    <EmptyState icon="clock" title="No executions yet" message="Run this schedule now or wait for its next planned time." />
  {:else}
    <div class="execution-list">
      {#each executions as execution (execution.id)}
        <article class="execution-row">
          <span class="execution-dot" class:active={executionCanCancel(execution.state)}></span>
          <div><strong>{formatAbsoluteTime(execution.intended_run_at)}</strong><span>Started {formatAbsoluteTime(execution.started_at)}{execution.completed_at ? ` · completed ${formatAbsoluteTime(execution.completed_at)}` : ""}</span>{#if execution.error}<small>{execution.error}</small>{/if}</div>
          <StatusBadge label={execution.state} severity={executionSeverity(execution.state)} />
          {#if executionCanCancel(execution.state)}<Button variant="subtle" disabled={!!executionBusy} onclick={() => void cancelExecution(execution)}>{executionBusy === execution.id ? "Cancelling…" : "Cancel"}</Button>{/if}
        </article>
      {/each}
    </div>
  {/if}
  {#snippet footer()}<Button variant="accent" onclick={() => (historySchedule = null)}>Done</Button>{/snippet}
</Dialog>

<ConfirmDialog open={!!deleteKind} title={`Delete ${deleteKind ?? "item"}?`} message={`This ${deleteKind ?? "item"} will be removed permanently.`} confirmLabel="Delete" destructive busy={deleteBusy} error={deleteError} onConfirm={() => void confirmDelete()} onClose={() => !deleteBusy && (deleteKind = null)} />

<style>
  .page { height: 100%; display: flex; flex-direction: column; }
  .controls { display: flex; align-items: center; gap: var(--space-2); padding: 0 var(--page-padding) var(--space-4); }
  .controls :global(.search-box) { margin-left: auto; }
  .segments { display: inline-flex; padding: 2px; border: 1px solid var(--stroke-control); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .segments button { display: flex; align-items: center; gap: var(--space-2); height: 30px; padding: 0 var(--space-3); border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; cursor: default; }
  .segments button[aria-selected="true"] { color: var(--text-primary); background: var(--surface-card-hover); box-shadow: 0 1px 2px rgba(0,0,0,.08); }
  .segments button span { color: var(--text-tertiary); font-size: var(--text-caption); }
  .content { flex: 1; min-height: 0; padding: 0 var(--page-padding) var(--page-padding); }
  :global(.automation-surface) { height: 100%; display: flex; flex-direction: column; }
  .state { padding: var(--space-6); } .muted { color: var(--text-secondary); }
  .item-list { min-height: 0; overflow: auto; }
  .automation-item { display: grid; grid-template-columns: 38px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); min-height: 78px; padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .automation-item:hover { background: var(--bg-subtle-hover); }
  .item-icon { display: grid; place-items: center; width: 34px; height: 34px; border-radius: var(--radius-medium); color: var(--accent-text); background: var(--accent-subtle); }
  .item-copy { display: flex; flex-direction: column; min-width: 0; }
  .item-copy > div { display: flex; align-items: center; gap: var(--space-2); }
  .item-copy strong, .item-copy span, .item-copy small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item-copy strong { font-weight: 500; }
  .item-copy span, .item-copy small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .item-copy .last-error { color: var(--status-error); }
  .item-actions { display: flex; align-items: center; gap: var(--space-1); }
  .history-heading { display: grid; grid-template-columns: 38px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); margin-bottom: var(--space-4); }
  .history-heading > div { display: flex; min-width: 0; flex-direction: column; }
  .history-heading strong, .history-heading span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .history-heading span { color: var(--text-secondary); font-size: var(--text-caption); }
  .history-state { padding: var(--space-5) 0; }
  .execution-list { display: flex; flex-direction: column; max-height: 470px; overflow: auto; border-top: 1px solid var(--stroke-divider); }
  .execution-row { display: grid; grid-template-columns: 12px minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); min-height: 64px; padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .execution-row > div { display: flex; min-width: 0; flex-direction: column; }
  .execution-row span, .execution-row small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .execution-row small { color: var(--status-error); }
  .execution-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--stroke-control-strong); }
  .execution-dot.active { background: var(--accent-default); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .form { display: flex; flex-direction: column; gap: var(--space-4); }
  .two-column { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-3); }
  .field { display: flex; flex-direction: column; gap: var(--space-1); }
  @media (max-width: 850px) { .controls { align-items: stretch; flex-wrap: wrap; } .controls :global(.search-box) { order: 3; width: 100%; margin-left: 0; } .automation-item { grid-template-columns: 38px minmax(0, 1fr); } .item-actions { grid-column: 2; justify-content: flex-start; } }
  @media (max-width: 600px) { .two-column { grid-template-columns: 1fr; } }
</style>
