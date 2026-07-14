<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextField from "../components/TextField.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import type { AutomationController } from "./automationController.svelte";

  let { controller }: { controller: AutomationController } = $props();

  const kindOptions: DropdownOption[] = [
    { value: "http", label: "Direct download" },
    { value: "media", label: "Media" },
    { value: "torrent", label: "Torrent" },
  ];
  const cadenceOptions: DropdownOption[] = [
    { value: "once", label: "Once" },
    { value: "interval", label: "Every N minutes or hours" },
    { value: "daily", label: "Daily at" },
    { value: "weekly", label: "Weekly on" },
    { value: "advanced", label: "Advanced cron" },
  ];
  const unitOptions: DropdownOption[] = [
    { value: "minutes", label: "Minutes" },
    { value: "hours", label: "Hours" },
  ];
  const weekdayOptions: DropdownOption[] = [
    { value: "1", label: "Monday" },
    { value: "2", label: "Tuesday" },
    { value: "3", label: "Wednesday" },
    { value: "4", label: "Thursday" },
    { value: "5", label: "Friday" },
    { value: "6", label: "Saturday" },
    { value: "0", label: "Sunday" },
  ];
</script>

<Dialog
  open={controller.scheduleOpen}
  title={controller.editingSchedule ? "Edit schedule" : "New schedule"}
  size="large"
  onClose={() => controller.closeSchedule()}
  preventClose={controller.scheduleBusy}
>
  <div class="editor">
    <TextField bind:value={controller.scheduleDraft.source} label="Source" placeholder="https://example.com/file.zip" />
    <PathPicker bind:value={controller.scheduleDraft.destination} label="Destination" placeholder="Choose a download folder" />

    <div class="two-column">
      <label class="select-field"><span>Download type</span><Dropdown bind:value={controller.scheduleDraft.kind} options={kindOptions} label="Download type" /></label>
      <label class="select-field"><span>Schedule</span><Dropdown bind:value={controller.scheduleDraft.cadence} options={cadenceOptions} label="Schedule mode" /></label>
    </div>

    <section class="cadence-panel">
      {#if controller.scheduleDraft.cadence === "once"}
        <label class="native-field"><span>Run at</span><input type="datetime-local" bind:value={controller.scheduleDraft.onceAt} /></label>
      {:else if controller.scheduleDraft.cadence === "interval"}
        <div class="two-column">
          <TextField bind:value={controller.scheduleDraft.intervalValue} label="Repeat every" inputmode="numeric" placeholder="1" />
          <label class="select-field"><span>Unit</span><Dropdown bind:value={controller.scheduleDraft.intervalUnit} options={unitOptions} label="Interval unit" /></label>
        </div>
      {:else if controller.scheduleDraft.cadence === "daily"}
        <label class="native-field"><span>Time</span><input type="time" bind:value={controller.scheduleDraft.timeOfDay} /></label>
      {:else if controller.scheduleDraft.cadence === "weekly"}
        <div class="two-column">
          <label class="select-field"><span>Day</span><Dropdown bind:value={controller.scheduleDraft.weekday} options={weekdayOptions} label="Weekday" /></label>
          <label class="native-field"><span>Time</span><input type="time" bind:value={controller.scheduleDraft.timeOfDay} /></label>
        </div>
      {:else}
        <AdvancedDisclosure title="Cron expression" description="Use this only when the simple schedule modes cannot represent the timing you need." open>
          <TextField bind:value={controller.scheduleDraft.cronExpression} label="Cron" placeholder="0 9 * * 1-5" hint="Five fields: minute, hour, day of month, month, day of week." />
        </AdvancedDisclosure>
      {/if}
    </section>

    <ToggleSwitch bind:checked={controller.scheduleDraft.enabled} label="Enable immediately" description="You can disable the schedule later without deleting it." />
  </div>

  {#snippet footer()}
    <Button disabled={controller.scheduleBusy} onclick={() => controller.closeSchedule()}>Cancel</Button>
    <Button
      variant="accent"
      disabled={controller.scheduleBusy || !controller.scheduleDraft.source.trim() || !controller.scheduleDraft.destination.trim()}
      onclick={() => void controller.saveSchedule()}
    >
      {controller.scheduleBusy ? "Saving…" : controller.editingSchedule ? "Save schedule" : "Create schedule"}
    </Button>
  {/snippet}
</Dialog>

<style>
  .editor { display: flex; flex-direction: column; gap: var(--space-4); }
  .two-column { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-3); }
  .select-field, .native-field { display: flex; flex-direction: column; gap: var(--space-1); }
  .select-field > span, .native-field > span { color: var(--text-primary); font-size: var(--text-body); }
  .select-field :global(.dropdown), .select-field :global(select) { width: 100%; }
  .native-field input { height: var(--control-default); padding: 0 var(--space-3); border: 1px solid var(--stroke-control); border-bottom-color: var(--stroke-control-strong); border-radius: var(--radius-control); color: var(--text-primary); background: var(--bg-control); font: inherit; color-scheme: light dark; }
  .native-field input:focus { border-bottom: 2px solid var(--accent-default); outline: none; }
  .cadence-panel { padding: var(--space-4); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  @media (max-width: 660px) { .two-column { grid-template-columns: 1fr; } }
</style>
