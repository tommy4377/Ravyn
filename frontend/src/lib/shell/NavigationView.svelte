<script lang="ts">
  import { onMount } from "svelte";
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

  let compactViewport = $state(false);
  const effectivelyCollapsed = $derived(
    navigation.navigationCollapsed || compactViewport,
  );

  onMount(() => {
    const query = window.matchMedia("(max-width: 760px)");
    const update = (): void => {
      compactViewport = query.matches;
    };
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  });

  function activate(section: NavSection): void {
    navigation.section = section;
  }
</script>

<nav class:collapsed={effectivelyCollapsed} class="nav" aria-label="Primary navigation">
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
    <span class="group-label">Workspace</span>
    <ul class="sections">
      {#each primary as section (section.id)}
        <li>
          <Tooltip text={section.label} disabled={!effectivelyCollapsed}>
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

  <span class="group-label secondary-label">System</span>
  <ul class="sections secondary">
    {#each secondary as section (section.id)}
      <li>
        <Tooltip text={section.label} disabled={!effectivelyCollapsed}>
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
    width: 216px;
    min-width: 216px;
    padding: var(--space-2) var(--space-2) var(--space-3);
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
    border-radius: var(--radius-medium);
    color: var(--text-on-accent);
    background: var(--accent-default);
    font-family: var(--font-family-display);
    font-size: 16px;
    font-weight: 700;
  }
  .brand-copy { display: flex; flex-direction: column; min-width: 0; white-space: nowrap; }
  .brand-copy strong { font-size: 14px; line-height: 17px; }
  .brand-copy small { color: var(--text-tertiary); font-size: 11px; line-height: 14px; }
  .nav-scroll { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; }
  .group-label { display: block; margin: var(--space-4) var(--space-3) var(--space-1); color: var(--text-tertiary); font-size: 11px; font-weight: 600; letter-spacing: .08em; text-transform: uppercase; white-space: nowrap; }
  .secondary-label { margin-top: var(--space-2); }
  .sections { display: flex; flex-direction: column; gap: 2px; list-style: none; margin: 0; padding: 0; }
  .secondary { flex: none; }
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
  .section[aria-current="page"] { color: var(--text-primary); background: var(--bg-subtle-hover); font-weight: 600; }
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
  .collapsed .brand, .collapsed .group-label { display: none; }
  .collapsed .section { justify-content: center; padding: 0; gap: 0; }
  .collapsed .section-label { display: none; }
  .collapsed .sections { align-items: center; }
  @media (forced-colors: active) { .nav { backdrop-filter: none; } }
</style>
