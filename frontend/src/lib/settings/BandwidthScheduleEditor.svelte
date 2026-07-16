<script lang="ts">
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import TextField from "../components/TextField.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();

  const weekdays = [
    { value: 1, label: "M" },
    { value: 2, label: "T" },
    { value: 3, label: "W" },
    { value: 4, label: "T" },
    { value: 5, label: "F" },
    { value: 6, label: "S" },
    { value: 7, label: "S" },
  ];
</script>

<div class="schedule-editor">
  <div class="schedule-header">
    <div>
      <strong>Bandwidth schedule</strong>
      <span>Override the global speed limit during selected local-time windows.</span>
    </div>
    <Button variant="subtle" disabled={controller.bandwidthWindows.length >= 32} onclick={() => controller.addBandwidthWindow()}>
      <Icon name="add" size={15} /> Add window
    </Button>
  </div>

  <TextField bind:value={controller.bandwidthTimezone} label="IANA timezone" placeholder="Europe/Rome" hint="Examples: Europe/Rome, America/New_York, UTC." />

  {#if controller.bandwidthWindows.length === 0}
    <div class="empty-schedule">
      <Icon name="clock" size={18} />
      <span>No scheduled limits. The global speed limit applies all day.</span>
    </div>
  {:else}
    <div class="window-list">
      {#each controller.bandwidthWindows as window, index (index)}
        <div class="window-row">
          <div class="weekday-picker" aria-label={`Weekdays for bandwidth window ${index + 1}`}>
            {#each weekdays as weekday}
              <button
                type="button"
                class:active={window.weekdays.includes(weekday.value)}
                aria-pressed={window.weekdays.includes(weekday.value)}
                aria-label={`${["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][weekday.value - 1]} ${window.weekdays.includes(weekday.value) ? "selected" : "not selected"}`}
                onclick={() => controller.toggleBandwidthWeekday(index, weekday.value)}
              >
                {weekday.label}
              </button>
            {/each}
          </div>
          <label class="time-field">
            <span>Start</span>
            <input type="time" bind:value={window.startTime} />
          </label>
          <label class="time-field">
            <span>End</span>
            <input type="time" bind:value={window.endTime} />
          </label>
          <TextField bind:value={window.limitMbps} label="Limit (Mbit/s)" inputmode="decimal" placeholder="0 for unlimited" />
          <IconButton icon="trash" label={`Remove bandwidth window ${index + 1}`} variant="subtle" onclick={() => controller.removeBandwidthWindow(index)} />
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .schedule-editor { display: flex; flex-direction: column; gap: var(--space-4); padding-right: var(--space-4); }
  .schedule-header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); }
  .schedule-header > div { display: flex; flex-direction: column; gap: var(--space-1); }
  .schedule-header span, .empty-schedule { color: var(--text-secondary); font-size: var(--text-caption); }
  .window-list { display: flex; flex-direction: column; gap: var(--space-3); }
  .window-row { display: grid; grid-template-columns: auto minmax(110px, .55fr) minmax(110px, .55fr) minmax(150px, .8fr) auto; align-items: end; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .weekday-picker { align-self: end; display: grid; grid-template-columns: repeat(7, 30px); gap: 2px; padding-bottom: 1px; }
  .weekday-picker button { width: 30px; height: 30px; padding: 0; border: 1px solid var(--stroke-control); border-radius: var(--radius-control); color: var(--text-secondary); background: var(--bg-control); font: inherit; font-size: var(--text-caption); }
  .weekday-picker button:hover { color: var(--text-primary); background: var(--bg-control-hover); }
  .weekday-picker button.active { color: var(--text-on-accent); background: var(--accent-default); border-color: transparent; }
  .time-field { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-body); }
  .time-field input { height: var(--control-default); min-width: 0; padding: 0 var(--space-3); border: 1px solid var(--stroke-control); border-bottom-color: var(--stroke-control-strong); border-radius: var(--radius-control); color: var(--text-primary); background: var(--bg-control); font: inherit; color-scheme: light dark; }
  .time-field input:focus { border-bottom: 2px solid var(--accent-default); outline: none; }
  .empty-schedule { min-height: 54px; display: flex; align-items: center; gap: var(--space-2); padding: var(--space-3); border: 1px dashed var(--stroke-control); border-radius: var(--radius-medium); }
  @media (max-width: 980px) { .window-row { grid-template-columns: 1fr 1fr auto; } .weekday-picker { grid-column: 1 / -1; } }
  @media (max-width: 620px) { .schedule-header { align-items: stretch; flex-direction: column; } .window-row { grid-template-columns: 1fr auto; } .weekday-picker { grid-column: 1 / -1; grid-template-columns: repeat(7, minmax(28px, 1fr)); } .weekday-picker button { width: 100%; } .time-field { min-width: 0; } }
</style>
