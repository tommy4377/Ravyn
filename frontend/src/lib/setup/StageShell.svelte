<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "../components/Button.svelte";

  let {
    title,
    subtitle = "",
    backLabel = "Back",
    nextLabel = "Next",
    showBack = true,
    nextDisabled = false,
    busy = false,
    onback,
    onnext,
    children,
  }: {
    title: string;
    subtitle?: string;
    backLabel?: string;
    nextLabel?: string;
    showBack?: boolean;
    nextDisabled?: boolean;
    busy?: boolean;
    onback?: () => void;
    onnext?: () => void;
    children: Snippet;
  } = $props();

  let heading = $state<HTMLHeadingElement | null>(null);

  // Move focus to the stage heading so Narrator announces the step change.
  $effect(() => {
    heading?.focus();
  });
</script>

<div class="stage">
  <header class="header">
    <h1 class="title" tabindex="-1" bind:this={heading}>{title}</h1>
    {#if subtitle}
      <p class="subtitle">{subtitle}</p>
    {/if}
  </header>

  <div class="content">
    {@render children()}
  </div>

  <footer class="footer">
    <div>
      {#if showBack && onback}
        <Button onclick={() => onback()} disabled={busy}>{backLabel}</Button>
      {/if}
    </div>
    <div>
      {#if onnext}
        <Button
          variant="accent"
          onclick={() => onnext()}
          disabled={nextDisabled || busy}
        >
          {nextLabel}
        </Button>
      {/if}
    </div>
  </footer>
</div>

<style>
  .stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .header {
    padding: var(--space-8) var(--space-8) var(--space-4);
  }
  .title {
    margin: 0;
    font-family: var(--font-family-display);
    font-size: var(--text-title);
    font-weight: 600;
    line-height: 36px;
    outline: none;
  }
  .subtitle {
    margin: var(--space-2) 0 0;
    color: var(--text-secondary);
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-2) var(--space-8);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .footer {
    display: flex;
    justify-content: space-between;
    padding: var(--space-4) var(--space-8) var(--space-6);
    border-top: 1px solid var(--stroke-divider);
    background: var(--bg-layer-alt);
  }
</style>
