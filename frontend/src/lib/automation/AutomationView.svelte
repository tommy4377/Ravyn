<script lang="ts">
  import Button from "../components/Button.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import PageCommandBar from "../components/PageCommandBar.svelte";
  import PageScaffold from "../components/PageScaffold.svelte";
  import SearchBox from "../components/SearchBox.svelte";
  import Surface from "../components/Surface.svelte";
  import { AutomationController } from "./automationController.svelte";
  import ExecutionHistory from "./ExecutionHistory.svelte";
  import RuleEditor from "./RuleEditor.svelte";
  import RulePreview from "./RulePreview.svelte";
  import RulesList from "./RulesList.svelte";
  import ScheduleEditor from "./ScheduleEditor.svelte";
  import SchedulesList from "./SchedulesList.svelte";
  import { navigation } from "../stores/navigation.svelte";

  const controller = new AutomationController();

  $effect(() => {
    void controller.load();
  });

  $effect(() => {
    if (navigation.pendingScheduleSource === null) return;
    const source = navigation.consumeScheduleSource();
    if (source === null) return;
    controller.tab = "schedules";
    controller.openSchedule();
    controller.scheduleDraft.source = source;
  });
</script>

<PageScaffold title="Automation" summary="Organize incoming downloads and run recurring tasks.">
  {#snippet actions()}
    {#if controller.tab === "rules"}
      <Button onclick={() => controller.openRulePreview()}><Icon name="search" size={15} /> Test rules</Button>
      <Button variant="accent" onclick={() => controller.openRule()}><Icon name="add" size={15} /> New rule</Button>
    {:else}
      <Button variant="accent" onclick={() => controller.openSchedule()}><Icon name="add" size={15} /> New schedule</Button>
    {/if}
  {/snippet}

  {#snippet commandBar()}
    <PageCommandBar ariaLabel="Automation commands">
      {#snippet leading()}
        <div class="segments" role="tablist" aria-label="Automation section">
          <button type="button" role="tab" aria-selected={controller.tab === "rules"} onclick={() => (controller.tab = "rules")}>
            <Icon name="rule" size={16} /> Rules <span>{controller.rules.length}</span>
          </button>
          <button type="button" role="tab" aria-selected={controller.tab === "schedules"} onclick={() => (controller.tab = "schedules")}>
            <Icon name="calendar" size={16} /> Schedules <span>{controller.schedules.length}</span>
          </button>
        </div>
      {/snippet}
      {#snippet actions()}
        <SearchBox bind:value={controller.search} label="Search automation" placeholder={`Search ${controller.tab}`} />
        <MenuButton
          label="More automation commands"
          icon="more"
          iconOnly
          variant="subtle"
          items={[
            { id: "refresh", label: "Refresh", icon: "refresh", disabled: controller.loading, onSelect: () => void controller.load() },
            ...(controller.tab === "rules" ? [{ id: "test", label: "Test rules", icon: "search" as const, onSelect: () => controller.openRulePreview() }] : []),
          ]}
        />
      {/snippet}
    </PageCommandBar>
  {/snippet}

  {#if controller.error}
    <div class="state"><InlineError title="Couldn't load automation" message={controller.error} retry={() => void controller.load()} /></div>
  {:else if controller.loading}
    <div class="state muted">Loading automation…</div>
  {:else}
    <Surface padding="none" class="automation-surface">
      {#if controller.tab === "rules"}
        <RulesList
          rules={controller.visibleRules}
          search={controller.search}
          onCreate={() => controller.openRule()}
          onEdit={(rule) => controller.openRule(rule)}
          onToggle={(rule) => void controller.toggleRule(rule)}
          onDelete={(rule) => controller.requestDelete("rule", rule.id)}
        />
      {:else}
        <SchedulesList
          schedules={controller.visibleSchedules}
          search={controller.search}
          onCreate={() => controller.openSchedule()}
          onEdit={(schedule) => controller.openSchedule(schedule)}
          onToggle={(schedule) => void controller.toggleSchedule(schedule)}
          onRun={(schedule) => void controller.runNow(schedule)}
          onHistory={(schedule) => void controller.openHistory(schedule)}
          onDelete={(schedule) => controller.requestDelete("schedule", schedule.id)}
        />
      {/if}
    </Surface>
  {/if}
</PageScaffold>

<RuleEditor {controller} />
<ScheduleEditor {controller} />
<ExecutionHistory {controller} />
<RulePreview {controller} />

<ConfirmDialog
  open={!!controller.deleteKind}
  title={`Delete ${controller.deleteKind ?? "item"}?`}
  message={`This ${controller.deleteKind ?? "item"} will be removed permanently.`}
  confirmLabel="Delete"
  destructive
  busy={controller.deleteBusy}
  error={controller.deleteError}
  onConfirm={() => void controller.confirmDelete()}
  onClose={() => !controller.deleteBusy && (controller.deleteKind = null)}
/>

<style>
  .segments { display: inline-flex; padding: 2px; border: 1px solid var(--stroke-control); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .segments button { height: 30px; display: flex; align-items: center; gap: var(--space-2); padding: 0 var(--space-3); border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; font: inherit; }
  .segments button[aria-selected="true"] { color: var(--text-primary); background: var(--surface-card-hover); }
  .segments button span { color: var(--text-tertiary); font-size: var(--text-caption); }
  .state { padding: var(--page-padding); }
  .muted { color: var(--text-secondary); }
  :global(.automation-surface) { height: calc(100% - var(--page-padding)); margin: 0 var(--page-padding) var(--page-padding); display: flex; flex-direction: column; }
  @media (max-width: 700px) { .segments button { padding: 0 var(--space-2); } }
</style>
