<script lang="ts">
  import Tooltip from "../lib/components/Tooltip.svelte";
</script>

<Tooltip text="Helpful description">
  {#snippet children()}
    <button type="button" aria-describedby="existing-description">Target</button>
  {/snippet}
</Tooltip>
<span id="existing-description">Existing description</span>
