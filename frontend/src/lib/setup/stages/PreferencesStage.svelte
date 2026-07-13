<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import Checkbox from "../../components/Checkbox.svelte";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  function next() {
    controller.step = "install";
    // Kick off the real installation as soon as the stage is entered.
    void controller.runInstallation();
  }
</script>

<StageShell
  title="A few preferences"
  subtitle="Everything here can be changed later in Settings."
  onback={() => (controller.step = "library")}
  onnext={next}
  nextLabel="Install"
>
  <div class="list">
    <Checkbox
      label="Create a Start Menu shortcut"
      bind:checked={controller.startMenuShortcut}
    />
    <Checkbox
      label="Create a desktop shortcut"
      bind:checked={controller.desktopShortcut}
    />
    <Checkbox
      label="Start Ravyn when Windows starts"
      bind:checked={controller.launchAtStartup}
    />
    <Checkbox
      label="Open Ravyn after setup"
      bind:checked={controller.launchAfterSetup}
    />
  </div>
</StageShell>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
</style>
