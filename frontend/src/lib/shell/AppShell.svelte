<script lang="ts">
  import DownloadsView from "../downloads/DownloadsView.svelte";
  import JobDetailsPane from "../downloads/JobDetailsPane.svelte";
  import { JobsService } from "../services/jobs";
  import { connection } from "../stores/connection.svelte";
  import { jobsStore } from "../stores/jobs.svelte";
  import { navigation } from "../stores/navigation.svelte";
  import ConnectionBoot from "./ConnectionBoot.svelte";
  import NavigationView from "./NavigationView.svelte";
  import NotificationHost from "./NotificationHost.svelte";
  import StatusBar from "./StatusBar.svelte";

  navigation.init();

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
</script>

{#if connection.status !== "ready"}
  <ConnectionBoot />
{:else}
  <div class="shell">
    <div class="body">
      <NavigationView />
      <main class="content">
        {#if navigation.section === "downloads"}
          <DownloadsView />
        {/if}
      </main>
      {#if navigation.detailsPaneOpen && navigation.selectedJobId}
        <JobDetailsPane jobId={navigation.selectedJobId} onClose={() => navigation.selectJob(null)} />
      {/if}
    </div>
    <StatusBar />
  </div>
{/if}

<NotificationHost />

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
</style>
