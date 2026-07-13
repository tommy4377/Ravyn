<script lang="ts">
  import Icon, { type IconName } from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import { navigation, type NavSection } from "../stores/navigation.svelte";

  const sections: { id: NavSection; label: string; icon: IconName }[] = [
    { id: "downloads", label: "Downloads", icon: "download" },
    // Library/Basket/Automation/Components/Settings/Diagnostics are not
    // wired to a real connected screen yet — per the frontend plan, unbuilt
    // sections are left out of navigation rather than shown as placeholders.
  ];

  function cycleTheme(): void {
    const order = ["system", "light", "dark"] as const;
    const next = order[(order.indexOf(navigation.theme) + 1) % order.length] ?? "system";
    navigation.setTheme(next);
  }

  function toggleDensity(): void {
    navigation.setDensity(navigation.density === "comfortable" ? "compact" : "comfortable");
  }
</script>

<nav class="nav" aria-label="Primary">
  <div class="brand">
    <span class="mark" aria-hidden="true">R</span>
    <span class="name">Ravyn</span>
  </div>

  <ul class="sections">
    {#each sections as section (section.id)}
      <li>
        <button
          type="button"
          class="section"
          aria-current={navigation.section === section.id ? "page" : undefined}
          onclick={() => (navigation.section = section.id)}
        >
          <Icon name={section.icon} size={16} />
          <span>{section.label}</span>
        </button>
      </li>
    {/each}
  </ul>

  <div class="footer">
    <IconButton
      icon={navigation.density === "compact" ? "list" : "grid"}
      label={navigation.density === "compact" ? "Switch to comfortable density" : "Switch to compact density"}
      variant="subtle"
      onclick={toggleDensity}
    />
    <IconButton
      icon="info"
      label={`Theme: ${navigation.theme} (click to change)`}
      variant="subtle"
      onclick={cycleTheme}
    />
  </div>
</nav>

<style>
  .nav {
    display: flex;
    flex-direction: column;
    width: 216px;
    flex: none;
    background: var(--bg-layer-alt);
    border-right: 1px solid var(--stroke-divider);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-4) var(--space-3);
  }
  .mark {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-medium);
    background: var(--accent-default);
    color: var(--text-on-accent);
    font-weight: 700;
    font-size: 13px;
  }
  .name {
    font-size: var(--text-body-strong);
    font-weight: 600;
  }
  .sections {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .section {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: var(--control-large);
    padding: 0 var(--space-3);
    border: none;
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: var(--text-body);
    cursor: default;
  }
  .section:hover {
    background: var(--bg-subtle-hover);
    color: var(--text-primary);
  }
  .section[aria-current="page"] {
    background: var(--accent-subtle);
    color: var(--accent-text);
    font-weight: 600;
  }
  .footer {
    margin-top: auto;
    display: flex;
    gap: var(--space-1);
    padding: var(--space-3);
    border-top: 1px solid var(--stroke-divider);
  }
</style>
