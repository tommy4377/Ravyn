import type { BypassModifierKey, ExtensionSettings } from "./contracts";

export const DEFAULT_SETTINGS: ExtensionSettings = {
  // A download manager extension should work immediately after installation.
  // Users can still choose Rules only, Ask every time, or Disabled in Options.
  interceptionMode: "all-compatible",
  automaticInterception: true,
  // Mirrors IDM's Alt-to-bypass: hold the key while clicking a download link
  // to let Firefox handle that one download instead of Ravyn.
  bypassModifierKey: "alt",
  // Empty means "no restriction" — every GET download is a candidate, as
  // today. Users can narrow this to a specific extension list in Options.
  interceptExtensions: [],
  minInterceptSizeBytes: 0,
  mediaDetection: true,
  networkObservation: false,
  videoOverlays: true,
  imageOverlays: false,
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

export const SETTINGS_KEY = "ravyn.settings";

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
    bypassModifierKey: (
      ["alt", "shift", "ctrl", "none"] as BypassModifierKey[]
    ).includes(value.bypassModifierKey)
      ? value.bypassModifierKey
      : DEFAULT_SETTINGS.bypassModifierKey,
    interceptExtensions: normalizeExtensionList(value.interceptExtensions),
    minInterceptSizeBytes: clampInteger(
      value.minInterceptSizeBytes,
      0,
      1024 ** 3,
      DEFAULT_SETTINGS.minInterceptSizeBytes,
    ),
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

/**
 * Accepts free text like "example.com", "https://example.com/path" or
 * "*.example.com" and normalizes to a bare, lowercase host (or wildcard
 * host) — a pasted scheme/path previously matched nothing, silently.
 */
function normalizeDomainList(values: string[]): string[] {
  return [
    ...new Set(
      values
        .map((value) => stripToHost(value))
        .filter((value): value is string => !!value),
    ),
  ].slice(0, 500);
}

function stripToHost(value: string): string | null {
  const trimmed = value.trim().toLowerCase();
  if (!trimmed) return null;
  if (!/^[a-z][a-z0-9+.-]*:\/\//.test(trimmed)) {
    // No scheme: treat the leading segment before any path/query as the
    // host, preserving a leading "*." wildcard.
    const host = trimmed.split(/[/?#]/, 1)[0];
    return host || null;
  }
  try {
    return new URL(trimmed).hostname || null;
  } catch {
    return null;
  }
}

function normalizeExtensionList(values: string[]): string[] {
  return [
    ...new Set(
      values
        .map((value) => value.trim().toLowerCase().replace(/^\./, ""))
        .filter((value) => /^[a-z0-9]{1,16}$/.test(value)),
    ),
  ].slice(0, 200);
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
