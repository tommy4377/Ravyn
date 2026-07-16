<script lang="ts">
  import ravynMark from "../../assets/ravyn-mark.svg";
  import Button from "../components/Button.svelte";
  import Icon from "../components/Icon.svelte";
  import Surface from "../components/Surface.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import SettingsCategoryHeader from "./SettingsCategoryHeader.svelte";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();

  async function copySystemInformation(): Promise<void> {
    const information = [
      `Ravyn ${controller.installation?.app_version ?? controller.updateStatus?.current_version ?? "unknown"}`,
      `Mode: ${controller.installation?.portable ? "Portable" : controller.installation?.installed ? "Installed" : controller.installation?.development ? "Development" : "Unknown"}`,
      `Executable: ${controller.installation?.exe_path ?? "Unknown"}`,
      `Install directory: ${controller.installation?.install_dir ?? "Not installed"}`,
      `Data directory: ${controller.backend?.data_dir ?? "Unknown"}`,
      `Backend: ${controller.backend?.base_url ?? "Unavailable"}`,
      `Update phase: ${controller.updateStatus?.phase ?? "Unavailable"}`,
    ].join("\n");
    await navigator.clipboard.writeText(information);
    notifications.success("System information copied");
  }
</script>

<SettingsCategoryHeader title="About" description="Version, installation, update channel, paths, and support information." />
<Surface padding="none">
  <div class="hero">
    <img class="logo" src={ravynMark} alt="" />
    <div><h3>Ravyn</h3><p>A focused download manager for Windows.</p></div>
    <Button onclick={() => void copySystemInformation()}><Icon name="copy" size={15} /> Copy system information</Button>
  </div>
  <dl>
    <dt>Version</dt><dd>{controller.installation?.app_version ?? controller.updateStatus?.current_version ?? "Unknown"}</dd>
    <dt>Update channel</dt><dd>{controller.updateStatus?.automatic ? "Automatic stable updates" : "Manual or unavailable"}</dd>
    <dt>Install mode</dt><dd>{controller.installation?.portable ? "Portable" : controller.installation?.installed ? "Installed" : controller.installation?.development ? "Development" : "Unknown"}</dd>
    <dt>Executable</dt><dd class="path">{controller.installation?.exe_path ?? "Unknown"}</dd>
    <dt>Install directory</dt><dd class="path">{controller.installation?.install_dir ?? "Not installed"}</dd>
    <dt>Data directory</dt><dd class="path">{controller.backend?.data_dir ?? "Unknown"}</dd>
    <dt>Backend address</dt><dd class="path">{controller.backend?.base_url ?? "Unavailable"}</dd>
  </dl>
</Surface>

<style>
  .hero { min-height: 100px; display: grid; grid-template-columns: 54px minmax(0, 1fr) auto; align-items: center; gap: var(--space-4); padding: var(--space-5); border-bottom: 1px solid var(--stroke-divider); }
  .logo { width: 52px; height: 52px; border-radius: var(--radius-layer); object-fit: contain; }
  h3, p { margin: 0; } h3 { font-size: var(--text-subtitle); } p { margin-top: 3px; color: var(--text-secondary); }
  dl { display: grid; grid-template-columns: minmax(130px, 190px) minmax(0, 1fr); gap: 0; margin: 0; }
  dt, dd { min-height: 48px; display: flex; align-items: center; padding: var(--space-2) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  dt { color: var(--text-secondary); } dd { margin: 0; }
  .path { min-width: 0; overflow-wrap: anywhere; font: 12px/1.5 ui-monospace, "Cascadia Code", Consolas, monospace; }
  @media (max-width: 680px) { .hero { grid-template-columns: 54px 1fr; } .hero :global(button) { grid-column: 1 / -1; } dl { grid-template-columns: 1fr; } dt { min-height: 32px; padding-bottom: 0; border-bottom: 0; } }
</style>
