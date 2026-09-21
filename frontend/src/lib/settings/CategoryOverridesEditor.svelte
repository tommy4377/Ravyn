<script lang="ts">
  import Button from "../components/Button.svelte";
  import Dropdown, { type DropdownOption } from "../components/Dropdown.svelte";
  import Icon from "../components/Icon.svelte";
  import IconButton from "../components/IconButton.svelte";
  import TextField from "../components/TextField.svelte";
  import type { LibraryCategory } from "../api/types";
  import type { SettingsController } from "./settingsController.svelte";

  let { controller }: { controller: SettingsController } = $props();

  const categories: DropdownOption[] = [
    { value: "downloads", label: "Downloads" },
    { value: "videos", label: "Videos" },
    { value: "music", label: "Music" },
    { value: "documents", label: "Documents" },
    { value: "images", label: "Images" },
    { value: "archives", label: "Archives" },
    { value: "torrents", label: "Torrents" },
    { value: "playlists", label: "Playlists" },
    { value: "temporary", label: "Temporary" },
    { value: "other", label: "Other" },
  ];
</script>

<div class="override-editor">
  <div class="override-header">
    <div>
      <strong>Extension overrides</strong>
      <span>Route specific file extensions to a chosen Library category before automatic classification.</span>
    </div>
    <Button variant="subtle" onclick={() => controller.addCategoryOverride()}>
      <Icon name="add" size={15} /> Add override
    </Button>
  </div>

  {#if controller.categoryOverrides.length === 0}
    <div class="empty-overrides">
      <Icon name="folder-open" size={18} />
      <span>No custom extension routes. Ravyn uses its built-in MIME and extension classifier.</span>
    </div>
  {:else}
    <div class="override-list">
      {#each controller.categoryOverrides as override, index (index)}
        <div class="override-row">
          <TextField bind:value={override.extension} label="Extension" placeholder="mkv" />
          <div class="category-field">
            <span>Category</span>
            <Dropdown
              options={categories}
              value={override.category}
              label={`Category for extension override ${index + 1}`}
              onchange={(value) => (override.category = value as LibraryCategory)}
            />
          </div>
          <IconButton icon="trash" label={`Remove extension override ${index + 1}`} variant="subtle" onclick={() => controller.removeCategoryOverride(index)} />
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .override-editor { display: flex; flex-direction: column; gap: var(--space-4); padding-right: var(--space-4); }
  .override-header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); }
  .override-header > div { display: flex; flex-direction: column; gap: var(--space-1); }
  .override-header span, .empty-overrides { color: var(--text-secondary); font-size: var(--text-caption); }
  .override-list { display: flex; flex-direction: column; gap: var(--space-3); }
  .override-row { display: grid; grid-template-columns: minmax(140px, .65fr) minmax(180px, 1fr) auto; align-items: end; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--stroke-divider); border-radius: var(--radius-medium); background: var(--bg-subtle); }
  .category-field { display: flex; flex-direction: column; gap: var(--space-1); font-size: var(--text-body); }
  .category-field :global(.dropdown) { min-width: 0; }
  .empty-overrides { min-height: 54px; display: flex; align-items: center; gap: var(--space-2); padding: var(--space-3); border: 1px dashed var(--stroke-control); border-radius: var(--radius-medium); }
  @media (max-width: 680px) { .override-header { align-items: stretch; flex-direction: column; } .override-row { grid-template-columns: 1fr auto; } .override-row :global(.field:first-child) { grid-column: 1 / -1; } }
</style>
