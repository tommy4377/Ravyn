/**
 * App-shell chrome state: which primary section/view is active, the
 * details-pane selection, and the persisted density/theme preferences.
 * Only sections with a real connected screen are ever set here — per the
 * design plan, unbuilt sections simply do not appear in navigation yet.
 */

export type NavSection = "downloads";

export type DownloadsView = "all" | "active" | "queued" | "completed" | "failed";

export type Density = "comfortable" | "compact";
export type ThemePreference = "system" | "light" | "dark";

const DENSITY_KEY = "ravyn.density";
const THEME_KEY = "ravyn.theme";

function loadDensity(): Density {
  const stored = localStorage.getItem(DENSITY_KEY);
  return stored === "compact" ? "compact" : "comfortable";
}

function loadTheme(): ThemePreference {
  const stored = localStorage.getItem(THEME_KEY);
  return stored === "light" || stored === "dark" ? stored : "system";
}

function applyTheme(theme: ThemePreference): void {
  const root = document.documentElement;
  if (theme === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", theme);
  }
}

function applyDensity(density: Density): void {
  document.documentElement.setAttribute("data-density", density);
}

class NavigationStore {
  section = $state<NavSection>("downloads");
  downloadsView = $state<DownloadsView>("all");
  selectedJobId = $state<string | null>(null);
  detailsPaneOpen = $state(true);
  density = $state<Density>("comfortable");
  theme = $state<ThemePreference>("system");

  /** Apply persisted preferences to the document; call once at startup. */
  init(): void {
    this.density = loadDensity();
    this.theme = loadTheme();
    applyDensity(this.density);
    applyTheme(this.theme);
  }

  setDensity(density: Density): void {
    this.density = density;
    localStorage.setItem(DENSITY_KEY, density);
    applyDensity(density);
  }

  setTheme(theme: ThemePreference): void {
    this.theme = theme;
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme);
  }

  selectJob(id: string | null): void {
    this.selectedJobId = id;
    if (id) this.detailsPaneOpen = true;
  }
}

export const navigation = new NavigationStore();
