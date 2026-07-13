<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";
  import Tooltip from "./Tooltip.svelte";

  let {
    icon,
    label,
    variant = "standard",
    disabled = false,
    pressed = undefined,
    onclick,
  }: {
    icon: IconName;
    /** Required accessible name; also shown as a tooltip. */
    label: string;
    variant?: "standard" | "subtle" | "accent";
    disabled?: boolean;
    pressed?: boolean;
    onclick?: (event: MouseEvent) => void;
  } = $props();
</script>

<Tooltip text={label}>
  <button
    type="button"
    class="icon-button {variant}"
    aria-label={label}
    aria-pressed={pressed}
    {disabled}
    {onclick}
  >
    <Icon name={icon} size={16} />
  </button>
</Tooltip>

<style>
  .icon-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-default);
    height: var(--control-default);
    border-radius: var(--radius-medium);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-primary);
    cursor: default;
    transition: background var(--motion-fast) var(--motion-easing);
  }
  .icon-button:hover:not(:disabled) {
    background: var(--bg-subtle-hover);
  }
  .icon-button:active:not(:disabled) {
    background: var(--bg-subtle-pressed);
  }
  .icon-button:disabled {
    color: var(--text-disabled);
  }
  .icon-button[aria-pressed="true"] {
    background: var(--accent-subtle);
    color: var(--accent-text);
  }
  .icon-button.standard {
    border-color: var(--stroke-control);
    background: var(--bg-control);
  }
  .icon-button.standard:hover:not(:disabled) {
    background: var(--bg-control-hover);
  }
  .icon-button.accent {
    background: var(--accent-default);
    color: var(--text-on-accent);
  }
  .icon-button.accent:hover:not(:disabled) {
    background: var(--accent-hover);
  }
</style>
