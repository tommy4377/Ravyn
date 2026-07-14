<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    detailsOpen = false,
    detailsLabel = "Details",
    detailsWidth = "410px",
    list,
    details,
  }: {
    detailsOpen?: boolean;
    detailsLabel?: string;
    detailsWidth?: string;
    list: Snippet;
    details?: Snippet;
  } = $props();
</script>

<div
  class="list-details-layout"
  class:with-details={detailsOpen && !!details}
  style={`--details-pane-width:${detailsWidth}`}
>
  <div class="list-region">
    {@render list()}
  </div>
  {#if detailsOpen && details}
    <aside class="details-region" aria-label={detailsLabel}>
      {@render details()}
    </aside>
  {/if}
</div>

<style>
  .list-details-layout {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-3);
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .list-details-layout.with-details {
    grid-template-columns: minmax(0, 1fr) minmax(320px, var(--details-pane-width));
  }

  .list-region,
  .details-region {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .details-region {
    border-left: 1px solid var(--stroke-divider);
    background: var(--surface-content);
  }

  @media (max-width: 920px) {
    .list-details-layout.with-details {
      grid-template-columns: minmax(0, 1fr);
    }

    .details-region {
      position: absolute;
      inset: 0;
      z-index: 30;
      border: 1px solid var(--stroke-surface);
      border-radius: var(--radius-layer);
      background: var(--surface-overlay);
      box-shadow: var(--shadow-flyout);
      backdrop-filter: blur(28px) saturate(120%);
      -webkit-backdrop-filter: blur(28px) saturate(120%);
    }
  }
</style>
