<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    children,
    padding = "normal",
    class: className = "",
  }: {
    children: Snippet;
    padding?: "none" | "small" | "normal" | "large";
    class?: string;
  } = $props();
</script>

<section class="surface {className}" data-padding={padding}>
  {@render children()}
</section>

<style>
  .surface {
    position: relative;
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--stroke-surface);
    border-radius: var(--radius-layer);
    background: var(--surface-card);
  }
  .surface[data-padding="small"] { padding: var(--space-3); }
  .surface[data-padding="normal"] { padding: var(--space-5); }
  .surface[data-padding="large"] { padding: var(--space-7); }
</style>
