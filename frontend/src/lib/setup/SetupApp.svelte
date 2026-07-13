<script lang="ts">
  import { SetupController } from "./controller.svelte";
  import InlineError from "../components/InlineError.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import WelcomeStage from "./stages/WelcomeStage.svelte";
  import SetupTypeStage from "./stages/SetupTypeStage.svelte";
  import FeaturesStage from "./stages/FeaturesStage.svelte";
  import LibraryStage from "./stages/LibraryStage.svelte";
  import PreferencesStage from "./stages/PreferencesStage.svelte";
  import InstallStage from "./stages/InstallStage.svelte";
  import CompletionStage from "./stages/CompletionStage.svelte";

  const controller = new SetupController();

  $effect(() => {
    void controller.init();
    return () => controller.events?.close();
  });
</script>

<div class="setup" role="main">
  {#if controller.loading}
    <div class="loading" aria-live="polite">
      <div class="loading-inner">
        <ProgressBar value={null} label="Starting Ravyn" />
        <p>Starting Ravyn…</p>
      </div>
    </div>
  {:else if controller.connectionError}
    <div class="loading">
      <div class="loading-inner">
        <InlineError
          title="Ravyn could not start its background service"
          message={controller.connectionError}
          retry={() => void controller.init()}
        />
      </div>
    </div>
  {:else if controller.step === "welcome"}
    <WelcomeStage {controller} />
  {:else if controller.step === "setup-type"}
    <SetupTypeStage {controller} />
  {:else if controller.step === "features"}
    <FeaturesStage {controller} />
  {:else if controller.step === "library"}
    <LibraryStage {controller} />
  {:else if controller.step === "preferences"}
    <PreferencesStage {controller} />
  {:else if controller.step === "install"}
    <InstallStage {controller} />
  {:else if controller.step === "done"}
    <CompletionStage {controller} />
  {/if}
</div>

<style>
  .setup {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-app);
  }
  .loading {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .loading-inner {
    width: min(420px, 80%);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    text-align: center;
    color: var(--text-secondary);
  }
</style>
