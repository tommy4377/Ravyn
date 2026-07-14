<script lang="ts">
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import EmptyState from "../components/EmptyState.svelte";
  import Icon from "../components/Icon.svelte";
  import InlineError from "../components/InlineError.svelte";
  import TextField from "../components/TextField.svelte";
  import type { AutomationController } from "./automationController.svelte";

  let { controller }: { controller: AutomationController } = $props();
</script>

<Dialog open={controller.previewOpen} title="Test rules" size="large" onClose={() => !controller.previewBusy && (controller.previewOpen = false)} preventClose={controller.previewBusy}>
  <div class="form">
    <TextField bind:value={controller.previewUrl} label="URL to test" placeholder="https://example.com/videos/movie.mkv" />
    <div class="two-column">
      <TextField bind:value={controller.previewExtension} label="File extension" placeholder="mkv (optional)" />
      <TextField bind:value={controller.previewMime} label="MIME type" placeholder="video/x-matroska (optional)" />
    </div>

    {#if controller.previewError}
      <InlineError title="Couldn't test the rules" message={controller.previewError} retry={() => void controller.runRulePreview()} />
    {:else if controller.previewResult}
      <div class="comparison">
        <section>
          <span class="section-label">Before</span>
          <dl><dt>Destination</dt><dd>Library default</dd><dt>Tags</dt><dd>None</dd><dt>Speed limit</dt><dd>Unlimited</dd></dl>
        </section>
        <span class="arrow"><Icon name="chevron-right" size={18} /></span>
        <section class="after">
          <span class="section-label">After rules</span>
          <dl>
            <dt>Destination</dt><dd>{controller.previewResult.result.destination ?? "Library default"}</dd>
            <dt>Tags</dt><dd>{controller.previewResult.result.options?.tags?.length ? controller.previewResult.result.options.tags.join(", ") : "None"}</dd>
            <dt>Speed limit</dt><dd>{controller.previewResult.result.speed_limit_bps ? `${Math.round(controller.previewResult.result.speed_limit_bps / 125000 * 10) / 10} Mbit/s` : "Unlimited"}</dd>
          </dl>
        </section>
      </div>

      {#if controller.previewResult.matches.length === 0}
        <EmptyState icon="rule" title="No rules match" message="The download keeps its original settings." />
      {:else}
        <div class="matches">
          <h3>Applied rules</h3>
          {#each controller.previewResult.matches as match (match.id)}
            <div class="match"><Icon name="check-circle" size={16} /><div><strong>{match.name}</strong><span>Priority {match.priority}{match.destination_shadowed ? " · destination overridden by a higher-priority rule" : ""}{match.speed_limit_shadowed ? " · speed limit overridden" : ""}</span></div></div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
  {#snippet footer()}
    <Button disabled={controller.previewBusy} onclick={() => (controller.previewOpen = false)}>Close</Button>
    <Button variant="accent" disabled={controller.previewBusy || !controller.previewUrl.trim()} onclick={() => void controller.runRulePreview()}>{controller.previewBusy ? "Testing…" : "Run test"}</Button>
  {/snippet}
</Dialog>

<style>
  .form { display: flex; flex-direction: column; gap: var(--space-4); }
  .two-column { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-3); }
  .comparison { display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: stretch; gap: var(--space-3); }
  .comparison section { padding: var(--space-4); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .comparison section.after { border-color: color-mix(in srgb, var(--accent-default) 34%, var(--stroke-divider)); background: color-mix(in srgb, var(--accent-subtle) 42%, var(--surface-content)); }
  .section-label { display: block; margin-bottom: var(--space-3); color: var(--text-tertiary); font-size: var(--text-caption); font-weight: 700; text-transform: uppercase; letter-spacing: .04em; }
  .arrow { display: grid; place-items: center; color: var(--text-tertiary); }
  dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-2) var(--space-3); margin: 0; }
  dt { color: var(--text-secondary); } dd { margin: 0; overflow-wrap: anywhere; }
  .matches { display: flex; flex-direction: column; gap: var(--space-2); }
  .matches h3 { margin: 0; font-size: var(--text-body-strong); }
  .match { display: flex; align-items: flex-start; gap: var(--space-2); padding: var(--space-2) 0; border-bottom: 1px solid var(--stroke-divider); }
  .match > div { display: flex; flex-direction: column; }
  .match span { color: var(--text-tertiary); font-size: var(--text-caption); }
  @media (max-width: 700px) { .two-column, .comparison { grid-template-columns: 1fr; } .arrow { transform: rotate(90deg); } }
</style>
