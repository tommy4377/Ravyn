<script lang="ts">
  /** Lightweight Fluent-style line icons using a shared 20 px grid. */
  export type IconName =
    | "add" | "search" | "close" | "filter" | "sort" | "more"
    | "chevron-down" | "chevron-up" | "chevron-right" | "chevron-left"
    | "play" | "pause" | "refresh" | "cancel" | "trash" | "folder-open"
    | "external-link" | "check-circle" | "alert-circle" | "warning" | "info"
    | "download" | "upload" | "clock" | "spinner" | "paste" | "grid" | "list"
    | "library" | "basket" | "automation" | "components" | "settings" | "diagnostics"
    | "torrent" | "file" | "video" | "music" | "document" | "image" | "archive"
    | "database" | "shield" | "wrench" | "sun" | "moon" | "palette" | "compact"
    | "menu" | "speed" | "peer" | "tag" | "calendar" | "rule" | "cloud" | "copy"
    | "hard-drive" | "restore" | "verify" | "eye" | "edit" | "save" | "bolt" | "bell";

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
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#if name === "add"}<path d="M10 4v12M4 10h12" />
  {:else if name === "search"}<circle cx="8.5" cy="8.5" r="5" /><path d="M16 16l-3.8-3.8" />
  {:else if name === "close"}<path d="M5 5l10 10M15 5L5 15" />
  {:else if name === "filter"}<path d="M3.5 5h13l-5 5.4v4.4l-3 1.7v-6.1z" />
  {:else if name === "sort"}<path d="M6 4v12M6 4L3.5 6.5M6 4l2.5 2.5M14 16V4M14 16l2.5-2.5M14 16l-2.5-2.5" />
  {:else if name === "more"}<circle cx="4.5" cy="10" r="1" fill="currentColor" stroke="none" /><circle cx="10" cy="10" r="1" fill="currentColor" stroke="none" /><circle cx="15.5" cy="10" r="1" fill="currentColor" stroke="none" />
  {:else if name === "chevron-down"}<path d="M5 7.5l5 5 5-5" />
  {:else if name === "chevron-up"}<path d="M5 12.5l5-5 5 5" />
  {:else if name === "chevron-right"}<path d="M7.5 5l5 5-5 5" />
  {:else if name === "chevron-left"}<path d="M12.5 5l-5 5 5 5" />
  {:else if name === "play"}<path d="M6 4.5l9 5.5-9 5.5z" fill="currentColor" stroke="none" />
  {:else if name === "pause"}<path d="M6.5 4.5v11M13.5 4.5v11" />
  {:else if name === "refresh"}<path d="M16 10a6 6 0 1 1-1.8-4.3" /><path d="M16 3.5V7h-3.5" />
  {:else if name === "cancel"}<circle cx="10" cy="10" r="6.5" /><path d="M7.5 7.5l5 5M12.5 7.5l-5 5" />
  {:else if name === "trash"}<path d="M4.5 6h11M8 6V4.5h4V6M6 6l.6 9.2a1 1 0 0 0 1 .8h4.8a1 1 0 0 0 1-.8L14 6" />
  {:else if name === "folder-open"}<path d="M2.8 6.7v8.1a1.2 1.2 0 0 0 1.2 1.2h11.2a1.2 1.2 0 0 0 1.2-1l1-6H7.2L5.7 11M2.8 6.7h4l1.3 1.8h6.5" />
  {:else if name === "external-link"}<path d="M8.5 5H5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-3.5" /><path d="M11.5 3.5h5v5M16 4l-7 7" />
  {:else if name === "check-circle"}<circle cx="10" cy="10" r="6.5" /><path d="M7 10.2l2.1 2.1L13.3 8" />
  {:else if name === "alert-circle"}<circle cx="10" cy="10" r="6.5" /><path d="M10 6.7v4M10 13.2h.01" />
  {:else if name === "warning"}<path d="M10 3.8l7.2 12.4a1 1 0 0 1-.87 1.5H3.67a1 1 0 0 1-.87-1.5L10 3.8z" /><path d="M10 8.5v3.2M10 14.3h.01" />
  {:else if name === "info"}<circle cx="10" cy="10" r="6.5" /><path d="M10 9v4.2M10 6.6h.01" />
  {:else if name === "download"}<path d="M10 3.5v9.2M6.3 9.2L10 12.9l3.7-3.7M4 15.5h12" />
  {:else if name === "upload"}<path d="M10 16.5V7.3M6.3 10.8L10 7.1l3.7 3.7M4 4.5h12" />
  {:else if name === "clock"}<circle cx="10" cy="10" r="6.5" /><path d="M10 6.3V10l3 1.8" />
  {:else if name === "spinner"}<path d="M10 3.5v3M10 13.5v3M3.5 10h3M13.5 10h3M5.6 5.6l2.1 2.1M12.3 12.3l2.1 2.1M5.6 14.4l2.1-2.1M12.3 7.7l2.1-2.1" opacity=".9" />
  {:else if name === "paste"}<path d="M7.5 4.5h5v2h-5zM5.5 6.5h9a1 1 0 0 1 1 1V15a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1V7.5a1 1 0 0 1 1-1z" />
  {:else if name === "copy"}<rect x="6.5" y="6.5" width="9.5" height="9.5" rx="1.5" /><path d="M4.5 13.5H4a1.5 1.5 0 0 1-1.5-1.5V4A1.5 1.5 0 0 1 4 2.5h8A1.5 1.5 0 0 1 13.5 4v.5" />
  {:else if name === "grid"}<rect x="3.5" y="3.5" width="5.5" height="5.5" rx="1" /><rect x="11" y="3.5" width="5.5" height="5.5" rx="1" /><rect x="3.5" y="11" width="5.5" height="5.5" rx="1" /><rect x="11" y="11" width="5.5" height="5.5" rx="1" />
  {:else if name === "list"}<path d="M6.5 5.5h10M6.5 10h10M6.5 14.5h10M3.5 5.5h.01M3.5 10h.01M3.5 14.5h.01" />
  {:else if name === "library"}<path d="M3 4.5h4v11H3zM8.2 4.5h3.5v11H8.2zM13.2 5l3.1-.8 2.4 10.6-3.1.8z" />
  {:else if name === "basket"}<path d="M3.5 7h13l-1.2 8.2a1 1 0 0 1-1 .8H5.7a1 1 0 0 1-1-.8zM7 7l3-3 3 3M7.5 10v3M12.5 10v3" />
  {:else if name === "automation"}<path d="M4 6.5h7.5M8.5 3.5l3 3-3 3M16 13.5H8.5M11.5 10.5l-3 3 3 3" />
  {:else if name === "components"}<path d="M3.5 3.5h5.5V9H3.5zM11 3.5h5.5V9H11zM3.5 11h5.5v5.5H3.5zM11 11h5.5v5.5H11z" />
  {:else if name === "settings"}<circle cx="10" cy="10" r="2.2" /><path d="M10 3.2v1.5M10 15.3v1.5M3.2 10h1.5M15.3 10h1.5M5.2 5.2l1.1 1.1M13.7 13.7l1.1 1.1M14.8 5.2l-1.1 1.1M6.3 13.7l-1.1 1.1" />
  {:else if name === "diagnostics"}<path d="M3.5 14.5h2.3l1.5-6 2.4 8 2.1-11 1.6 6h3.1" />
  {:else if name === "torrent"}<path d="M5 5.2a7 7 0 0 1 9.8 9.8M5 9a3 3 0 0 1 6 0v2a2 2 0 1 0 4 0V9M5 12.8a7 7 0 0 0 9.8 2.2" />
  {:else if name === "file"}<path d="M5 2.8h6l4 4V17H5zM11 2.8v4h4" />
  {:else if name === "video"}<rect x="3" y="4" width="14" height="12" rx="2" /><path d="M8 7.5l5 2.5-5 2.5z" fill="currentColor" stroke="none" />
  {:else if name === "music"}<path d="M8 14.5V5l7-1.5V13M8 7l7-1.5M5.5 16.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM12.5 15a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z" />
  {:else if name === "document"}<path d="M5 2.8h6l4 4V17H5zM11 2.8v4h4M7.5 10h5M7.5 13h5" />
  {:else if name === "image"}<rect x="3" y="3.5" width="14" height="13" rx="2" /><circle cx="7" cy="7.5" r="1.2" /><path d="M4.5 14l3.5-3.5 2.3 2.3 2.1-2.1 3.1 3.3" />
  {:else if name === "archive"}<path d="M4 4h12v12H4zM4 7h12M8 4v3M12 4v3M8 10h4v3H8z" />
  {:else if name === "database"}<ellipse cx="10" cy="5" rx="6" ry="2.5" /><path d="M4 5v5c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5V5M4 10v5c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5v-5" />
  {:else if name === "shield"}<path d="M10 2.8l6 2.3v4.5c0 3.6-2.3 6.2-6 7.6-3.7-1.4-6-4-6-7.6V5.1zM7.3 10l1.8 1.8 3.7-4" />
  {:else if name === "wrench"}<path d="M12.5 4.2a4 4 0 0 0-4.8 5.2L3.5 13.6a1.7 1.7 0 0 0 2.4 2.4l4.2-4.2a4 4 0 0 0 5.2-4.8l-2.4 2.4-2.2-.6-.6-2.2z" />
  {:else if name === "sun"}<circle cx="10" cy="10" r="3" /><path d="M10 2.5v2M10 15.5v2M2.5 10h2M15.5 10h2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4" />
  {:else if name === "moon"}<path d="M15.8 12.7A6.5 6.5 0 0 1 7.3 4.2 6.5 6.5 0 1 0 15.8 12.7z" />
  {:else if name === "palette"}<path d="M10 3a7 7 0 1 0 0 14h1.1a1.5 1.5 0 0 0 0-3H10a1.5 1.5 0 0 1 0-3h2.5A4.5 4.5 0 0 0 17 6.5C17 4.6 13.9 3 10 3z" /><circle cx="6.5" cy="7" r=".7" fill="currentColor" /><circle cx="9.5" cy="5.5" r=".7" fill="currentColor" /><circle cx="13" cy="6.5" r=".7" fill="currentColor" />
  {:else if name === "compact"}<path d="M4 5h12M4 10h12M4 15h12" />
  {:else if name === "menu"}<path d="M3.5 5.5h13M3.5 10h13M3.5 14.5h13" />
  {:else if name === "speed"}<path d="M4 14a6.5 6.5 0 1 1 12 0M10 11l3.5-3.5M6 14h8" />
  {:else if name === "peer"}<circle cx="7" cy="7" r="2.2" /><circle cx="14" cy="8" r="1.8" /><path d="M3.5 15c.4-2.6 2-4 3.5-4s3.1 1.4 3.5 4M11.5 15c.2-1.9 1.3-3 2.5-3 1.1 0 2.2 1.1 2.5 3" />
  {:else if name === "tag"}<path d="M3.5 4h6.3l6.7 6.7-5.8 5.8L4 9.8z" /><circle cx="7" cy="7" r="1" />
  {:else if name === "calendar"}<rect x="3" y="4.5" width="14" height="12" rx="2" /><path d="M6.5 2.8v3.4M13.5 2.8v3.4M3 8h14" />
  {:else if name === "rule"}<path d="M4 5h12M4 10h8M4 15h10M14 8l2 2-2 2" />
  {:else if name === "cloud"}<path d="M6 15.5h8a3.5 3.5 0 0 0 .5-7 5 5 0 0 0-9.7 1.2A3 3 0 0 0 6 15.5z" />
  {:else if name === "hard-drive"}<rect x="3" y="4" width="14" height="12" rx="2" /><path d="M3 11h14M12.5 13.5h.01M15 13.5h.01" />
  {:else if name === "restore"}<path d="M5 7H2.8V4.8M3 6.8A7 7 0 1 1 4.7 15M10 6v4l3 2" />
  {:else if name === "verify"}<path d="M10 2.8l6 2.3v4.5c0 3.6-2.3 6.2-6 7.6-3.7-1.4-6-4-6-7.6V5.1zM7.3 10l1.8 1.8 3.7-4" />
  {:else if name === "eye"}<path d="M2.5 10s2.7-5 7.5-5 7.5 5 7.5 5-2.7 5-7.5 5-7.5-5-7.5-5z" /><circle cx="10" cy="10" r="2" />
  {:else if name === "edit"}<path d="M4 14.5V17h2.5L15 8.5 11.5 5zM13 3.5l3.5 3.5" />
  {:else if name === "save"}<path d="M4 3.5h10l2 2V16H4zM7 3.5V8h6V3.5M7 16v-5h6v5" />
  {:else if name === "bolt"}<path d="M11.5 2.8L5 11h4l-.5 6.2L15 9h-4z" />
  {:else if name === "bell"}<path d="M5 8.5a5 5 0 0 1 10 0v3.2l1.5 2.3h-13L5 11.7zM8 16h4" />
  {/if}
</svg>

<style>
  .icon { display: inline-block; flex: none; vertical-align: middle; }
  .icon.spin { animation: icon-spin .9s linear infinite; }
  @media (prefers-reduced-motion: reduce) { .icon.spin { animation-duration: 2.4s; } }
  @keyframes icon-spin { to { transform: rotate(360deg); } }
</style>
