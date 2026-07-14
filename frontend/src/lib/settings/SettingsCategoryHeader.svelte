<script lang="ts">
  let { title, description }: { title: string; description: string } = $props();
</script>
<header class="category-header"><h2>{title}</h2><p>{description}</p></header>
<style>
  .category-header { margin-bottom: var(--space-5); }
  h2 { margin: 0; font-family: var(--font-family-display); font-size: var(--text-subtitle); font-weight: 620; }
  p { margin: var(--space-1) 0 0; color: var(--text-secondary); font-size: var(--text-body); }
</style>
