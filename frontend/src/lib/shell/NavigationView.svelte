<script lang="ts">
  import Icon, { type IconName } from "../components/Icon.svelte";
  import Tooltip from "../components/Tooltip.svelte";
  import { navigation, type NavSection } from "../stores/navigation.svelte";

  const primary: { id: NavSection; label: string; icon: IconName }[] = [
    { id: "downloads", label: "Downloads", icon: "download" },
    { id: "library", label: "Library", icon: "library" },
    { id: "media", label: "Media", icon: "video" },
    { id: "torrents", label: "Torrents", icon: "torrent" },
    { id: "basket", label: "Basket", icon: "basket" },
    { id: "automation", label: "Automation", icon: "automation" },
    { id: "components", label: "Components", icon: "components" },
  ];

  const secondary: { id: NavSection; label: string; icon: IconName }[] = [
    { id: "settings", label: "Settings", icon: "settings" },
    { id: "diagnostics", label: "Diagnostics", icon: "diagnostics" },
  ];

  function activate(section: NavSection): void {
    navigation.section = section;
  }
</script>

<nav class:collapsed={navigation.navigationCollapsed} class="nav" aria-label="Primary navigation">
  <div class="brand-row">
    <button
      class="pane-toggle"
      type="button"
      aria-label={navigation.navigationCollapsed ? "Expand navigation" : "Collapse navigation"}
      onclick={() => navigation.setNavigationCollapsed(!navigation.navigationCollapsed)}
    >
      <Icon name="menu" size={18} />
    </button>
    <div class="brand" aria-label="Ravyn">
      <span class="mark" aria-hidden="true">R</span>
      <span class="brand-copy"><strong>Ravyn</strong><small>Download manager</small></span>
    </div>
  </div>

  <div class="nav-scroll">
    <ul class="sections">
      {#each primary as section (section.id)}
        <li>
          <Tooltip text={section.label} disabled={!navigation.navigationCollapsed}>
            <button
              type="button"
              class="section"
              aria-current={navigation.section === section.id ? "page" : undefined}
              onclick={() => activate(section.id)}
            >
              <span class="indicator" aria-hidden="true"></span>
              <Icon name={section.icon} size={18} />
              <span class="section-label">{section.label}</span>
            </button>
          </Tooltip>
        </li>
      {/each}
    </ul>
  </div>

  <ul class="sections secondary">
    {#each secondary as section (section.id)}
      <li>
        <Tooltip text={section.label} disabled={!navigation.navigationCollapsed}>
          <button
            type="button"
            class="section"
            aria-current={navigation.section === section.id ? "page" : undefined}
            onclick={() => activate(section.id)}
          >
            <span class="indicator" aria-hidden="true"></span>
            <Icon name={section.icon} size={18} />
            <span class="section-label">{section.label}</span>
          </button>
        </Tooltip>
      </li>
    {/each}
  </ul>
</nav>

<style>
  .nav {
    position: relative;
    z-index: 2;
    display: flex;
    flex-direction: column;
    width: 224px;
    min-width: 224px;
    padding: var(--space-2);
    border-right: 1px solid var(--stroke-divider);
    background: var(--surface-navigation);
    backdrop-filter: blur(34px) saturate(118%);
    -webkit-backdrop-filter: blur(34px) saturate(118%);
    transition: width var(--motion-normal) var(--motion-easing), min-width var(--motion-normal) var(--motion-easing);
  }
  .nav.collapsed { width: 56px; min-width: 56px; }
  .brand-row { display: flex; align-items: center; min-height: 52px; gap: var(--space-1); }
  .pane-toggle {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    flex: none;
    border: 0;
    border-radius: var(--radius-medium);
    background: transparent;
    cursor: default;
  }
  .pane-toggle:hover { background: var(--bg-subtle-hover); }
  .brand { display: flex; align-items: center; min-width: 0; gap: var(--space-2); overflow: hidden; }
  .mark {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 8px;
    color: var(--text-on-accent);
    background: linear-gradient(145deg, var(--accent-default), color-mix(in srgb, var(--accent-default), #121b38 24%));
    box-shadow: inset 0 1px rgba(255,255,255,.2), 0 2px 8px color-mix(in srgb, var(--accent-default), transparent 75%);
    font-family: var(--font-family-display);
    font-size: 16px;
    font-weight: 700;
  }
  .brand-copy { display: flex; flex-direction: column; min-width: 0; white-space: nowrap; }
  .brand-copy strong { font-size: 14px; line-height: 17px; }
  .brand-copy small { color: var(--text-tertiary); font-size: 11px; line-height: 14px; }
  .nav-scroll { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; }
  .sections { display: flex; flex-direction: column; gap: 2px; list-style: none; margin: var(--space-2) 0 0; padding: 0; }
  .secondary { flex: none; padding-top: var(--space-2); border-top: 1px solid var(--stroke-divider); }
  .sections li, .sections :global(.tooltip-wrapper) { width: 100%; }
  .section {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
    height: 40px;
    gap: var(--space-3);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-medium);
    color: var(--text-secondary);
    background: transparent;
    cursor: default;
    overflow: hidden;
    text-align: left;
  }
  .section:hover { color: var(--text-primary); background: var(--bg-subtle-hover); }
  .section:active { background: var(--bg-subtle-pressed); }
  .section[aria-current="page"] { color: var(--text-primary); background: var(--surface-card-hover); font-weight: 600; }
  .indicator {
    position: absolute;
    left: 0;
    width: 3px;
    height: 16px;
    border-radius: var(--radius-pill);
    background: transparent;
    transform: scaleY(.35);
    transition: transform var(--motion-fast) var(--motion-easing), background var(--motion-fast) var(--motion-easing);
  }
  .section[aria-current="page"] .indicator { background: var(--accent-default); transform: scaleY(1); }
  .section-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .collapsed .brand { display: none; }
  .collapsed .section { justify-content: center; padding: 0; gap: 0; }
  .collapsed .section-label { display: none; }
  .collapsed .sections { align-items: center; }
  @media (max-width: 1040px) {
    .nav:not(:hover):not(:focus-within) { width: 56px; min-width: 56px; }
    .nav:not(:hover):not(:focus-within) .brand { display: none; }
    .nav:not(:hover):not(:focus-within) .section { justify-content: center; padding: 0; gap: 0; }
    .nav:not(:hover):not(:focus-within) .section-label { display: none; }
  }
  @media (forced-colors: active) { .nav { backdrop-filter: none; } }
</style>
