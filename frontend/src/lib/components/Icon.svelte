<script lang="ts">
  /**
   * Ravyn's own minimal, consistent icon set (20x20, 1.6 stroke,
   * round caps, `currentColor`) — deliberately not Fluent System Icons,
   * Material, or emoji, so it themes correctly with the token system and
   * needs no bundled icon-font/SVG dependency. Extend the `paths` map as
   * new screens need new glyphs; keep the visual weight consistent.
   */
  export type IconName =
    | "add"
    | "search"
    | "close"
    | "filter"
    | "sort"
    | "more"
    | "chevron-down"
    | "chevron-up"
    | "chevron-right"
    | "chevron-left"
    | "play"
    | "pause"
    | "refresh"
    | "cancel"
    | "trash"
    | "folder-open"
    | "external-link"
    | "check-circle"
    | "alert-circle"
    | "warning"
    | "info"
    | "download"
    | "upload"
    | "clock"
    | "spinner"
    | "paste"
    | "grid"
    | "list";

  let { name, size = 16 }: { name: IconName; size?: number } = $props();
</script>

<svg
  class="icon"
  class:spin={name === "spinner"}
  width={size}
  height={size}
  viewBox="0 0 20 20"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === "add"}
    <path d="M10 4v12M4 10h12" />
  {:else if name === "search"}
    <circle cx="8.5" cy="8.5" r="5" />
    <path d="M16 16l-3.8-3.8" />
  {:else if name === "close"}
    <path d="M5 5l10 10M15 5L5 15" />
  {:else if name === "filter"}
    <path d="M3 5h14M6 10h8M8.5 15h3" />
  {:else if name === "sort"}
    <path d="M6 4v12M6 4L3 7M6 4l3 3M14 16V4M14 16l3-3M14 16l-3-3" />
  {:else if name === "more"}
    <circle cx="4.5" cy="10" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="10" cy="10" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="15.5" cy="10" r="1.1" fill="currentColor" stroke="none" />
  {:else if name === "chevron-down"}
    <path d="M5 7.5l5 5 5-5" />
  {:else if name === "chevron-up"}
    <path d="M5 12.5l5-5 5 5" />
  {:else if name === "chevron-right"}
    <path d="M7.5 5l5 5-5 5" />
  {:else if name === "chevron-left"}
    <path d="M12.5 5l-5 5 5 5" />
  {:else if name === "play"}
    <path d="M6 4.5l9 5.5-9 5.5z" fill="currentColor" stroke="none" />
  {:else if name === "pause"}
    <path d="M6.5 4.5v11M13.5 4.5v11" />
  {:else if name === "refresh"}
    <path d="M16 10a6 6 0 1 1-1.8-4.3" />
    <path d="M16 3.5V7h-3.5" />
  {:else if name === "cancel"}
    <circle cx="10" cy="10" r="6.5" />
    <path d="M7.5 7.5l5 5M12.5 7.5l-5 5" />
  {:else if name === "trash"}
    <path d="M4.5 6h11M8 6V4.5h4V6M6 6l.6 9.2a1 1 0 0 0 1 .8h4.8a1 1 0 0 0 1-.8L14 6" />
  {:else if name === "folder-open"}
    <path d="M3 6.5V15a1 1 0 0 0 1 1h11.5a1 1 0 0 0 1-.8l1-6.2H7l-1 2M3 6.5h4l1.2 1.7h6.3a1 1 0 0 1 1 1.1" />
  {:else if name === "external-link"}
    <path d="M8.5 5H5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-3.5" />
    <path d="M11.5 3.5H16.5V8.5" />
    <path d="M16 4l-7 7" />
  {:else if name === "check-circle"}
    <circle cx="10" cy="10" r="6.5" />
    <path d="M7 10.2l2.1 2.1L13.3 8" />
  {:else if name === "alert-circle"}
    <circle cx="10" cy="10" r="6.5" />
    <path d="M10 6.7v4M10 13.2h.01" />
  {:else if name === "warning"}
    <path d="M10 3.8l7.2 12.4a1 1 0 0 1-.87 1.5H3.67a1 1 0 0 1-.87-1.5L10 3.8z" />
    <path d="M10 8.5v3.2M10 14.3h.01" />
  {:else if name === "info"}
    <circle cx="10" cy="10" r="6.5" />
    <path d="M10 9v4.2M10 6.6h.01" />
  {:else if name === "download"}
    <path d="M10 3.5v9.2M6.3 9.2L10 12.9l3.7-3.7" />
    <path d="M4 15.5h12" />
  {:else if name === "upload"}
    <path d="M10 16.5V7.3M6.3 10.8L10 7.1l3.7 3.7" />
    <path d="M4 4.5h12" />
  {:else if name === "clock"}
    <circle cx="10" cy="10" r="6.5" />
    <path d="M10 6.3V10l3 1.8" />
  {:else if name === "spinner"}
    <path d="M10 3.5v3M10 13.5v3M3.5 10h3M13.5 10h3M5.6 5.6l2.1 2.1M12.3 12.3l2.1 2.1M5.6 14.4l2.1-2.1M12.3 7.7l2.1-2.1" opacity="0.9" />
  {:else if name === "paste"}
    <path d="M7.5 4.5h5v2h-5z" />
    <path d="M5.5 6.5h9a1 1 0 0 1 1 1V15a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1V7.5a1 1 0 0 1 1-1z" />
  {:else if name === "grid"}
    <rect x="3.5" y="3.5" width="5.5" height="5.5" rx="1" />
    <rect x="11" y="3.5" width="5.5" height="5.5" rx="1" />
    <rect x="3.5" y="11" width="5.5" height="5.5" rx="1" />
    <rect x="11" y="11" width="5.5" height="5.5" rx="1" />
  {:else if name === "list"}
    <path d="M6.5 5.5h10M6.5 10h10M6.5 14.5h10M3.5 5.5h.01M3.5 10h.01M3.5 14.5h.01" />
  {/if}
</svg>

<style>
  .icon {
    display: inline-block;
    flex: none;
    vertical-align: middle;
  }
  .icon.spin {
    animation: icon-spin 0.9s linear infinite;
  }
  @media (prefers-reduced-motion: reduce) {
    .icon.spin {
      animation-duration: 2.4s;
    }
  }
  @keyframes icon-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
