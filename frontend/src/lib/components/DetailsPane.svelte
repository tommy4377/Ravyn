<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import Tabs, { type TabItem } from "./Tabs.svelte";

  let {
    title,
    subtitle = "",
    icon = "file",
    onClose,
    tabs = [],
    selectedTab = $bindable(""),
    actions,
    children,
  }: {
    title: string;
    subtitle?: string;
    icon?: IconName;
    onClose: () => void;
    tabs?: TabItem[];
    selectedTab?: string;
    actions?: Snippet;
    children: Snippet;
  } = $props();
</script>

<section class="details-pane">
  <header class="details-header">
    <div class="details-identity">
      <span class="details-icon"><Icon name={icon} size={20} /></span>
      <span class="details-copy">
        <h2>{title}</h2>
        {#if subtitle}<small>{subtitle}</small>{/if}
      </span>
    </div>
    <div class="header-actions">
      {#if actions}{@render actions()}{/if}
      <IconButton icon="close" label="Close details" variant="subtle" onclick={onClose} />
    </div>
  </header>

  {#if tabs.length > 0}
    <Tabs {tabs} bind:selected={selectedTab} />
  {/if}

  <div class="details-content">
    {@render children()}
  </div>
</section>

<style>
  .details-pane {
    height: 100%;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface-content);
  }

  .details-header {
    min-height: 66px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--stroke-divider);
  }

  .details-identity,
  .header-actions {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .details-icon {
    width: 32px;
    height: 32px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-secondary);
  }

  .details-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  h2,
  small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  h2 {
    margin: 0;
    font-size: var(--text-body-strong);
    font-weight: 600;
  }

  small {
    color: var(--text-tertiary);
    font-size: var(--text-caption);
  }

  .details-content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--space-4);
  }
</style>
