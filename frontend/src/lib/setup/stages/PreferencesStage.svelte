<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import Checkbox from "../../components/Checkbox.svelte";
  import RadioGroup from "../../components/RadioGroup.svelte";
  import type { InstallationMode } from "../../api/types";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  const applicationOptions = $derived(
    controller.installation?.development
      ? [
          {
            value: "development",
            label: "Development mode",
            description:
              "Run this debug build in place without registering it as an installed application.",
          },
        ]
      : [
          {
            value: "installed",
            label: "Install for this user",
            description:
              "Copy Ravyn to your local Programs folder and register it with Windows.",
          },
          {
            value: "portable",
            label: "Keep this copy portable",
            description:
              "Run Ravyn from its current location without Installed Apps registration or shortcuts.",
          },
        ],
  );

  function next() {
    controller.step = "install";
    void controller.runInstallation();
  }
</script>

<StageShell
  title="Installation preferences"
  subtitle="Choose how Ravyn should live on this PC. These choices are verified before setup can finish."
  onback={() => (controller.step = "library")}
  onnext={next}
  nextLabel="Install"
>
  <div class="content">
    <RadioGroup
      legend="Application mode"
      options={applicationOptions}
      value={controller.applicationMode}
      onchange={(value) =>
        controller.setApplicationMode(value as InstallationMode)}
    />

    <div class="list" aria-label="Windows integration options">
      <Checkbox
        label="Create a Start Menu shortcut"
        bind:checked={controller.startMenuShortcut}
        disabled={controller.applicationMode !== "installed"}
      />
      <Checkbox
        label="Create a desktop shortcut"
        bind:checked={controller.desktopShortcut}
        disabled={controller.applicationMode !== "installed"}
      />
      <Checkbox
        label="Start Ravyn when Windows starts"
        bind:checked={controller.launchAtStartup}
        disabled={controller.applicationMode !== "installed"}
      />
      <Checkbox
        label="Open Ravyn after setup"
        bind:checked={controller.launchAfterSetup}
      />
    </div>
  </div>
</StageShell>

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding-top: var(--space-4);
    border-top: 1px solid var(--stroke-divider);
  }
</style>
