<script lang="ts">
  import type { Snippet } from "svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";

  let {
    items,
    children,
  }: {
    items: MenuItem[];
    children: Snippet;
  } = $props();

  let open = $state(false);
  let position = $state({ x: 0, y: 0 });
  let returnFocusEl: HTMLElement | null = null;

  function onContextMenu(event: MouseEvent): void {
    event.preventDefault();
    returnFocusEl = document.activeElement as HTMLElement | null;
    position = { x: event.clientX, y: event.clientY };
    open = true;
  }

  function close(): void {
    open = false;
    returnFocusEl?.focus();
  }
</script>

<div role="presentation" oncontextmenu={onContextMenu}>
  {@render children()}
</div>

<Menu {items} {open} x={position.x} y={position.y} onClose={close} />
