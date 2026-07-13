<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    message = "",
    retry,
    details = "",
    children,
  }: {
    title: string;
    message?: string;
    retry?: () => void;
    details?: string;
    children?: Snippet;
  } = $props();

  let showDetails = $state(false);
</script>

<div class="error" role="alert">
  <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
    <path
      fill="currentColor"
      d="M8 1a7 7 0 1 1 0 14A7 7 0 0 1 8 1Zm0 9.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5ZM8 4a.75.75 0 0 0-.75.75v4a.75.75 0 0 0 1.5 0v-4A.75.75 0 0 0 8 4Z"
    />
  </svg>
  <div class="body">
    <p class="title">{title}</p>
    {#if message}
      <p class="message">{message}</p>
    {/if}
    <div class="actions">
      {#if retry}
        <button class="action" onclick={retry}>Retry</button>
      {/if}
      {#if details}
        <button class="action" onclick={() => (showDetails = !showDetails)}>
          {showDetails ? "Hide details" : "Details"}
        </button>
      {/if}
    </div>
    {#if showDetails && details}
      <pre class="details">{details}</pre>
    {/if}
    {#if children}
      {@render children()}
    {/if}
  </div>
</div>

<style>
  .error {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--stroke-control);
    border-radius: var(--radius-control);
    background: var(--status-error-bg);
    color: var(--text-primary);
  }
  .icon {
    width: 16px;
    height: 16px;
    margin-top: 2px;
    color: var(--status-error);
    flex: none;
  }
  .body {
    min-width: 0;
  }
  .title {
    margin: 0;
    font-weight: 600;
  }
  .message {
    margin: var(--space-1) 0 0;
    color: var(--text-secondary);
  }
  .actions {
    display: flex;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .action {
    border: none;
    background: none;
    padding: 0;
    font-family: inherit;
    font-size: var(--text-body);
    color: var(--accent-text);
    text-decoration: underline;
  }
  .details {
    margin: var(--space-2) 0 0;
    padding: var(--space-2);
    font-size: var(--text-caption);
    background: var(--bg-subtle);
    border-radius: var(--radius-control);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
