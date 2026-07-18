import { convertFileSrc, isTauri } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { desktopAppearance, type DesktopAppearance } from "../native/tauri";
import { navigation } from "../stores/navigation.svelte";

const MOVE_REFRESH_DELAY = 90;
const WALLPAPER_POLL_INTERVAL = 20_000;

class SystemAppearanceStore {
  supported = $state(false);
  wallpaperAvailable = $state(false);
  wallpaperPosition = $state<DesktopAppearance["wallpaper_position"]>("fill");
  accentColor = $state<string | null>(null);
  transparencyEnabled = $state(true);
  lastError = $state<string | null>(null);
  refreshing = $state(false);

  private initialized = false;
  private moveTimer: ReturnType<typeof setTimeout> | null = null;
  private pollTimer: ReturnType<typeof setInterval> | null = null;
  private unlisteners: UnlistenFn[] = [];
  private planeX = 0;
  private planeY = 0;
  private scaleFactor = 1;
  private frameOffsetX = 0;
  private frameOffsetY = 0;

  init(): () => void {
    if (this.initialized) return () => this.dispose();
    this.initialized = true;
    if (!isTauri()) return () => this.dispose();

    void this.refresh();
    const currentWindow = getCurrentWindow();
    void Promise.all([
      currentWindow.onMoved(({ payload }) => {
        this.applyMovedPosition(payload.x, payload.y);
        this.scheduleGeometryRefresh();
      }),
      currentWindow.onResized(() => this.scheduleGeometryRefresh()),
      currentWindow.onScaleChanged(() => this.scheduleGeometryRefresh()),
      currentWindow.onThemeChanged(() => {
        navigation.init();
        this.scheduleGeometryRefresh();
      }),
      currentWindow.onFocusChanged(({ payload: focused }) => {
        if (focused) void this.refresh();
      }),
    ]).then((unlisteners) => {
      this.unlisteners.push(...unlisteners);
    }).catch((cause) => {
      this.lastError = describeCause(cause);
    });
    this.pollTimer = setInterval(() => void this.refresh(), WALLPAPER_POLL_INTERVAL);
    return () => this.dispose();
  }

  async refresh(): Promise<void> {
    if (!isTauri() || this.refreshing) return;
    this.refreshing = true;
    try {
      const appearance = await desktopAppearance();
      this.apply(appearance);
      this.lastError = null;
    } catch (cause) {
      this.lastError = describeCause(cause);
    } finally {
      this.refreshing = false;
    }
  }

  private scheduleGeometryRefresh(): void {
    if (this.moveTimer) clearTimeout(this.moveTimer);
    this.moveTimer = setTimeout(() => {
      this.moveTimer = null;
      void this.refresh();
    }, MOVE_REFRESH_DELAY);
  }

  private apply(appearance: DesktopAppearance): void {
    this.supported = appearance.supported;
    this.wallpaperAvailable = Boolean(appearance.wallpaper_path);
    this.wallpaperPosition = appearance.wallpaper_position;
    this.accentColor = appearance.accent_color;
    this.transparencyEnabled = appearance.transparency_enabled;
    navigation.setSystemAccent(appearance.accent_color);

    this.planeX = appearance.plane_x;
    this.planeY = appearance.plane_y;
    this.scaleFactor = Math.max(0.5, appearance.scale_factor);
    this.frameOffsetX = appearance.frame_offset_x;
    this.frameOffsetY = appearance.frame_offset_y;

    const root = document.documentElement;
    const useNativeBackdrop = appearance.supported && appearance.transparency_enabled;
    root.dataset.nativeBackdrop = useNativeBackdrop ? "true" : "false";
    root.dataset.wallpaperPosition = appearance.wallpaper_position;
    root.dataset.systemBackdrop = appearance.wallpaper_path ? "true" : "false";
    root.dataset.systemTransparency = appearance.transparency_enabled ? "enabled" : "disabled";
    root.style.setProperty("--wallpaper-plane-width", String(Math.max(1, appearance.plane_width)));
    root.style.setProperty("--wallpaper-plane-height", String(Math.max(1, appearance.plane_height)));
    this.applyWindowPosition(appearance.window_x, appearance.window_y);

    if (appearance.wallpaper_path) {
      const base = convertFileSrc(appearance.wallpaper_path);
      const revision = appearance.wallpaper_revision
        ? `?v=${encodeURIComponent(appearance.wallpaper_revision)}`
        : "";
      root.style.setProperty("--system-backdrop-image", `url("${base}${revision}")`);
    } else {
      root.style.removeProperty("--system-backdrop-image");
    }
  }


  private applyMovedPosition(outerX: number, outerY: number): void {
    const logicalX = outerX / this.scaleFactor + this.frameOffsetX;
    const logicalY = outerY / this.scaleFactor + this.frameOffsetY;
    this.applyWindowPosition(logicalX, logicalY);
  }

  private applyWindowPosition(windowX: number, windowY: number): void {
    const root = document.documentElement;
    root.style.setProperty("--wallpaper-offset-x", String(windowX - this.planeX));
    root.style.setProperty("--wallpaper-offset-y", String(windowY - this.planeY));
  }

  private dispose(): void {
    if (this.moveTimer) clearTimeout(this.moveTimer);
    if (this.pollTimer) clearInterval(this.pollTimer);
    this.moveTimer = null;
    this.pollTimer = null;
    for (const unlisten of this.unlisteners.splice(0)) unlisten();
    this.initialized = false;
  }
}

function describeCause(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

export const systemAppearance = new SystemAppearanceStore();
