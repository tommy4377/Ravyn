<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { systemAppearance } from "./lib/appearance/systemAppearance.svelte";
  import CompactApp from "./lib/compact/CompactApp.svelte";
  import SetupApp from "./lib/setup/SetupApp.svelte";
  import AppBackdrop from "./lib/shell/AppBackdrop.svelte";
  import AppShell from "./lib/shell/AppShell.svelte";
  import { navigation } from "./lib/stores/navigation.svelte";

  // The shell decides which window to open; the frontend routes on the label.
  const label = getCurrentWindow().label;

  // Theme/density/material resolution must run for every window — the setup
  // window otherwise stays light even when Windows uses a dark theme.
  navigation.init();

  onMount(() => systemAppearance.init());
</script>

<div class="app-root">
  <AppBackdrop />
  {#if label === "main"}
    <AppShell />
  {:else if label === "compact"}
    <CompactApp />
  {:else}
    <SetupApp />
  {/if}
</div>

<style>
  .app-root {
    position: relative;
    isolation: isolate;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: transparent;
  }
</style>
