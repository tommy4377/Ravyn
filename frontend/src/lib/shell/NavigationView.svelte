<script lang="ts">
  import { onMount } from "svelte";
  import ravynMark from "../../assets/ravyn-mark.svg";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import Tooltip from "../components/Tooltip.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { navigation, type NavSection } from "../stores/navigation.svelte";
  import { formatSpeed } from "../util/format";

  const primary: { id: NavSection; label: string; icon: IconName }[] = [
    { id: "downloads", label: "Downloads", icon: "download" },
    { id: "library", label: "Library", icon: "library" },
    { id: "media", label: "Media", icon: "video" },
    { id: "torrents", label: "Torrents", icon: "torrent" },
    { id: "automation", label: "Automation", icon: "automation" },
  ];

  const SPEED_HISTORY_LENGTH = 24;
  const totalDownSpeed = $derived(
    [...jobsStore.liveProgress.values()].reduce((sum, progress) => sum + Math.max(0, progress.bytesPerSecond), 0),
  );
  let speedHistory = $state<number[]>([]);
  const sparklinePoints = $derived(buildSparklinePoints(speedHistory));

  function buildSparklinePoints(samples: number[]): string {
    if (samples.length === 0) return "";
    const max = Math.max(1, ...samples);
    const step = samples.length > 1 ? 100 / (samples.length - 1) : 100;
    return samples.map((value, index) => `${(index * step).toFixed(1)},${(16 - (value / max) * 15).toFixed(1)}`).join(" ");
  }

  let compactViewport = $state(false);
  let overlayViewport = $state(false);
  const effectivelyCollapsed = $derived(navigation.navigationCollapsed || compactViewport);

  onMount(() => {
    const compactQuery = window.matchMedia("(max-width: 980px)");
    const overlayQuery = window.matchMedia("(max-width: 680px)");
    const update = (): void => {
      compactViewport = compactQuery.matches;
      overlayViewport = overlayQuery.matches;
      if (!overlayViewport) navigation.navigationOverlayOpen = false;
    };
    update();
    compactQuery.addEventListener("change", update);
    overlayQuery.addEventListener("change", update);
    return () => {
      compactQuery.removeEventListener("change", update);
      overlayQuery.removeEventListener("change", update);
    };
  });

  $effect(() => {
    const interval = setInterval(() => {
      speedHistory = [...speedHistory.slice(-(SPEED_HISTORY_LENGTH - 1)), totalDownSpeed];
    }, 1000);
    return () => clearInterval(interval);
  });

  function toggleNavigation(): void {
    if (overlayViewport) navigation.navigationOverlayOpen = !navigation.navigationOverlayOpen;
    else navigation.setNavigationCollapsed(!navigation.navigationCollapsed);
  }

  function activate(section: NavSection): void {
    navigation.navigate(section);
  }
</script>

{#if overlayViewport && navigation.navigationOverlayOpen}
  <button class="scrim" type="button" aria-label="Close navigation" onclick={() => (navigation.navigationOverlayOpen = false)}></button>
{/if}

<nav class:collapsed={effectivelyCollapsed} class:overlay-open={navigation.navigationOverlayOpen} class="nav" aria-label="Primary navigation">
  <div class="brand-row">
    <button class="pane-toggle" type="button" aria-label="Toggle navigation" onclick={toggleNavigation}>
      <Icon name="menu" size={18} />
    </button>
    <div class="brand" aria-label="Ravyn">
      <img class="mark" src={ravynMark} alt="" aria-hidden="true" />
      <span class="brand-copy"><strong>Ravyn</strong><small>Download manager</small></span>
    </div>
  </div>

  <div class="nav-scroll">
    <div class="nav-label">Transfer</div>
    <ul class="sections">
      {#each primary as section (section.id)}
        <li>
          <Tooltip text={section.label} disabled={!effectivelyCollapsed}>
            <button type="button" class="section" aria-current={navigation.section === section.id ? "page" : undefined} onclick={() => activate(section.id)}>
              <span class="indicator" aria-hidden="true"></span>
              <Icon name={section.icon} size={18} />
              <span class="section-label">{section.label}</span>
            </button>
          </Tooltip>
        </li>
      {/each}
    </ul>
  </div>

  <div class="nav-footer">
    <Tooltip text="Batch queue" disabled={!effectivelyCollapsed}>
      <button type="button" class="section" aria-expanded={navigation.basketDrawerOpen} onclick={() => navigation.openBasket()}>
        <span class="indicator" aria-hidden="true"></span>
        <Icon name="basket" size={18} />
        <span class="section-label">Batch queue</span>
      </button>
    </Tooltip>
    <Tooltip text="Settings" disabled={!effectivelyCollapsed}>
      <button type="button" class="section" aria-current={navigation.section === "settings" ? "page" : undefined} onclick={() => activate("settings")}>
        <span class="indicator" aria-hidden="true"></span>
        <Icon name="settings" size={18} />
        <span class="section-label">Settings</span>
      </button>
    </Tooltip>
  </div>

  <div class="throughput" aria-label="Aggregate download speed">
    <div class="throughput-row">
      <span class="dir">↓ Down</span>
      <span class="val">{formatSpeed(totalDownSpeed)}</span>
    </div>
    {#if !effectivelyCollapsed}
      <svg class="throughput-spark" viewBox="0 0 100 16" preserveAspectRatio="none" aria-hidden="true">
        <polyline points={sparklinePoints} fill="none" stroke="var(--accent-default)" stroke-width="1.6" />
      </svg>
    {/if}
  </div>
</nav>

<style>
  .nav { position: relative; z-index: 3; display: flex; flex-direction: column; width: 216px; min-width: 216px; padding: var(--space-2); border-right: 1px solid var(--stroke-divider); background: var(--surface-navigation); backdrop-filter: blur(32px) saturate(112%); -webkit-backdrop-filter: blur(32px) saturate(112%); transition: width var(--motion-normal) var(--motion-easing), min-width var(--motion-normal) var(--motion-easing), transform var(--motion-normal) var(--motion-easing); }
  .nav.collapsed { width: 56px; min-width: 56px; }
  .brand-row { display: flex; align-items: center; min-height: 52px; gap: var(--space-1); }
  .pane-toggle { display: grid; place-items: center; width: 40px; height: 40px; flex: none; border: 0; border-radius: var(--radius-control); background: transparent; cursor: default; }
  .pane-toggle:hover { background: var(--bg-subtle-hover); }
  .brand { display: flex; align-items: center; min-width: 0; gap: var(--space-2); overflow: hidden; }
  .mark { width: 28px; height: 28px; flex: none; border-radius: var(--radius-control); object-fit: contain; }
  .brand-copy { display: flex; flex-direction: column; min-width: 0; white-space: nowrap; }
  .brand-copy strong { font-size: 14px; line-height: 17px; }
  .brand-copy small { color: var(--text-tertiary); font-size: 11px; line-height: 14px; }
  .nav-scroll { flex: 1; min-height: 0; padding-top: var(--space-2); overflow-y: auto; overflow-x: hidden; }
  .nav-label { padding: var(--space-2) var(--space-3) 6px; color: var(--text-tertiary); font-family: var(--font-family-mono); font-size: 10px; font-weight: 600; letter-spacing: .12em; text-transform: uppercase; }
  .sections, .nav-footer { display: flex; flex-direction: column; gap: 2px; list-style: none; margin: 0; padding: 0; }
  .nav-footer { flex: none; padding-top: var(--space-2); border-top: 1px solid var(--stroke-divider); }
  .throughput { flex: none; margin-top: var(--space-2); padding: var(--space-2) var(--space-3); border-top: 1px solid var(--stroke-divider); display: flex; flex-direction: column; gap: 4px; }
  .throughput-row { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-2); font-family: var(--font-family-mono); }
  .throughput-row .dir { color: var(--text-tertiary); font-size: 10px; letter-spacing: .06em; text-transform: uppercase; }
  .throughput-row .val { color: var(--text-primary); font-size: 12.5px; font-variant-numeric: tabular-nums; }
  .throughput-spark { width: 100%; height: 16px; display: block; opacity: .85; }
  .collapsed .nav-label { display: none; }
  .collapsed .throughput-row .dir { display: none; }
  .collapsed .throughput { padding-inline: 0; text-align: center; }
  .sections li, .sections :global(.tooltip-wrapper), .nav-footer :global(.tooltip-wrapper) { width: 100%; }
  .section { position: relative; display: flex; align-items: center; width: 100%; height: 40px; gap: var(--space-3); padding: 0 var(--space-3); border: 0; border-radius: var(--radius-control); color: var(--text-secondary); background: transparent; cursor: default; overflow: hidden; text-align: left; }
  .section:hover { color: var(--text-primary); background: var(--bg-subtle-hover); }
  .section:active { background: var(--bg-subtle-pressed); }
  .section[aria-current="page"], .section[aria-expanded="true"] { color: var(--text-primary); background: var(--bg-subtle-hover); font-weight: 600; }
  .indicator { position: absolute; left: 0; width: 3px; height: 16px; border-radius: var(--radius-pill); background: transparent; transform: scaleY(.35); transition: transform var(--motion-fast) var(--motion-easing), background var(--motion-fast) var(--motion-easing); }
  .section[aria-current="page"] .indicator { background: var(--accent-default); transform: scaleY(1); }
  .section-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .collapsed .brand { display: none; }
  .collapsed .section { justify-content: center; padding: 0; gap: 0; }
  .collapsed .section-label { display: none; }
  .collapsed .sections, .collapsed .nav-footer { align-items: center; }
  .scrim { display: none; }
  @media (max-width: 680px) {
    .nav { position: absolute; inset: 0 auto 0 0; width: 216px; min-width: 216px; transform: translateX(-100%); box-shadow: var(--shadow-flyout); }
    .nav.overlay-open { transform: translateX(0); }
    .nav.collapsed { width: 216px; min-width: 216px; }
    .nav.collapsed .brand { display: flex; }
    .nav.collapsed .section { justify-content: flex-start; padding: 0 var(--space-3); gap: var(--space-3); }
    .nav.collapsed .section-label { display: inline; }
    .scrim { position: absolute; z-index: 2; inset: 0; display: block; border: 0; background: rgba(0, 0, 0, .34); }
  }
  @media (forced-colors: active) { .nav { backdrop-filter: none; } }
</style>
