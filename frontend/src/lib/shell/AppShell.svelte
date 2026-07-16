<script lang="ts">
  import { onMount } from "svelte";
  import BasketView from "../basket/BasketView.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import DownloadsView from "../downloads/DownloadsView.svelte";
  import JobDetailsPane from "../downloads/JobDetailsPane.svelte";
  import { JobsService } from "../services/jobs";
  import { onTrayAction, takeBrowserAction, type BrowserAction, type TrayAction } from "../native/tauri";
  import { notifyDownloadEvent } from "./downloadNotifications";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import { notifications } from "../stores/notifications.svelte";
  import ConnectionBoot from "./ConnectionBoot.svelte";
  import NavigationView from "./NavigationView.svelte";
  import NotificationHistoryDrawer from "./NotificationHistoryDrawer.svelte";
  import NotificationHost from "./NotificationHost.svelte";
  import LazySectionState from "./LazySectionState.svelte";
  import StatusBar from "./StatusBar.svelte";

  navigation.init();
  notifications.init();

  function applyBrowserAction(action: BrowserAction | null): void {
    if (!action) return;
    if (action.source_url) {
      navigation.requestAdd(
        action.section === "media"
          ? "media"
          : action.section === "torrents"
            ? "torrent"
            : "http",
        action.source_url,
      );
      return;
    }
    const section = action.section;
    if (section === "library" || section === "media" || section === "torrents" || section === "automation" || section === "settings") {
      navigation.navigate(section);
    } else if (section === "components") {
      navigation.navigate("settings");
    } else {
      navigation.navigate("downloads");
    }
  }

  onMount(() => {
    const readBrowserAction = (): void => {
      void takeBrowserAction().then(applyBrowserAction).catch(() => undefined);
    };
    readBrowserAction();
    const browserActionTimer = window.setInterval(readBrowserAction, 750);
    const onKeydown = (event: KeyboardEvent): void => {
      const target = event.target as HTMLElement | null;
      const editing = target?.matches("input, textarea, select, [contenteditable='true']") ?? false;

      if (event.key === "Escape" && navigation.closeTransientLayers()) {
        event.preventDefault();
        return;
      }
      if (event.ctrlKey && event.key === ",") {
        event.preventDefault();
        navigation.navigate("settings");
        return;
      }
      if (event.ctrlKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        navigation.requestAdd();
        return;
      }
      if (event.ctrlKey && event.key.toLowerCase() === "f" && navigation.section === "downloads") {
        event.preventDefault();
        document.getElementById("downloads-search")?.focus();
        return;
      }
      if (!editing && event.ctrlKey && event.key.toLowerCase() === "v" && navigation.section === "downloads") {
        event.preventDefault();
        window.dispatchEvent(new CustomEvent("ravyn:paste-add"));
        return;
      }
      if (!editing && event.key === "F5") {
        event.preventDefault();
        if (navigation.section === "downloads") void jobsStore.refreshAll();
      }
    };

    window.addEventListener("keydown", onKeydown);
    return () => {
      window.removeEventListener("keydown", onKeydown);
      window.clearInterval(browserActionTimer);
    };
  });

  let detailsWidth = $state(loadDetailsWidth());
  let sectionLoadRevision = $state(0);
  let resizing = $state(false);

  $effect(() => {
    void connection.connect();
  });

  $effect(() => {
    if (connection.status !== "ready" || !connection.client || !connection.events) return;
    const service = new JobsService(connection.client);
    jobsStore.init(service);
    const unsubscribe = connection.events.subscribe((event) => {
      notifyDownloadEvent(event);
      jobsStore.applyEvent(event);
    });
    let unlistenTray: (() => void) | undefined;
    void onTrayAction((action: TrayAction) => {
      const ids = jobsStore.list
        .filter((job) =>
          action === "pause-all"
            ? job.status === "queued" || job.status === "downloading"
            : job.status === "paused",
        )
        .map((job) => job.id);
      if (ids.length > 0) {
        void service
          .bulkAction(action === "pause-all" ? "pause" : "resume", ids)
          .catch(() => undefined);
      }
    })
      .then((unlisten) => (unlistenTray = unlisten))
      .catch(() => undefined);
    return () => {
      unsubscribe();
      unlistenTray?.();
      jobsStore.dispose();
    };
  });

  function loadLazySection<T>(loader: () => Promise<T>, revision: number): Promise<T> {
    void revision;
    return loader();
  }

  function retryLazySection(): void {
    sectionLoadRevision += 1;
  }

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
    <div class="body">
      <NavigationView />
      <main class="content-surface" aria-live="polite">
        {#if navigation.section === "downloads"}
          <DownloadsView />
        {:else if navigation.section === "library"}
          {#await loadLazySection(() => import("../library/LibraryView.svelte"), sectionLoadRevision)}
            <LazySectionState />
          {:then { default: LibraryView }}
            <LibraryView />
          {:catch error}
            <LazySectionState {error} onRetry={retryLazySection} />
          {/await}
        {:else if navigation.section === "media"}
          {#await loadLazySection(() => import("../media/MediaView.svelte"), sectionLoadRevision)}
            <LazySectionState />
          {:then { default: MediaView }}
            <MediaView />
          {:catch error}
            <LazySectionState {error} onRetry={retryLazySection} />
          {/await}
        {:else if navigation.section === "torrents"}
          {#await loadLazySection(() => import("../torrents/TorrentsView.svelte"), sectionLoadRevision)}
            <LazySectionState />
          {:then { default: TorrentsView }}
            <TorrentsView />
          {:catch error}
            <LazySectionState {error} onRetry={retryLazySection} />
          {/await}
        {:else if navigation.section === "automation"}
          {#await loadLazySection(() => import("../automation/AutomationView.svelte"), sectionLoadRevision)}
            <LazySectionState />
          {:then { default: AutomationView }}
            <AutomationView />
          {:catch error}
            <LazySectionState {error} onRetry={retryLazySection} />
          {/await}
        {:else if navigation.section === "settings"}
          {#await loadLazySection(() => import("../settings/SettingsView.svelte"), sectionLoadRevision)}
            <LazySectionState />
          {:then { default: SettingsView }}
            <SettingsView />
          {:catch error}
            <LazySectionState {error} onRetry={retryLazySection} />
          {/await}
        {:else}
          <DownloadsView />
        {/if}
      </main>

      {#if navigation.section === "downloads" && navigation.detailsPaneOpen && navigation.selectedJobId}
        <aside class="details-region" style:width={`${detailsWidth}px`} aria-label="Download details">
          <button type="button" class="resize-handle" aria-label="Resize details pane" onpointerdown={beginResize}></button>
          <JobDetailsPane jobId={navigation.selectedJobId} onClose={() => navigation.selectJob(null)} />
        </aside>
      {/if}

      {#if navigation.basketDrawerOpen}
        <button class="drawer-scrim" type="button" aria-label="Close batch queue" onclick={() => (navigation.basketDrawerOpen = false)}></button>
        <aside class="side-drawer" aria-label="Batch queue">
          <header class="drawer-header">
            <div>
              <h2>Batch queue</h2>
              <p>Review and start grouped downloads.</p>
            </div>
            <button type="button" class="close-button" aria-label="Close batch queue" onclick={() => (navigation.basketDrawerOpen = false)}>
              <Icon name="close" size={17} />
            </button>
          </header>
          <div class="drawer-content"><BasketView embedded /></div>
        </aside>
      {:else if navigation.notificationDrawerOpen}
        <button class="drawer-scrim" type="button" aria-label="Close notifications" onclick={() => (navigation.notificationDrawerOpen = false)}></button>
        <aside class="side-drawer" aria-label="Notification history">
          <NotificationHistoryDrawer onClose={() => (navigation.notificationDrawerOpen = false)} />
        </aside>
      {/if}
    </div>
    <StatusBar />
  </div>
{/if}

<ConfirmDialog
  open={!!navigation.pendingSection}
  title="Discard unsaved settings?"
  message="Leaving Settings now will discard backend changes that have not been saved. Appearance preferences are already stored."
  confirmLabel="Discard and leave"
  destructive
  onConfirm={() => navigation.confirmPendingNavigation()}
  onClose={() => navigation.cancelPendingNavigation()}
/>

<NotificationHost />

<style>
  .shell { position: relative; isolation: isolate; height: 100%; display: flex; flex-direction: column; overflow: hidden; background: transparent; }
  .shell.resizing, .shell.resizing * { cursor: col-resize !important; user-select: none !important; }
  .body { position: relative; z-index: 1; flex: 1; min-height: 0; display: flex; }
  .content-surface { position: relative; flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--surface-content); }
  .details-region { position: relative; flex: none; min-width: 320px; max-width: 520px; min-height: 0; overflow: hidden; border-left: 1px solid var(--stroke-divider); background: var(--surface-overlay); backdrop-filter: blur(26px) saturate(118%); -webkit-backdrop-filter: blur(26px) saturate(118%); }
  .resize-handle { position: absolute; z-index: 10; inset: 0 auto 0 -5px; width: 10px; border: 0; background: transparent; cursor: col-resize; }
  .resize-handle::after { content: ""; position: absolute; left: 4px; top: 28px; bottom: 28px; width: 2px; border-radius: var(--radius-pill); background: transparent; transition: background var(--motion-fast) var(--motion-easing); }
  .resize-handle:hover::after, .resizing .resize-handle::after { background: var(--accent-default); }
  .drawer-scrim { position: absolute; z-index: 29; inset: 0; border: 0; background: rgba(0, 0, 0, .32); }
  .side-drawer { position: absolute; z-index: 30; top: 0; right: 0; bottom: 0; display: flex; flex-direction: column; width: min(480px, calc(100% - 56px)); border-left: 1px solid var(--stroke-surface); background: var(--surface-overlay); box-shadow: var(--shadow-flyout); backdrop-filter: blur(28px) saturate(118%); -webkit-backdrop-filter: blur(28px) saturate(118%); animation: drawer-in var(--motion-normal) var(--motion-easing); }
  .drawer-header { min-height: 72px; display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) var(--space-4); border-bottom: 1px solid var(--stroke-divider); }
  .drawer-header h2 { margin: 0; font-size: var(--text-subtitle); font-weight: 620; }
  .drawer-header p { margin: 2px 0 0; color: var(--text-tertiary); font-size: var(--text-caption); }
  .close-button { display: grid; place-items: center; width: 32px; height: 32px; border: 0; border-radius: var(--radius-control); background: transparent; cursor: default; }
  .close-button:hover { background: var(--bg-subtle-hover); }
  .drawer-content { flex: 1; min-height: 0; overflow: hidden; }
  .drawer-content :global(.page-header) { display: none; }
  @keyframes drawer-in { from { transform: translateX(28px); opacity: 0; } to { transform: translateX(0); opacity: 1; } }
  @media (max-width: 980px) {
    .details-region { position: absolute; z-index: 20; top: 0; right: 0; bottom: 0; width: min(430px, calc(100% - 56px)) !important; box-shadow: var(--shadow-flyout); }
    .resize-handle { display: none; }
  }
  @media (max-width: 680px) {
    .details-region, .side-drawer { width: 100% !important; }
  }
</style>
