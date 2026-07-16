<script lang="ts">
  import Dialog from "../lib/components/Dialog.svelte";

  let { onClose = () => {} }: { onClose?: () => void } = $props();
</script>

<Dialog open title="First dialog" {onClose}>
  {#snippet children()}<button type="button">First action</button>{/snippet}
</Dialog>
<Dialog open title="Second dialog" {onClose}>
  {#snippet children()}<button type="button">Second action</button>{/snippet}
</Dialog>
