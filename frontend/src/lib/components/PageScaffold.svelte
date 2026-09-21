<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    summary = "",
    actions,
    commandBar,
    status,
    children,
    scroll = false,
  }: {
    title: string;
    summary?: string;
    actions?: Snippet;
    commandBar?: Snippet;
    status?: Snippet;
    children: Snippet;
    scroll?: boolean;
  } = $props();
</script>

<section class="page-scaffold">
  <header class="page-titlebar">
    <div class="page-copy">
      <h1>{title}</h1>
      {#if summary}<p>{summary}</p>{/if}
    </div>
    {#if actions}<div class="page-actions">{@render actions()}</div>{/if}
  </header>

  {#if commandBar}<div class="command-region">{@render commandBar()}</div>{/if}
  {#if status}<div class="status-region">{@render status()}</div>{/if}

  <div class:scroll class="page-content">
    {@render children()}
  </div>
</section>

<style>
  .page-scaffold { height: 100%; min-width: 0; display: flex; flex-direction: column; }
  .page-titlebar { min-height: 76px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-5); padding: var(--space-4) var(--page-padding) var(--space-3); border-bottom: 1px solid var(--stroke-divider); }
  .page-copy { min-width: 0; }
  h1 { margin: 0; font-family: var(--font-family-display); font-size: var(--text-title); font-weight: 620; line-height: 1.15; letter-spacing: -.012em; }
  p { margin: 5px 0 0; color: var(--text-tertiary); font-size: var(--text-caption); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .page-actions { display: flex; align-items: center; gap: var(--space-2); flex: none; }
  /* A container query, not a viewport media query: the command bar's actual
     available width depends on the nav sidebar's state (expanded/collapsed/
     overlay) and whether a details pane is open, not on the window's overall
     size. At the app's default window width the sidebar alone already eats
     enough space that a viewport breakpoint never fires, leaving the search
     box and view tabs squeezed into a row that doesn't fit them. */
  .command-region { container-type: inline-size; }
  .command-region, .status-region { flex: none; }
  .page-content { flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .page-content.scroll { overflow: auto; }
  @media (max-width: 720px) {
    .page-titlebar { min-height: 66px; padding-top: var(--space-3); padding-bottom: var(--space-3); }
    p { display: none; }
  }
</style>
