<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import PathPicker from "../components/PathPicker.svelte";
  import TextField from "../components/TextField.svelte";
  import ToggleSwitch from "../components/ToggleSwitch.svelte";
  import type { AutomationController } from "./automationController.svelte";
  import type { RuleActionKind, RuleConditionKind } from "./automationPresentation";

  let { controller }: { controller: AutomationController } = $props();

  const conditionOptions: DropdownOption[] = [
    { value: "domain", label: "Domain is or contains" },
    { value: "extension", label: "File extension is" },
    { value: "mime", label: "MIME type is" },
    { value: "url_pattern", label: "URL matches pattern" },
  ];
  const actionOptions: DropdownOption[] = [
    { value: "destination", label: "Set destination" },
    { value: "tags", label: "Add tags" },
    { value: "speed_limit", label: "Set speed limit" },
  ];

  function conditionPlaceholder(kind: RuleConditionKind): string {
    if (kind === "domain") return "example.com, cdn.example.com";
    if (kind === "extension") return "zip, mp4, mkv";
    if (kind === "mime") return "video/mp4, application/zip";
    return "^https://example\\.com/releases/";
  }

  function actionPlaceholder(kind: RuleActionKind): string {
    if (kind === "destination") return "Choose a destination";
    if (kind === "tags") return "work, archive";
    return "10 Mbit/s";
  }
</script>

<Dialog
  open={controller.ruleOpen}
  title={controller.editingRule ? "Edit rule" : "New rule"}
  size="large"
  onClose={() => controller.closeRule()}
  preventClose={controller.ruleBusy}
>
  <div class="editor">
    <TextField bind:value={controller.ruleDraft.name} label="Rule name" placeholder="Organize video downloads" />

    <section class="builder-section" aria-labelledby="rule-when-heading">
      <header>
        <div><span class="step">When</span><h3 id="rule-when-heading">A new download matches</h3></div>
        <Button variant="subtle" onclick={() => controller.addCondition()}><Icon name="add" size={14} /> Add condition</Button>
      </header>
      <div class="blocks">
        {#each controller.ruleDraft.conditions as condition (condition.id)}
          <div class="builder-block">
            <Dropdown bind:value={condition.kind} options={conditionOptions} label="Condition type" />
            <div class="value-field">
              {#if condition.kind === "domain" || condition.kind === "extension" || condition.kind === "mime"}
                <TextField bind:value={condition.value} label="Values" placeholder={conditionPlaceholder(condition.kind)} hint="Separate multiple values with commas." />
              {:else}
                <TextField bind:value={condition.value} label="Pattern" placeholder={conditionPlaceholder(condition.kind)} hint="Regular expression evaluated by the backend." />
              {/if}
            </div>
            <IconButton icon="trash" label="Remove condition" variant="subtle" onclick={() => controller.removeCondition(condition.id)} />
          </div>
        {/each}
      </div>
    </section>

    <section class="builder-section" aria-labelledby="rule-then-heading">
      <header>
        <div><span class="step">Then</span><h3 id="rule-then-heading">Apply these changes</h3></div>
        <Button variant="subtle" onclick={() => controller.addAction()}><Icon name="add" size={14} /> Add action</Button>
      </header>
      <div class="blocks">
        {#each controller.ruleDraft.actions as action (action.id)}
          <div class="builder-block">
            <Dropdown bind:value={action.kind} options={actionOptions} label="Action type" />
            <div class="value-field">
              {#if action.kind === "destination"}
                <PathPicker bind:value={action.value} label="Destination" placeholder={actionPlaceholder(action.kind)} />
              {:else if action.kind === "tags"}
                <TextField bind:value={action.value} label="Tags" placeholder={actionPlaceholder(action.kind)} hint="Separate multiple tags with commas." />
              {:else}
                <TextField bind:value={action.value} label="Speed limit (Mbit/s)" placeholder="10" inputmode="decimal" hint="Use 0 or leave empty for unlimited." />
              {/if}
            </div>
            <IconButton icon="trash" label="Remove action" variant="subtle" onclick={() => controller.removeAction(action.id)} />
          </div>
        {/each}
      </div>
    </section>

    <AdvancedDisclosure title="Advanced rule settings" description="Priority controls which rule wins when several rules set the same value.">
      <div class="advanced-grid">
        <TextField bind:value={controller.ruleDraft.priority} label="Priority" inputmode="numeric" placeholder="0" />
        <ToggleSwitch bind:checked={controller.ruleDraft.enabled} label="Enabled" description="Disabled rules are kept but do not affect new downloads." />
      </div>
    </AdvancedDisclosure>
  </div>

  {#snippet footer()}
    <Button disabled={controller.ruleBusy} onclick={() => controller.closeRule()}>Cancel</Button>
    <Button
      variant="accent"
      disabled={controller.ruleBusy || !controller.ruleDraft.name.trim()}
      onclick={() => void controller.saveRule()}
    >
      {controller.ruleBusy ? "Saving…" : "Save rule"}
    </Button>
  {/snippet}
</Dialog>

<style>
  .editor { display: flex; flex-direction: column; gap: var(--space-5); }
  .builder-section { display: flex; flex-direction: column; gap: var(--space-3); }
  .builder-section > header { min-height: 38px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
  .builder-section header > div { display: flex; align-items: baseline; gap: var(--space-2); }
  .step { color: var(--accent-text); font-size: var(--text-caption); font-weight: 700; text-transform: uppercase; letter-spacing: .04em; }
  h3 { margin: 0; font-size: var(--text-body-strong); }
  .blocks { display: flex; flex-direction: column; border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); overflow: hidden; }
  .builder-block { display: grid; grid-template-columns: minmax(180px, .55fr) minmax(0, 1.45fr) auto; align-items: start; gap: var(--space-3); padding: var(--space-3); border-bottom: 1px solid var(--stroke-divider); background: var(--surface-content); }
  .builder-block:last-child { border-bottom: 0; }
  .builder-block :global(.dropdown), .builder-block :global(select) { width: 100%; }
  .value-field { min-width: 0; }
  .advanced-grid { display: grid; grid-template-columns: minmax(160px, .5fr) minmax(0, 1.5fr); gap: var(--space-4); align-items: end; }
  @media (max-width: 720px) { .builder-block, .advanced-grid { grid-template-columns: 1fr; } .builder-block > :global(button) { justify-self: end; } }
</style>
