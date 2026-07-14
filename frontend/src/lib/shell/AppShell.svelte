<script lang="ts">
  import { onMount } from "svelte";
  import { systemAppearance } from "../appearance/systemAppearance.svelte";
  import AutomationView from "../automation/AutomationView.svelte";
  import BasketView from "../basket/BasketView.svelte";
  import ComponentsView from "../components/ComponentsView.svelte";
  import DiagnosticsView from "../diagnostics/DiagnosticsView.svelte";
  import DownloadsView from "../downloads/DownloadsView.svelte";
  import JobDetailsPane from "../downloads/JobDetailsPane.svelte";
  import LibraryView from "../library/LibraryView.svelte";
  import MediaView from "../media/MediaView.svelte";
  import SettingsView from "../settings/SettingsView.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import TorrentsView from "../torrents/TorrentsView.svelte";
  import AppBackdrop from "./AppBackdrop.svelte";
  import ConnectionBoot from "./ConnectionBoot.svelte";
  import NavigationView from "./NavigationView.svelte";
  import NotificationHost from "./NotificationHost.svelte";
  import StatusBar from "./StatusBar.svelte";

  navigation.init();
  onMount(() => systemAppearance.init());

  let detailsWidth = $state(loadDetailsWidth());
  let resizing = $state(false);

  $effect(() => {
    void connection.connect();
  });

  $effect(() => {
    if (connection.status !== "ready" || !connection.client || !connection.events) return;
    jobsStore.init(new JobsService(connection.client));
    const unsubscribe = connection.events.subscribe((event) => jobsStore.applyEvent(event));
    return () => {
      unsubscribe();
      jobsStore.dispose();
    };
  });

  function loadDetailsWidth(): number {
    const value = Number(localStorage.getItem("ravyn.detailsWidth") ?? 380);
    return Number.isFinite(value) ? Math.min(520, Math.max(320, value)) : 380;
  }

  function beginResize(event: PointerEvent): void {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = detailsWidth;
    resizing = true;

    const move = (moveEvent: PointerEvent): void => {
      detailsWidth = Math.min(520, Math.max(320, startWidth + startX - moveEvent.clientX));
    };
    const end = (): void => {
      resizing = false;
      localStorage.setItem("ravyn.detailsWidth", String(Math.round(detailsWidth)));
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
    };

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end, { once: true });
  }
</script>

{#if connection.status !== "ready"}
  <ConnectionBoot />
{:else}
  <div class="shell" class:resizing>
    <AppBackdrop />
    <div class="body">
      <NavigationView />
      <main class="content-surface" aria-live="polite">
        {#if navigation.section === "downloads"}
          <DownloadsView />
        {:else if navigation.section === "library"}
          <LibraryView />
        {:else if navigation.section === "media"}
          <MediaView />
        {:else if navigation.section === "torrents"}
          <TorrentsView />
        {:else if navigation.section === "basket"}
          <BasketView />
        {:else if navigation.section === "automation"}
          <AutomationView />
        {:else if navigation.section === "components"}
          <ComponentsView />
        {:else if navigation.section === "settings"}
          <SettingsView />
        {:else if navigation.section === "diagnostics"}
          <DiagnosticsView />
        {/if}
      </main>

      {#if navigation.section === "downloads" && navigation.detailsPaneOpen && navigation.selectedJobId}
        <aside class="details-region" style:width={`${detailsWidth}px`}>
          <button
            type="button"
            class="resize-handle"
            aria-label="Resize details pane"
            onpointerdown={beginResize}
          ></button>
          <JobDetailsPane jobId={navigation.selectedJobId} onClose={() => navigation.selectJob(null)} />
        </aside>
      {/if}
    </div>
    <StatusBar />
  </div>
{/if}

<NotificationHost />

<style>
  .shell {
    position: relative;
    isolation: isolate;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: transparent;
  }
  .shell.resizing,
  .shell.resizing * {
    cursor: col-resize !important;
    user-select: none !important;
  }
  .body {
    position: relative;
    z-index: 1;
    flex: 1;
    min-height: 0;
    display: flex;
    padding: var(--space-2) var(--space-2) 0 0;
  }
  .content-surface {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--stroke-surface);
    border-bottom: none;
    border-radius: var(--radius-large) var(--radius-large) 0 0;
    background: var(--surface-content);
    box-shadow: var(--shadow-card);
    backdrop-filter: blur(18px) saturate(112%);
    -webkit-backdrop-filter: blur(18px) saturate(112%);
  }
  .details-region {
    position: relative;
    flex: none;
    min-width: 320px;
    max-width: 520px;
    min-height: 0;
    margin-left: var(--space-2);
    overflow: hidden;
    border: 1px solid var(--stroke-surface);
    border-bottom: none;
    border-radius: var(--radius-large) var(--radius-large) 0 0;
    background: var(--surface-overlay);
    box-shadow: var(--shadow-flyout);
    backdrop-filter: blur(24px) saturate(118%);
    -webkit-backdrop-filter: blur(24px) saturate(118%);
  }
  .resize-handle {
    position: absolute;
    z-index: 10;
    inset: 0 auto 0 -5px;
    width: 10px;
    border: 0;
    background: transparent;
    cursor: col-resize;
  }
  .resize-handle::after {
    content: "";
    position: absolute;
    left: 4px;
    top: 28px;
    bottom: 28px;
    width: 2px;
    border-radius: var(--radius-pill);
    background: transparent;
    transition: background var(--motion-fast) var(--motion-easing);
  }
  .resize-handle:hover::after,
  .resizing .resize-handle::after {
    background: var(--accent-default);
  }
  @media (max-width: 980px) {
    .details-region {
      position: absolute;
      z-index: 20;
      top: var(--space-2);
      right: var(--space-2);
      bottom: 0;
      width: min(430px, calc(100% - 74px)) !important;
      margin: 0;
    }
    .resize-handle { display: none; }
  }
  @media (max-width: 680px) {
    .body { padding-right: 0; }
    .content-surface { border-right: 0; border-radius: var(--radius-large) 0 0 0; }
    .details-region { right: 0; width: calc(100% - 54px) !important; border-radius: var(--radius-large) 0 0 0; }
  }
</style>
