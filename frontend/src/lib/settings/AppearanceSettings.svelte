<script lang="ts">
  import AdvancedDisclosure from "../components/AdvancedDisclosure.svelte";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import Surface from "../components/Surface.svelte";
  import { systemAppearance } from "../appearance/systemAppearance.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";

  let backdropDraft = $state(navigation.backdropImage);
  let intensityDraft = $state(String(navigation.materialIntensity));
  let backdropStatus = $derived.by(() => {
    if (!systemAppearance.supported) return "Available in the installed Windows application.";
    if (navigation.material === "solid") return "Solid material is selected; compositor blur is off.";
    if (navigation.backdropImage) return "A custom image is replacing the compositor backdrop.";
    if (!systemAppearance.transparencyEnabled) {
      if (systemAppearance.wallpaperAvailable) {
        return `Transparency is disabled; using the Windows wallpaper fallback (${systemAppearance.wallpaperPosition}).`;
      }
      return "Transparency is disabled and Windows did not expose a usable fallback wallpaper.";
    }
    if (systemAppearance.nativeBackdrop) {
      return "Active — Windows is blurring the composed content behind Ravyn.";
    }
    if (systemAppearance.wallpaperAvailable) {
      return `This Windows version has no compositor backdrop; Ravyn renders the material from the desktop wallpaper (${systemAppearance.wallpaperPosition}).`;
    }
    return "This Windows version has no compositor backdrop and no usable wallpaper; using the solid fallback.";
  });
</script>

<SettingsCategoryHeader title="Appearance" description="Appearance changes apply immediately and are stored on this device." />
<Surface padding="none">
  <div class="row"><div><strong>App theme</strong><span>System follows the current Windows light or dark preference.</span></div><div class="choice-group" aria-label="App theme"><button class:active={navigation.theme === "system"} onclick={() => navigation.setTheme("system")}>System</button><button class:active={navigation.theme === "light"} onclick={() => navigation.setTheme("light")}><Icon name="sun" size={14} /> Light</button><button class:active={navigation.theme === "dark"} onclick={() => navigation.setTheme("dark")}><Icon name="moon" size={14} /> Dark</button></div></div>
  <div class="row"><div><strong>Window material</strong><span>Acrylic uses the Windows 11 compositor backdrop; on Windows 10 an equivalent material is rendered from the desktop wallpaper.</span></div><div class="choice-group"><button class:active={navigation.material === "synthetic"} onclick={() => navigation.setMaterial("synthetic")}>Acrylic</button><button class:active={navigation.material === "solid"} onclick={() => navigation.setMaterial("solid")}>Solid</button></div></div>
  <div class="row"><div><strong>Windows compositor backdrop</strong><span>{backdropStatus}</span>{#if systemAppearance.lastError}<small class="warning">{systemAppearance.lastError}</small>{/if}{#if systemAppearance.viewportMismatch}<small class="warning">{systemAppearance.viewportMismatch}</small>{/if}</div><Button disabled={systemAppearance.refreshing} onclick={() => void systemAppearance.refresh()}>{systemAppearance.refreshing ? "Refreshing…" : "Refresh"}</Button></div>
  <div class="row align-start"><div><strong>Material intensity</strong><span>Controls the compositor tint, fallback wallpaper, glow, and texture strength.</span></div><div class="range-control"><input type="range" min="0" max="100" bind:value={intensityDraft} oninput={() => navigation.setMaterialIntensity(Number(intensityDraft))} /><output>{intensityDraft}%</output></div></div>
  <div class="row"><div><strong>Content density</strong><span>Compact fits more rows; comfortable provides larger targets.</span></div><div class="choice-group"><button class:active={navigation.density === "comfortable"} onclick={() => navigation.setDensity("comfortable")}>Comfortable</button><button class:active={navigation.density === "compact"} onclick={() => navigation.setDensity("compact")}><Icon name="compact" size={14} /> Compact</button></div></div>
  <AdvancedDisclosure title="Custom backdrop" description="Replace the compositor backdrop with an image URL or asset URI.">
    <div class="backdrop-field"><input type="text" bind:value={backdropDraft} placeholder="https://… or asset URI" /><Button onclick={() => navigation.setBackdropImage(backdropDraft)}>Apply</Button></div>
  </AdvancedDisclosure>
</Surface>

<style>
  .row { min-height: 72px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-6); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .row > div:first-child { display: flex; flex-direction: column; max-width: 600px; }
  .row span, .row small { color: var(--text-secondary); font-size: var(--text-caption); }
  .row small.warning { color: var(--status-warning); }
  .align-start { align-items: flex-start; }
  .choice-group { display: inline-flex; flex: none; padding: 2px; border: 1px solid var(--stroke-control); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .choice-group button { height: 30px; display: flex; align-items: center; gap: 5px; padding: 0 var(--space-3); border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; font: inherit; }
  .choice-group button.active { color: var(--text-primary); background: var(--surface-card-hover); }
  .range-control { min-width: 250px; display: flex; align-items: center; gap: var(--space-3); }
  .range-control input { flex: 1; accent-color: var(--accent-default); }
  .range-control output { width: 42px; text-align: right; color: var(--text-secondary); }
  .backdrop-field { display: flex; gap: var(--space-2); padding-right: var(--space-4); }
  .backdrop-field input { flex: 1; min-width: 0; height: var(--control-default); padding: 0 var(--space-3); border: 1px solid var(--stroke-control); border-bottom-color: var(--stroke-control-strong); border-radius: var(--radius-control); color: var(--text-primary); background: var(--bg-control); font: inherit; }
  @media (max-width: 700px) { .row { align-items: stretch; flex-direction: column; gap: var(--space-3); } .choice-group, .range-control { width: 100%; } }
</style>
