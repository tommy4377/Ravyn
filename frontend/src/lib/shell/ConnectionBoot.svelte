<script lang="ts">
  import InlineError from "../components/InlineError.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import { connection } from "../stores/connection.svelte";
</script>

<div class="boot">
  <div class="box">
    {#if connection.status === "connecting"}
      <ProgressBar value={null} label="Connecting to the Ravyn backend" />
      <p>Connecting…</p>
    {:else}
      <InlineError
        title="Cannot reach the Ravyn backend"
        message={connection.errorMessage}
        retry={() => void connection.connect()}
      />
    {/if}
  </div>
</div>

<style>
  .boot {
    height: 100%;
    display: grid;
    place-items: center;
  }
  .box {
    width: min(460px, 85%);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    text-align: center;
    color: var(--text-secondary);
  }
</style>
