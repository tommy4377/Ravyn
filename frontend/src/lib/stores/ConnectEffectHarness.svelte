<script lang="ts">
  /**
   * Test-only harness: reproduces AppShell's `$effect(() => { void
   * connection.connect(); })` boot pattern so connection.svelte.test.ts can
   * verify connect() doesn't cause that effect to re-run itself.
   */
  import { connection } from "./connection.svelte";

  let { onRun }: { onRun: () => void } = $props();

  $effect(() => {
    onRun();
    void connection.connect();
  });
</script>
