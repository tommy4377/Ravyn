import { createAccentPalette } from "../appearance/colors";

/**
 * Main-window navigation and local appearance preferences.
 * Backend settings remain in /v1/settings; these values only control the shell.
 */

export type NavSection =
  | "downloads"
  | "library"
  | "media"
  | "torrents"
  | "basket"
  | "automation"
  | "settings";

export type DownloadsView = "all" | "active" | "queued" | "completed" | "failed";
export type Density = "comfortable" | "compact";
export type ThemePreference = "system" | "light" | "dark";
export type MaterialPreference = "synthetic" | "solid";

const DENSITY_KEY = "ravyn.density";
const THEME_KEY = "ravyn.theme";
const MATERIAL_KEY = "ravyn.material";
const MATERIAL_INTENSITY_KEY = "ravyn.materialIntensity";
const BACKDROP_IMAGE_KEY = "ravyn.backdropImage";
const NAV_COLLAPSED_KEY = "ravyn.navigationCollapsed";

function loadDensity(): Density {
  return localStorage.getItem(DENSITY_KEY) === "compact" ? "compact" : "comfortable";
}

function loadTheme(): ThemePreference {
  const stored = localStorage.getItem(THEME_KEY);
  return stored === "light" || stored === "dark" ? stored : "system";
}

function loadMaterial(): MaterialPreference {
  return localStorage.getItem(MATERIAL_KEY) === "solid" ? "solid" : "synthetic";
}

function clampIntensity(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value)));
}

class NavigationStore {
  section = $state<NavSection>("downloads");
  downloadsView = $state<DownloadsView>("all");
  selectedJobId = $state<string | null>(null);
  detailsPaneOpen = $state(true);
  density = $state<Density>("comfortable");
  theme = $state<ThemePreference>("system");
  resolvedTheme = $state<"light" | "dark">("light");
  material = $state<MaterialPreference>("synthetic");
  materialIntensity = $state(76);
  backdropImage = $state("");
  navigationCollapsed = $state(false);
  navigationOverlayOpen = $state(false);
  basketDrawerOpen = $state(false);
  notificationDrawerOpen = $state(false);
  systemAccent = $state<string | null>(null);
  pendingAddKind = $state<"http" | "media" | "torrent" | null>(null);
  settingsDirty = $state(false);
  pendingSection = $state<NavSection | null>(null);

  private initialized = false;
  private mediaQuery: MediaQueryList | null = null;
  private readonly onSystemThemeChange = (): void => this.applyAppearance();

  init(): void {
    if (this.initialized) return;
    this.initialized = true;
    this.density = loadDensity();
    this.theme = loadTheme();
    this.material = loadMaterial();
    this.materialIntensity = clampIntensity(Number(localStorage.getItem(MATERIAL_INTENSITY_KEY) ?? 76));
    this.backdropImage = localStorage.getItem(BACKDROP_IMAGE_KEY) ?? "";
    this.navigationCollapsed = localStorage.getItem(NAV_COLLAPSED_KEY) === "true";
    this.mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    this.mediaQuery.addEventListener("change", this.onSystemThemeChange);
    this.applyAppearance();
  }

  private applyAppearance(): void {
    const dark = this.theme === "dark" || (this.theme === "system" && !!this.mediaQuery?.matches);
    this.resolvedTheme = dark ? "dark" : "light";
    const root = document.documentElement;
    root.dataset.theme = this.resolvedTheme;
    root.dataset.themePreference = this.theme;
    root.dataset.density = this.density;
    root.dataset.material = this.material;
    root.style.setProperty("--material-intensity", `${this.materialIntensity / 100}`);
    const accentProperties = [
      "--accent-default",
      "--accent-hover",
      "--accent-pressed",
      "--accent-subtle",
      "--accent-border",
      "--accent-text",
      "--accent-on-color",
      "--text-on-accent",
    ];
    if (window.matchMedia("(forced-colors: active)").matches) {
      for (const property of accentProperties) root.style.removeProperty(property);
    } else {
      const accent = createAccentPalette(this.systemAccent, this.resolvedTheme);
      root.style.setProperty("--accent-default", accent.default);
      root.style.setProperty("--accent-hover", accent.hover);
      root.style.setProperty("--accent-pressed", accent.pressed);
      root.style.setProperty("--accent-subtle", accent.subtle);
      root.style.setProperty("--accent-border", accent.border);
      root.style.setProperty("--accent-text", accent.text);
      root.style.setProperty("--accent-on-color", accent.onColor);
      root.style.setProperty("--text-on-accent", accent.onColor);
    }
    if (this.backdropImage.trim()) {
      const escaped = this.backdropImage.replace(/["\\]/g, "\\$&");
      root.style.setProperty("--ravyn-backdrop-image", `url("${escaped}")`);
      root.dataset.hasBackdropImage = "true";
    } else {
      root.style.removeProperty("--ravyn-backdrop-image");
      delete root.dataset.hasBackdropImage;
    }
  }

  setDensity(density: Density): void {
    this.density = density;
    localStorage.setItem(DENSITY_KEY, density);
    this.applyAppearance();
  }

  setTheme(theme: ThemePreference): void {
    this.theme = theme;
    localStorage.setItem(THEME_KEY, theme);
    this.applyAppearance();
  }

  setMaterial(material: MaterialPreference): void {
    this.material = material;
    localStorage.setItem(MATERIAL_KEY, material);
    this.applyAppearance();
  }

  setMaterialIntensity(intensity: number): void {
    this.materialIntensity = clampIntensity(intensity);
    localStorage.setItem(MATERIAL_INTENSITY_KEY, String(this.materialIntensity));
    this.applyAppearance();
  }

  setBackdropImage(value: string): void {
    this.backdropImage = value.trim();
    localStorage.setItem(BACKDROP_IMAGE_KEY, this.backdropImage);
    this.applyAppearance();
  }

  setSystemAccent(accent: string | null): void {
    this.systemAccent = accent;
    this.applyAppearance();
  }


  navigate(section: NavSection): boolean {
    if (this.section === "settings" && this.settingsDirty && section !== "settings") {
      this.pendingSection = section;
      return false;
    }
    this.section = section;
    this.navigationOverlayOpen = false;
    return true;
  }

  confirmPendingNavigation(): void {
    const target = this.pendingSection;
    this.pendingSection = null;
    this.settingsDirty = false;
    if (target) this.navigate(target);
  }

  cancelPendingNavigation(): void {
    this.pendingSection = null;
  }

  openBasket(): void {
    this.notificationDrawerOpen = false;
    this.basketDrawerOpen = true;
  }

  openNotifications(): void {
    this.basketDrawerOpen = false;
    this.notificationDrawerOpen = true;
  }

  closeTransientLayers(): boolean {
    if (this.notificationDrawerOpen) {
      this.notificationDrawerOpen = false;
      return true;
    }
    if (this.basketDrawerOpen) {
      this.basketDrawerOpen = false;
      return true;
    }
    if (this.navigationOverlayOpen) {
      this.navigationOverlayOpen = false;
      return true;
    }
    return false;
  }

  setNavigationCollapsed(collapsed: boolean): void {
    this.navigationCollapsed = collapsed;
    localStorage.setItem(NAV_COLLAPSED_KEY, String(collapsed));
  }

  selectJob(id: string | null): void {
    this.selectedJobId = id;
    if (id) this.detailsPaneOpen = true;
  }

  requestAdd(kind: "http" | "media" | "torrent" = "http"): void {
    this.pendingAddKind = kind;
    this.navigate("downloads");
  }

  consumeAddRequest(): "http" | "media" | "torrent" | null {
    const kind = this.pendingAddKind;
    this.pendingAddKind = null;
    return kind;
  }
}

export const navigation = new NavigationStore();
