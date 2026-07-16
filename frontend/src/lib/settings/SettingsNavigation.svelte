<script lang="ts">
  import Icon, { type IconName } from "../components/Icon.svelte";
  import type { SettingsCategory } from "./settingsController.svelte";

  let {
    selected,
    onSelect,
  }: {
    selected: SettingsCategory;
    onSelect: (category: SettingsCategory) => void;
  } = $props();

  const categories: { id: SettingsCategory; label: string; icon: IconName }[] = [
    { id: "general", label: "General", icon: "settings" },
    { id: "downloads", label: "Downloads", icon: "download" },
    { id: "storage", label: "Storage and Library", icon: "library" },
    { id: "appearance", label: "Appearance", icon: "palette" },
    { id: "tools", label: "Tools", icon: "components" },
    { id: "network", label: "Network", icon: "cloud" },
    { id: "updates", label: "Updates", icon: "refresh" },
    { id: "privacy", label: "Privacy and Secrets", icon: "shield" },
    { id: "browser", label: "Firefox Integration", icon: "external-link" },
    { id: "troubleshooting", label: "Troubleshooting", icon: "diagnostics" },
    { id: "about", label: "About", icon: "info" },
  ];
</script>

<nav class="settings-nav" aria-label="Settings categories">
  {#each categories as category (category.id)}
    <button type="button" aria-current={selected === category.id ? "page" : undefined} onclick={() => onSelect(category.id)}>
      <span class="indicator"></span>
      <Icon name={category.icon} size={17} />
      <span>{category.label}</span>
    </button>
  {/each}
</nav>

<style>
  .settings-nav { display: flex; flex-direction: column; gap: 2px; padding: var(--space-3); border-right: 1px solid var(--stroke-divider); background: var(--bg-subtle); overflow: auto; }
  button { position: relative; min-height: 38px; display: flex; align-items: center; gap: var(--space-3); padding: 0 var(--space-3); border: 0; border-radius: var(--radius-control); color: var(--text-secondary); background: transparent; font: inherit; text-align: left; }
  button:hover { color: var(--text-primary); background: var(--bg-subtle-hover); }
  button[aria-current="page"] { color: var(--text-primary); background: var(--surface-card-hover); font-weight: 600; }
  .indicator { position: absolute; left: 0; width: 3px; height: 16px; border-radius: var(--radius-pill); background: transparent; }
  button[aria-current="page"] .indicator { background: var(--accent-default); }
  @media (max-width: 900px) {
    .settings-nav { flex-direction: row; border-right: 0; border-bottom: 1px solid var(--stroke-divider); overflow-x: auto; }
    button { flex: none; }
  }
</style>
