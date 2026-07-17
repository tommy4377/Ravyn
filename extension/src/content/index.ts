import type { BackgroundRequest, ExtensionSettings } from "../shared/contracts";
import { DEFAULT_SETTINGS, SETTINGS_KEY } from "../shared/settings";
import { normalizeUrl } from "../shared/urls";
import { scanDocument } from "./scanner/dom-scanner";
import { BoundedMutationScanner } from "./scanner/mutation-observer";
import { MediaOverlayController } from "./media/overlay";

let settings: ExtensionSettings = DEFAULT_SETTINGS;
let monitorEnabled = false;
let lastScanAt = 0;
let lastContextTarget: Element | null = null;
let bypassModifierHeld = false;
const overlay = new MediaOverlayController(settings);
const mutationScanner = new BoundedMutationScanner(() => {
  if (monitorEnabled) void publishResources();
});

document.addEventListener(
  "contextmenu",
  (event) => {
    lastContextTarget = event.target instanceof Element ? event.target : null;
  },
  true,
);

// IDM-style escape hatch: holding the configured modifier while clicking a
// download link tells the interceptor to leave that one download to
// Firefox. Tracked here (not in the background) because keyboard state
// isn't visible to the background page.
document.addEventListener("keydown", (event) => trackModifier(event), true);
document.addEventListener("keyup", (event) => trackModifier(event), true);
window.addEventListener("blur", () => (bypassModifierHeld = false));
document.addEventListener(
  "click",
  (event) => {
    if (!bypassModifierHeld || settings.bypassModifierKey === "none") return;
    const target = event.target instanceof Element ? event.target : null;
    const href = target?.closest("a")?.href;
    if (!href) return;
    const normalized = normalizeUrl(href);
    if (!normalized) return;
    void browser.runtime.sendMessage({
      type: "bypass-download",
      url: normalized,
    } satisfies BackgroundRequest);
  },
  true,
);

void initialize();

async function initialize(): Promise<void> {
  try {
    const response: unknown = await browser.runtime.sendMessage({
      type: "get-settings",
    } satisfies BackgroundRequest);
    if (response) settings = response as ExtensionSettings;
  } catch {
    settings = DEFAULT_SETTINGS;
  }
  overlay.updateSettings(settings);
  overlay.start();
}

// storage.onChanged (not a runtime.sendMessage push) is the only channel
// that reliably reaches every open tab's content script in Firefox —
// runtime.sendMessage only delivers to extension pages (popup/options).
browser.storage.onChanged.addListener((changes, area) => {
  if (area !== "local" || !changes[SETTINGS_KEY]) return;
  settings = changes[SETTINGS_KEY].newValue as ExtensionSettings;
  overlay.updateSettings(settings);
});

browser.runtime.onMessage.addListener((message: unknown) => {
  if (!message || typeof message !== "object") return undefined;
  const request = message as Record<string, unknown>;
  if (request.type === "scan-page") return publishResources(true);
  if (request.type === "monitor-page") {
    monitorEnabled = request.enabled === true;
    if (monitorEnabled) {
      mutationScanner.start();
      return publishResources(true);
    }
    mutationScanner.stop();
    return Promise.resolve([]);
  }
  if (request.type === "collect-context")
    return Promise.resolve(
      collectContext(request.context as string | undefined),
    );
  return undefined;
});

function trackModifier(event: KeyboardEvent): void {
  const key = settings.bypassModifierKey;
  if (key === "none") {
    bypassModifierHeld = false;
    return;
  }
  const pressed =
    (key === "alt" && event.altKey) ||
    (key === "shift" && event.shiftKey) ||
    (key === "ctrl" && event.ctrlKey);
  bypassModifierHeld = pressed;
}

async function publishResources(
  force = false,
): Promise<ReturnType<typeof scanDocument>> {
  if (!force && Date.now() - lastScanAt < 500) return [];
  lastScanAt = Date.now();
  const resources = scanDocument();
  await browser.runtime.sendMessage({
    type: "resources-detected",
    resources,
  } satisfies BackgroundRequest);
  return resources;
}

function resolveUrl(value: string): string | undefined {
  try {
    return new URL(value, document.baseURI).href;
  } catch {
    return undefined;
  }
}

function collectContext(context?: string): Record<string, unknown> {
  const active = lastContextTarget ?? document.activeElement;
  if (context === "selection")
    return { selectionText: window.getSelection()?.toString() ?? "" };
  if (active instanceof HTMLImageElement) {
    const pictureSources = [
      ...(active
        .closest("picture")
        ?.querySelectorAll<HTMLSourceElement>("source[srcset]") ?? []),
    ].flatMap((source) =>
      source.srcset
        .split(",")
        .map((entry) => entry.trim().split(/\s+/, 1)[0])
        .filter((value): value is string => !!value)
        .map(resolveUrl)
        .filter((value): value is string => !!value),
    );
    const srcsetSources = active.srcset
      .split(",")
      .map((entry) => entry.trim().split(/\s+/, 1)[0])
      .filter((value): value is string => !!value)
      .map(resolveUrl)
      .filter((value): value is string => !!value);
    return {
      currentSrc: active.currentSrc,
      src: active.src,
      sources: [
        ...new Set(
          [
            active.currentSrc,
            ...pictureSources,
            ...srcsetSources,
            active.src,
          ].filter(Boolean),
        ),
      ],
      parentLink: active.closest("a")?.href,
      naturalWidth: active.naturalWidth,
      naturalHeight: active.naturalHeight,
      alt: active.alt,
    };
  }
  return { pageUrl: location.href, pageTitle: document.title };
}
