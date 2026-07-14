<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let { title = "Advanced", description = "", open = false, children }: { title?: string; description?: string; open?: boolean; children: Snippet } = $props();
</script>

<details bind:open class="advanced-disclosure">
  <summary>
    <span class="chevron"><Icon name="chevron-right" size={14} /></span>
    <span class="copy"><strong>{title}</strong>{#if description}<small>{description}</small>{/if}</span>
  </summary>
  <div class="content">{@render children()}</div>
</details>

<style>
  .advanced-disclosure { border-top: 1px solid var(--stroke-divider); }
  summary { min-height: 48px; display: flex; align-items: center; gap: var(--space-2); padding: 0 var(--space-1); cursor: default; list-style: none; }
  summary::-webkit-details-marker { display: none; }
  summary:hover { background: var(--bg-subtle); }
  .chevron { display: grid; place-items: center; color: var(--text-tertiary); transition: transform var(--motion-fast) var(--motion-easing); }
  details[open] .chevron { transform: rotate(90deg); }
  .copy { display: flex; flex-direction: column; min-width: 0; }
  strong { font-size: var(--text-body); font-weight: 600; }
  small { color: var(--text-tertiary); font-size: var(--text-caption); }
  .content { padding: 0 0 var(--space-4) var(--space-6); }
</style>
