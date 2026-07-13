<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";

  let {
    label,
    icon,
    items,
    variant = "standard",
    iconOnly = false,
  }: {
    label: string;
    icon?: IconName;
    items: MenuItem[];
    variant?: "standard" | "subtle";
    iconOnly?: boolean;
  } = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let position = $state({ x: 0, y: 0 });

  function toggle(): void {
    if (open) {
      close();
      return;
    }
    const rect = triggerEl?.getBoundingClientRect();
    if (rect) position = { x: rect.left, y: rect.bottom + 4 };
    open = true;
  }

  function close(): void {
    open = false;
    triggerEl?.focus();
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" && !open) {
      event.preventDefault();
      toggle();
    }
  }
</script>

<button
  bind:this={triggerEl}
  type="button"
  class="menu-trigger {variant}"
  class:icon-only={iconOnly}
  aria-haspopup="menu"
  aria-expanded={open}
  onclick={toggle}
  onkeydown={onKeydown}
>
  {#if icon}<Icon name={icon} size={15} />{/if}
  {#if !iconOnly}<span>{label}</span>{/if}
  {#if !iconOnly}<Icon name="chevron-down" size={12} />{/if}
</button>

<Menu {items} {open} x={position.x} y={position.y} onClose={close} />

<style>
  .menu-trigger {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: var(--control-default);
    padding: 0 var(--space-3);
    border-radius: var(--radius-medium);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--text-body);
    cursor: default;
  }
  .menu-trigger.icon-only {
    width: var(--control-default);
    padding: 0;
    justify-content: center;
  }
  .menu-trigger.standard {
    border-color: var(--stroke-control);
    background: var(--bg-control);
  }
  .menu-trigger:hover {
    background: var(--bg-control-hover);
  }
  .menu-trigger[aria-expanded="true"] {
    background: var(--bg-subtle-pressed);
  }
</style>
