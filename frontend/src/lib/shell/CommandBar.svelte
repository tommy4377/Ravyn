<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import MenuButton from "../components/MenuButton.svelte";
  import type { MenuItem } from "../components/Menu.svelte";

  export interface Command {
    id: string;
    label: string;
    icon: IconName;
    onSelect: () => void;
    disabled?: boolean;
    accent?: boolean;
  }

  let {
    commands,
    overflow = [],
    trailing,
  }: {
    commands: Command[];
    overflow?: MenuItem[];
    trailing?: Snippet;
  } = $props();
</script>

<div class="command-bar" role="toolbar" aria-label="Commands">
  {#each commands as command (command.id)}
    <button
      type="button"
      class="command"
      class:accent={command.accent}
      disabled={command.disabled}
      onclick={command.onSelect}
    >
      <Icon name={command.icon} size={15} />
      <span>{command.label}</span>
    </button>
  {/each}

  {#if overflow.length}
    <MenuButton label="More" icon="more" items={overflow} iconOnly variant="subtle" />
  {/if}

  <div class="spacer"></div>

  {#if trailing}
    {@render trailing()}
  {/if}
</div>

<style>
  .command-bar {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--control-large);
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--stroke-divider);
    background: var(--bg-layer);
    flex: none;
  }
  .command {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--control-default);
    padding: 0 var(--space-2);
    border: none;
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--text-body);
    cursor: default;
  }
  .command:hover:not(:disabled) {
    background: var(--bg-subtle-hover);
  }
  .command:disabled {
    color: var(--text-disabled);
  }
  .command.accent {
    color: var(--accent-text);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
</style>
