<script lang="ts">
  import type { AutomationRule } from "../api/types";
  import Button from "../components/Button.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import StatusBadge from "../components/StatusBadge.svelte";

  let {
    rules,
    search = "",
    onCreate,
    onEdit,
    onToggle,
    onDelete,
  }: {
    rules: AutomationRule[];
    search?: string;
    onCreate: () => void;
    onEdit: (rule: AutomationRule) => void;
    onToggle: (rule: AutomationRule) => void;
    onDelete: (rule: AutomationRule) => void;
  } = $props();

  function conditionSummary(rule: AutomationRule): string {
    const parts: string[] = [];
    if (rule.matcher.domains.length) parts.push(`${rule.matcher.domains.length} domain${rule.matcher.domains.length === 1 ? "" : "s"}`);
    if (rule.matcher.extensions.length) parts.push(`${rule.matcher.extensions.length} extension${rule.matcher.extensions.length === 1 ? "" : "s"}`);
    if (rule.matcher.mime_types.length) parts.push(`${rule.matcher.mime_types.length} MIME type${rule.matcher.mime_types.length === 1 ? "" : "s"}`);
    if (rule.matcher.url_regex) parts.push("URL pattern");
    return parts.length ? parts.join(" · ") : "Matches every download";
  }

  function actionSummary(rule: AutomationRule): string {
    const parts: string[] = [];
    if (rule.actions.destination) parts.push(rule.actions.destination);
    if (rule.actions.tags.length) parts.push(`Tags: ${rule.actions.tags.join(", ")}`);
    if (rule.actions.speed_limit_bps) parts.push(`${Math.round(rule.actions.speed_limit_bps / 125000 * 10) / 10} Mbit/s`);
    return parts.length ? parts.join(" · ") : "No changes configured";
  }
</script>

{#if rules.length === 0}
  <EmptyState
    icon="rule"
    title={search ? "No matching rules" : "No rules yet"}
    message={search ? "Try another search." : "Create a rule to organize new downloads automatically."}
  >
    {#if !search}<Button variant="accent" onclick={onCreate}>Create a rule</Button>{/if}
  </EmptyState>
{:else}
  <div class="list" role="list">
    {#each rules as rule (rule.id)}
      <article class="row" role="listitem" class:disabled={!rule.enabled}>
        <span class="row-icon"><Icon name="rule" size={18} /></span>
        <div class="copy">
          <div class="title-line">
            <strong>{rule.name}</strong>
            <StatusBadge label={rule.enabled ? "Enabled" : "Disabled"} severity={rule.enabled ? "success" : "neutral"} />
          </div>
          <span>{conditionSummary(rule)}</span>
          <small>{actionSummary(rule)} · Priority {rule.priority}</small>
        </div>
        <MenuButton
          label={`Actions for ${rule.name}`}
          icon="more"
          iconOnly
          variant="subtle"
          items={[
            { id: "edit", label: "Edit", icon: "edit", onSelect: () => onEdit(rule) },
            { id: "toggle", label: rule.enabled ? "Disable" : "Enable", icon: rule.enabled ? "pause" : "play", onSelect: () => onToggle(rule) },
            { id: "delete", label: "Delete", icon: "trash", danger: true, separatorBefore: true, onSelect: () => onDelete(rule) },
          ]}
        />
      </article>
    {/each}
  </div>
{/if}

<style>
  .list { height: 100%; min-height: 0; overflow: auto; }
  .row { min-height: 76px; display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .row:hover { background: var(--bg-subtle-hover); }
  .row.disabled { opacity: .72; }
  .row-icon { width: 32px; height: 32px; display: grid; place-items: center; color: var(--text-secondary); }
  .copy { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .title-line { min-width: 0; display: flex; align-items: center; gap: var(--space-2); }
  strong, span, small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  strong { font-weight: 600; }
  .copy > span, small { color: var(--text-tertiary); font-size: var(--text-caption); }
</style>
