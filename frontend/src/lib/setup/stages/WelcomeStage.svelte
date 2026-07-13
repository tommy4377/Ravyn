<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  const modeText = $derived.by(() => {
    switch (controller.mode) {
      case "update":
        return `Ravyn ${controller.installation?.installed_version ?? ""} is already installed. This setup updates it to ${controller.installation?.app_version}.`;
      case "repair":
        return "Ravyn is present but setup did not finish. You can complete or repair it now.";
      default:
        return "A modern download manager for files, media, archives, and torrents.";
    }
  });

  const nextLabel = $derived(
    controller.mode === "update"
      ? "Update"
      : controller.mode === "repair"
        ? "Repair"
        : "Get started",
  );
</script>

<StageShell
  title="Welcome to Ravyn"
  subtitle={modeText}
  showBack={false}
  {nextLabel}
  onnext={() => (controller.step = "setup-type")}
>
  <div class="facts">
    <dl>
      <dt>Version</dt>
      <dd>{controller.installation?.app_version ?? "—"}</dd>
      {#if controller.installation?.portable}
        <dt>Mode</dt>
        <dd>Portable — running outside the installed location</dd>
      {/if}
      {#if controller.installation?.development}
        <dt>Build</dt>
        <dd>Development</dd>
      {/if}
      <dt>Data folder</dt>
      <dd>{controller.setupState?.data_dir ?? "—"}</dd>
    </dl>
  </div>
</StageShell>

<style>
  .facts {
    margin-top: auto;
    padding-top: var(--space-6);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-1) var(--space-4);
    margin: 0;
    font-size: var(--text-caption);
    color: var(--text-tertiary);
  }
  dt {
    font-weight: 600;
  }
  dd {
    margin: 0;
    word-break: break-all;
  }
</style>
