import type { ExtensionSettings } from "./contracts";

export const DEFAULT_SETTINGS: ExtensionSettings = {
  interceptionMode: "rules-only",
  automaticInterception: false,
  mediaDetection: true,
  networkObservation: false,
  videoOverlays: true,
  overlayMinimumWidth: 320,
  overlayMinimumHeight: 180,
  includePrivateWindows: false,
  eraseDelegatedBrowserEntries: false,
  notifications: true,
  sameDomainOnly: false,
  disabledDomains: [],
  alwaysInterceptDomains: [],
  allowCookiesByOrigin: [],
  maxResourcesPerTab: 2_000,
};

const SETTINGS_KEY = "ravyn.settings";

export async function loadSettings(): Promise<ExtensionSettings> {
  const result = await browser.storage.local.get(SETTINGS_KEY);
  const stored = result[SETTINGS_KEY] as Partial<ExtensionSettings> | undefined;
  return sanitizeSettings({ ...DEFAULT_SETTINGS, ...stored });
}

export async function saveSettings(
  patch: Partial<ExtensionSettings>,
): Promise<ExtensionSettings> {
  const current = await loadSettings();
  const next = sanitizeSettings({ ...current, ...patch });
  await browser.storage.local.set({ [SETTINGS_KEY]: next });
  return next;
}

export function sanitizeSettings(value: ExtensionSettings): ExtensionSettings {
  return {
    ...DEFAULT_SETTINGS,
    ...value,
    interceptionMode: [
      "disabled",
      "rules-only",
      "ask",
      "all-compatible",
    ].includes(value.interceptionMode)
      ? value.interceptionMode
      : DEFAULT_SETTINGS.interceptionMode,
    overlayMinimumWidth: clampInteger(
      value.overlayMinimumWidth,
      120,
      3840,
      DEFAULT_SETTINGS.overlayMinimumWidth,
    ),
    overlayMinimumHeight: clampInteger(
      value.overlayMinimumHeight,
      80,
      2160,
      DEFAULT_SETTINGS.overlayMinimumHeight,
    ),
    maxResourcesPerTab: clampInteger(
      value.maxResourcesPerTab,
      100,
      5_000,
      DEFAULT_SETTINGS.maxResourcesPerTab,
    ),
    disabledDomains: normalizeDomainList(value.disabledDomains),
    alwaysInterceptDomains: normalizeDomainList(value.alwaysInterceptDomains),
    allowCookiesByOrigin: [
      ...new Set(
        value.allowCookiesByOrigin.filter((item) => /^https?:\/\//i.test(item)),
      ),
    ].slice(0, 500),
  };
}

export async function clearExtensionData(): Promise<void> {
  await browser.storage.local.clear();
}

function normalizeDomainList(values: string[]): string[] {
  return [
    ...new Set(
      values.map((value) => value.trim().toLowerCase()).filter(Boolean),
    ),
  ].slice(0, 500);
}

function clampInteger(
  value: number,
  min: number,
  max: number,
  fallback: number,
): number {
  return Number.isInteger(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}
