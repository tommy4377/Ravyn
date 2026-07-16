import type { BackgroundRequest, ExtensionSettings } from "../shared/contracts";
import { DEFAULT_SETTINGS } from "../shared/settings";
import { scanDocument } from "./scanner/dom-scanner";
import { BoundedMutationScanner } from "./scanner/mutation-observer";
import { MediaOverlayController } from "./media/overlay";

let settings: ExtensionSettings = DEFAULT_SETTINGS;
let monitorEnabled = false;
let lastScanAt = 0;
let lastContextTarget: Element | null = null;
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
  if (request.type === "update-settings") {
    settings = {
      ...settings,
      ...(request.settings as Partial<ExtensionSettings>),
    };
    overlay.updateSettings(settings);
  }
  if (request.type === "collect-context")
    return Promise.resolve(
      collectContext(request.context as string | undefined),
    );
  return undefined;
});

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
        .filter((value): value is string => !!value),
    );
    const srcsetSources = active.srcset
      .split(",")
      .map((entry) => entry.trim().split(/\s+/, 1)[0])
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
