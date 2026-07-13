<script lang="ts">
  import { tick, type Snippet } from "svelte";
  import IconButton from "./IconButton.svelte";

  let {
    open,
    title,
    onClose,
    size = "medium",
    preventClose = false,
    children,
    footer,
  }: {
    open: boolean;
    title: string;
    onClose: () => void;
    size?: "small" | "medium" | "large";
    preventClose?: boolean;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let dialogEl = $state<HTMLDivElement | null>(null);
  let returnFocusEl: HTMLElement | null = null;

  function focusableElements(): HTMLElement[] {
    if (!dialogEl) return [];
    return Array.from(
      dialogEl.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    );
  }

  function trapFocus(event: KeyboardEvent): void {
    const focusables = focusableElements();
    if (focusables.length === 0) return;
    const first = focusables[0]!;
    const last = focusables[focusables.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !preventClose) {
      event.preventDefault();
      onClose();
    } else if (event.key === "Tab") {
      trapFocus(event);
    }
  }

  $effect(() => {
    if (!open) return;
    returnFocusEl = document.activeElement as HTMLElement | null;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    void tick().then(() => {
      const initial =
        dialogEl?.querySelector<HTMLElement>("[data-autofocus]") ?? focusableElements()[0];
      (initial ?? dialogEl)?.focus();
    });
    window.addEventListener("keydown", onKeydown);
    return () => {
      window.removeEventListener("keydown", onKeydown);
      document.body.style.overflow = previousOverflow;
      returnFocusEl?.focus();
    };
  });
</script>

{#if open}
  <div class="backdrop" role="presentation" onpointerdown={() => !preventClose && onClose()}>
    <div
      bind:this={dialogEl}
      class="dialog {size}"
      role="dialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      tabindex="-1"
      onpointerdown={(event) => event.stopPropagation()}
    >
      <header class="dialog-header">
        <h2 id="dialog-title">{title}</h2>
        <IconButton icon="close" label="Close" variant="subtle" onclick={() => onClose()} />
      </header>
      <div class="dialog-body">
        {@render children()}
      </div>
      {#if footer}
        <footer class="dialog-footer">
          {@render footer()}
        </footer>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.35);
    padding: var(--space-6);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - var(--space-12));
    width: 100%;
    border-radius: var(--radius-layer);
    background: var(--bg-layer);
    box-shadow: var(--shadow-flyout);
    border: 1px solid var(--stroke-control);
  }
  .dialog.small {
    max-width: 380px;
  }
  .dialog.medium {
    max-width: 560px;
  }
  .dialog.large {
    max-width: 800px;
  }
  .dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--stroke-divider);
  }
  .dialog-header h2 {
    margin: 0;
    font-size: var(--text-subtitle);
    font-weight: 600;
  }
  .dialog-body {
    padding: var(--space-5);
    overflow-y: auto;
  }
  .dialog-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-5);
    border-top: 1px solid var(--stroke-divider);
  }
</style>
