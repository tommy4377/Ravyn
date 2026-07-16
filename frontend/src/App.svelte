<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { systemAppearance } from "./lib/appearance/systemAppearance.svelte";
  import SetupApp from "./lib/setup/SetupApp.svelte";
  import AppBackdrop from "./lib/shell/AppBackdrop.svelte";
  import AppShell from "./lib/shell/AppShell.svelte";

  // The shell decides which window to open; the frontend routes on the label.
  const label = getCurrentWindow().label;

  onMount(() => systemAppearance.init());
</script>

<div class="app-root">
  <AppBackdrop />
  {#if label === "main"}
    <AppShell />
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
