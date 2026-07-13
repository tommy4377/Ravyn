<script lang="ts">
  import StageShell from "../StageShell.svelte";
  import Checkbox from "../../components/Checkbox.svelte";
  import InlineError from "../../components/InlineError.svelte";
  import { FEATURE_UI } from "../componentStates";
  import type { SetupController } from "../controller.svelte";

  let { controller }: { controller: SetupController } = $props();

  // Real feature catalog from the backend, in backend display order.
  const catalog = $derived(controller.overview?.features ?? []);

  // Unsupported components must be shown, not hidden (plan §4.3).
  const unsupported = $derived(
    (controller.overview?.components ?? []).filter(
      (c) => c.state === "unsupported",
    ),
  );

  async function next() {
    if (await controller.saveFeatures()) {
      controller.step = "library";
    }
  }
</script>

<StageShell
  title="Choose features"
  subtitle="Features are installed as verified components. Nothing you disable is installed."
  busy={controller.busy}
  onback={() => (controller.step = "setup-type")}
  onnext={next}
>
  {#if controller.stepError}
    <InlineError
      title="Your selection could not be saved"
      message={controller.stepError}
      retry={next}
    />
  {/if}

  <div class="list" role="group" aria-label="Available features">
    {#each catalog as feature (feature.feature)}
      {@const ui = FEATURE_UI[feature.feature]}
      <Checkbox
        label={ui.title}
        description={ui.description +
          (ui.engine ? ` Powered by ${ui.engine}.` : "")}
        checked={controller.features.has(feature.feature)}
        disabled={ui.locked}
        onchange={(checked) =>
          controller.toggleFeature(feature.feature, checked)}
      />
    {/each}
  </div>

  {#if unsupported.length > 0}
    <p class="unsupported" role="note">
      Not available on this device ({controller.overview?.platform}):
      {unsupported.map((c) => c.component).join(", ")}.
    </p>
  {/if}
</StageShell>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .unsupported {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--status-warning);
  }
</style>
