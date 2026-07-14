<script lang="ts">
  import type { ScheduleRecord } from "../api/types";
  import Button from "../components/Button.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";
  import { formatAbsoluteTime } from "../util/format";
  import { detectScheduleCadence } from "./automationPresentation";

  let {
    schedules,
    search = "",
    onCreate,
    onEdit,
    onToggle,
    onRun,
    onHistory,
    onDelete,
  }: {
    schedules: ScheduleRecord[];
    search?: string;
    onCreate: () => void;
    onEdit: (schedule: ScheduleRecord) => void;
    onToggle: (schedule: ScheduleRecord) => void;
    onRun: (schedule: ScheduleRecord) => void;
    onHistory: (schedule: ScheduleRecord) => void;
    onDelete: (schedule: ScheduleRecord) => void;
  } = $props();

  function cadenceLabel(schedule: ScheduleRecord): string {
    const cadence = detectScheduleCadence(schedule);
    if (cadence === "interval" && schedule.interval_seconds) {
      const minutes = Math.round(schedule.interval_seconds / 60);
      return minutes % 60 === 0 ? `Every ${minutes / 60} hour${minutes === 60 ? "" : "s"}` : `Every ${minutes} minutes`;
    }
    if (cadence === "daily") return "Daily";
    if (cadence === "weekly") return "Weekly";
    if (cadence === "advanced") return "Advanced schedule";
    return "Once";
  }
</script>

{#if schedules.length === 0}
  <EmptyState
    icon="calendar"
    title={search ? "No matching schedules" : "No schedules yet"}
    message={search ? "Try another search." : "Create a schedule to start a download automatically."}
  >
    {#if !search}<Button variant="accent" onclick={onCreate}>Create a schedule</Button>{/if}
  </EmptyState>
{:else}
  <div class="list" role="list">
    {#each schedules as schedule (schedule.id)}
      <article class="row" role="listitem" class:disabled={!schedule.enabled}>
        <span class="row-icon"><Icon name="calendar" size={18} /></span>
        <div class="copy">
          <div class="title-line">
            <strong>{schedule.source}</strong>
            <StatusBadge label={schedule.enabled ? "Enabled" : "Disabled"} severity={schedule.enabled ? "success" : "neutral"} />
          </div>
          <span>{cadenceLabel(schedule)} · Next run {formatAbsoluteTime(schedule.next_run_at)}</span>
          <small>{schedule.kind} · {schedule.destination}</small>
          {#if schedule.last_error}<small class="error">{schedule.last_error}</small>{/if}
        </div>
        <Button variant="subtle" onclick={() => onRun(schedule)}><Icon name="play" size={14} /> Run now</Button>
        <MenuButton
          label={`Actions for ${schedule.source}`}
          icon="more"
          iconOnly
          variant="subtle"
          items={[
            { id: "history", label: "Execution history", icon: "clock", onSelect: () => onHistory(schedule) },
            { id: "edit", label: "Edit", icon: "edit", onSelect: () => onEdit(schedule) },
            { id: "toggle", label: schedule.enabled ? "Disable" : "Enable", icon: schedule.enabled ? "pause" : "play", onSelect: () => onToggle(schedule) },
            { id: "delete", label: "Delete", icon: "trash", danger: true, separatorBefore: true, onSelect: () => onDelete(schedule) },
          ]}
        />
      </article>
    {/each}
  </div>
{/if}

<style>
  .list { height: 100%; min-height: 0; overflow: auto; }
  .row { min-height: 84px; display: grid; grid-template-columns: 34px minmax(0, 1fr) auto auto; align-items: center; gap: var(--space-3); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .row:hover { background: var(--bg-subtle-hover); }
  .row.disabled { opacity: .72; }
  .row-icon { width: 32px; height: 32px; display: grid; place-items: center; color: var(--text-secondary); }
  .copy { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .title-line { min-width: 0; display: flex; align-items: center; gap: var(--space-2); }
  strong, span, small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  strong { font-weight: 600; }
  .copy > span, small { color: var(--text-tertiary); font-size: var(--text-caption); }
  small.error { color: var(--status-error); }
  @media (max-width: 760px) { .row { grid-template-columns: 34px minmax(0, 1fr) auto; } .row > :global(button:not(.menu-trigger)) { display: none; } }
</style>
