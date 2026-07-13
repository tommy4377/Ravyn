<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import SetupApp from "./lib/setup/SetupApp.svelte";
  import AppShell from "./lib/shell/AppShell.svelte";

  // The shell decides which window to open; the frontend routes on the label.
  const label = getCurrentWindow().label;
</script>

{#if label === "main"}
  <AppShell />
{:else}
  <SetupApp />
{/if}
