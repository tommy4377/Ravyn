<script lang="ts">
  let {
    value = null,
    label,
  }: {
    /** 0-100, or null for indeterminate. */
    value?: number | null;
    label: string;
  } = $props();

  const clamped = $derived(
    value === null ? null : Math.max(0, Math.min(100, value)),
  );
</script>

<div
  class="track"
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={clamped ?? undefined}
>
  {#if clamped === null}
    <div class="fill indeterminate"></div>
  {:else}
    <div class="fill" style:width="{clamped}%"></div>
  {/if}
</div>

<style>
  .track {
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--bg-subtle-pressed);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent-default);
    transition: width var(--motion-fast) var(--motion-easing);
  }
  .indeterminate {
    width: 33%;
    animation: slide 1.4s var(--motion-easing) infinite;
  }
  @keyframes slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(320%);
    }
  }
</style>
