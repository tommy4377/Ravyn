<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import Checkbox from "../../components/Checkbox.svelte";
  import RadioGroup from "../../components/RadioGroup.svelte";
  import TextField from "../../components/TextField.svelte";
  import ToggleSwitch from "../../components/ToggleSwitch.svelte";
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

  async function next() {
    if (!(await controller.savePreferences())) return;
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

    <div class="download-preferences" aria-label="Download preferences">
      <div class="section-heading">
        <strong>Download defaults</strong>
        <span>These can be changed later in Settings.</span>
      </div>
      <ToggleSwitch
        bind:checked={controller.autoOrganize}
        label="Organize completed downloads automatically"
        description="Route files into Videos, Music, Documents, Images, Archives, and other Library folders."
      />
      <ToggleSwitch
        bind:checked={controller.autoProvision}
        label="Maintain selected tools automatically"
        description="Download and update checksum-verified managed tools required by enabled features."
      />
      <div class="download-grid">
        <TextField bind:value={controller.maxActive} label="Active downloads" inputmode="numeric" />
        <TextField bind:value={controller.speedLimitMbps} label="Global speed limit (Mbit/s)" inputmode="decimal" placeholder="0 for unlimited" />
      </div>
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
  .download-preferences { display: flex; flex-direction: column; gap: var(--space-2); padding-top: var(--space-4); border-top: 1px solid var(--stroke-divider); }
  .section-heading { display: flex; flex-direction: column; gap: var(--space-1); margin-bottom: var(--space-2); }
  .section-heading span { color: var(--text-secondary); font-size: var(--text-caption); }
  .download-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-4); margin-top: var(--space-3); }
  @media (max-width: 620px) { .download-grid { grid-template-columns: 1fr; } }
</style>
