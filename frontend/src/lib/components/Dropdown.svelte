<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";

  export interface DropdownOption {
    value: string;
    label: string;
  }

  let {
    options,
    value = $bindable(""),
    label,
    onchange,
    id,
  }: {
    options: DropdownOption[];
    value?: string;
    label: string;
    onchange?: (value: string) => void;
    id?: string;
  } = $props();

  let open = $state(false);
  let activeIndex = $state(0);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let listEl = $state<HTMLDivElement | null>(null);
  let optionEls = $state<(HTMLDivElement | null)[]>([]);
  let listX = $state(0);
  let listY = $state(0);
  let listWidth = $state(0);
  let positioned = $state(false);
  let typeahead = "";
  let typeaheadTimer: ReturnType<typeof setTimeout> | null = null;

  const selectedIndex = $derived(Math.max(0, options.findIndex((option) => option.value === value)));
  const selectedLabel = $derived(options.find((option) => option.value === value)?.label ?? options[0]?.label ?? "");
  const listboxId = $derived(`${id ?? "dropdown"}-listbox`);

  function openList(): void {
    if (open || options.length === 0) return;
    activeIndex = selectedIndex;
    open = true;
  }

  function closeList(focusTrigger = true): void {
    if (!open) return;
    open = false;
    positioned = false;
    if (focusTrigger) triggerEl?.focus();
  }

  function commit(index: number): void {
    const option = options[index];
    if (!option) return;
    const changed = option.value !== value;
    value = option.value;
    closeList();
    if (changed) onchange?.(option.value);
  }

  function clampPosition(): void {
    if (!listEl || !triggerEl) return;
    const margin = 8;
    const trigger = triggerEl.getBoundingClientRect();
    const rect = listEl.getBoundingClientRect();
    listWidth = Math.max(trigger.width, 160);
    listX = Math.max(margin, Math.min(trigger.left, window.innerWidth - rect.width - margin));
    const below = trigger.bottom + 4;
    listY = below + rect.height + margin > window.innerHeight
      ? Math.max(margin, trigger.top - rect.height - 4)
      : below;
    positioned = true;
  }

  $effect(() => {
    if (!open || !listEl) return;
    positioned = false;
    if (typeof listEl.showPopover === "function") {
      try {
        listEl.showPopover();
      } catch {
        // A reactive update can run while the popover is already open.
      }
    }
    void tick().then(() => {
      clampPosition();
      optionEls[activeIndex]?.scrollIntoView?.({ block: "nearest" });
      listEl?.focus();
    });

    function onPointerDown(event: PointerEvent): void {
      if (
        event.target instanceof Node &&
        !listEl?.contains(event.target) &&
        !triggerEl?.contains(event.target)
      ) {
        closeList(false);
      }
    }
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("resize", clampPosition);
    window.addEventListener("scroll", onScrollDismiss, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("resize", clampPosition);
      window.removeEventListener("scroll", onScrollDismiss, true);
    };
  });

  function onScrollDismiss(event: Event): void {
    if (event.target instanceof Node && listEl?.contains(event.target)) return;
    closeList(false);
  }

  function moveActive(delta: number): void {
    if (options.length === 0) return;
    activeIndex = (activeIndex + delta + options.length) % options.length;
    optionEls[activeIndex]?.scrollIntoView?.({ block: "nearest" });
  }

  function runTypeahead(key: string): void {
    typeahead += key.toLowerCase();
    if (typeaheadTimer) clearTimeout(typeaheadTimer);
    typeaheadTimer = setTimeout(() => (typeahead = ""), 600);
    const match = options.findIndex((option) => option.label.toLowerCase().startsWith(typeahead));
    if (match !== -1) {
      if (open) {
        activeIndex = match;
        optionEls[match]?.scrollIntoView?.({ block: "nearest" });
      } else {
        commit(match);
      }
    }
  }

  function onTriggerKeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
      case "Enter":
      case " ":
        event.preventDefault();
        openList();
        break;
      case "Home":
        event.preventDefault();
        commit(0);
        break;
      case "End":
        event.preventDefault();
        commit(options.length - 1);
        break;
      default:
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          runTypeahead(event.key);
        }
        break;
    }
  }

  function onListKeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveActive(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveActive(-1);
        break;
      case "Home":
        event.preventDefault();
        activeIndex = 0;
        optionEls[0]?.scrollIntoView?.({ block: "nearest" });
        break;
      case "End":
        event.preventDefault();
        activeIndex = options.length - 1;
        optionEls[activeIndex]?.scrollIntoView?.({ block: "nearest" });
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        commit(activeIndex);
        break;
      case "Escape":
        event.preventDefault();
        closeList();
        break;
      case "Tab":
        closeList(false);
        break;
      default:
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          event.preventDefault();
          runTypeahead(event.key);
        }
        break;
    }
  }

  function syncPopoverState(event: ToggleEvent): void {
    if (event.newState === "closed" && open) {
      closeList(false);
    }
  }
</script>

<div class="dropdown">
  <button
    bind:this={triggerEl}
    {id}
    type="button"
    class="trigger"
    role="combobox"
    aria-label={label}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={open ? listboxId : undefined}
    onclick={() => (open ? closeList(false) : openList())}
    onkeydown={onTriggerKeydown}
  >
    <span class="trigger-label">{selectedLabel}</span>
    <Icon name="chevron-down" size={12} />
  </button>
</div>

{#if open}
  <div
    bind:this={listEl}
    id={listboxId}
    class="listbox"
    role="listbox"
    aria-label={label}
    tabindex="-1"
    popover="auto"
    ontoggle={syncPopoverState}
    onkeydown={onListKeydown}
    aria-activedescendant={`${listboxId}-option-${activeIndex}`}
    style="left:{listX}px; top:{listY}px; min-width:{listWidth}px; visibility:{positioned ? 'visible' : 'hidden'};"
  >
    {#each options as option, index (option.value)}
      <div
        bind:this={optionEls[index]}
        id={`${listboxId}-option-${index}`}
        class="option"
        class:active={index === activeIndex}
        role="option"
        tabindex="-1"
        aria-selected={option.value === value}
        onpointerenter={() => (activeIndex = index)}
        onpointerdown={(event) => event.preventDefault()}
        onclick={() => commit(index)}
        onkeydown={onListKeydown}
      >
        <span class="check" aria-hidden="true">
          {#if option.value === value}<Icon name="check" size={13} />{/if}
        </span>
        <span class="option-label">{option.label}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .dropdown {
    position: relative;
    display: inline-flex;
    align-items: center;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    width: 100%;
    min-width: 0;
    height: var(--control-default);
    padding: 0 var(--space-3);
    border-radius: var(--radius-medium);
    border: 1px solid var(--stroke-control);
    background: var(--bg-control);
    color: var(--text-primary);
    font-family: inherit;
    font-size: var(--text-body);
    text-align: left;
    cursor: default;
  }
  .trigger:hover {
    background: var(--bg-control-hover);
  }
  .trigger:focus-visible {
    outline: 2px solid var(--stroke-focus);
    outline-offset: 1px;
  }
  .trigger-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .trigger :global(svg) {
    flex: none;
    color: var(--text-secondary);
  }
  .listbox {
    position: fixed;
    inset: auto;
    margin: 0;
    z-index: 200;
    max-width: min(420px, calc(100vw - 16px));
    max-height: min(320px, calc(100vh - 16px));
    overflow-y: auto;
    padding: var(--space-1);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-layer);
    border: 1px solid var(--stroke-surface);
    background: var(--surface-flyout);
    box-shadow: var(--shadow-flyout);
    backdrop-filter: blur(28px) saturate(125%);
    -webkit-backdrop-filter: blur(28px) saturate(125%);
  }
  .listbox:focus-visible {
    outline: none;
  }
  .option {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-2);
    border-radius: var(--radius-medium);
    color: var(--text-primary);
    font-size: var(--text-body);
    white-space: nowrap;
    cursor: default;
  }
  .option.active {
    background: var(--bg-subtle-hover);
  }
  .option[aria-selected="true"] {
    font-weight: 600;
  }
  .check {
    display: inline-flex;
    width: 14px;
    flex: none;
    color: var(--accent-default);
  }
  .option-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
