<script lang="ts">
  let {
    width = "100%",
    height = "1em",
    radius = "var(--radius-control)",
  }: {
    width?: string;
    height?: string;
    radius?: string;
  } = $props();
</script>

<span
  class="skeleton"
  style="width:{width}; height:{height}; border-radius:{radius};"
  aria-hidden="true"
></span>

<style>
  .skeleton {
    display: inline-block;
    background: linear-gradient(
      90deg,
      var(--bg-subtle) 25%,
      var(--bg-subtle-hover) 37%,
      var(--bg-subtle) 63%
    );
    background-size: 400% 100%;
    animation: skeleton-shimmer 1.4s ease infinite;
  }
  @keyframes skeleton-shimmer {
    0% {
      background-position: 100% 50%;
    }
    100% {
      background-position: 0 50%;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .skeleton {
      animation: none;
      background: var(--bg-subtle);
    }
  }
</style>
