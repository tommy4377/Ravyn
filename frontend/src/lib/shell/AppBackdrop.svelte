<script lang="ts">
  // The backdrop is entirely controlled by CSS custom properties on the root element.
</script>

<div class="backdrop" aria-hidden="true">
  <div class="wallpaper"></div>
  <div class="tint"></div>
  <div class="glow glow-a"></div>
  <div class="glow glow-b"></div>
  <div class="noise"></div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: -10;
    overflow: hidden;
    pointer-events: none;
    background: var(--backdrop-base);
  }
  .wallpaper {
    position: absolute;
    inset: -48px;
    background-image: var(--ravyn-backdrop-image, var(--system-backdrop-image, var(--backdrop-fallback)));
    background-position: center;
    background-size: cover;
    filter: blur(var(--backdrop-blur)) saturate(var(--backdrop-saturation)) brightness(var(--backdrop-brightness));
    transform: scale(1.08);
    opacity: calc(var(--material-intensity) * 0.88);
  }

  :global(:root[data-system-backdrop="true"]:not([data-has-backdrop-image])) .wallpaper {
    inset: auto;
    left: calc(var(--wallpaper-offset-x) * -1px);
    top: calc(var(--wallpaper-offset-y) * -1px);
    width: calc(var(--wallpaper-plane-width) * 1px);
    height: calc(var(--wallpaper-plane-height) * 1px);
    transform: none;
    background-repeat: no-repeat;
    background-position: center;
    background-size: cover;
  }
  :global(:root[data-system-backdrop="true"][data-wallpaper-position="fit"]:not([data-has-backdrop-image])) .wallpaper {
    background-size: contain;
  }
  :global(:root[data-system-backdrop="true"][data-wallpaper-position="stretch"]:not([data-has-backdrop-image])) .wallpaper {
    background-size: 100% 100%;
  }
  :global(:root[data-system-backdrop="true"][data-wallpaper-position="tile"]:not([data-has-backdrop-image])) .wallpaper {
    background-repeat: repeat;
    background-position: left top;
    background-size: auto;
  }
  :global(:root[data-system-backdrop="true"][data-wallpaper-position="center"]:not([data-has-backdrop-image])) .wallpaper {
    background-size: auto;
  }
  .tint {
    position: absolute;
    inset: 0;
    background: var(--backdrop-tint);
  }
  .glow {
    position: absolute;
    width: min(62vw, 760px);
    aspect-ratio: 1;
    border-radius: 50%;
    filter: blur(80px);
    opacity: calc(var(--material-intensity) * 0.08);
  }
  .glow-a { top: -38%; left: -12%; background: var(--backdrop-glow-a); }
  .glow-b { right: -22%; bottom: -48%; background: var(--backdrop-glow-b); }
  .noise {
    position: absolute;
    inset: 0;
    opacity: var(--noise-opacity);
    mix-blend-mode: soft-light;
    background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 180 180' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.82' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)' opacity='.72'/%3E%3C/svg%3E");
  }
  :global(:root[data-system-backdrop="true"]) .glow,
  :global(:root[data-has-backdrop-image]) .glow {
    display: none;
  }
  :global(:root[data-material="solid"]) .wallpaper,
  :global(:root[data-material="solid"]) .glow,
  :global(:root[data-material="solid"]) .noise { display: none; }
  @media (forced-colors: active) { .backdrop { background: Canvas; } .wallpaper, .glow, .noise, .tint { display: none; } }
</style>
