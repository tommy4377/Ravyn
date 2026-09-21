import type {
  CreateDownloadPayload,
  DetectedResource,
  SourceContext,
} from "../../shared/contracts";
import { normalizeUrl } from "../../shared/urls";
import type { NativeClient } from "../native/client";
import type { ResourceCache } from "../network/cache";
import { openResourcePopup } from "../popup";
import { MenuId } from "./register";

export function registerMenuHandlers(
  native: NativeClient,
  cache: ResourceCache,
): void {
  browser.menus.onClicked.addListener((info, tab) => {
    void handle(info, tab, native, cache);
  });
}

async function handle(
  info: browser.menus.OnClickData,
  tab: browser.tabs.Tab | undefined,
  native: NativeClient,
  cache: ResourceCache,
): Promise<void> {
  const tabId = tab?.id;
  const sourceContext = contextFor(tab, info.frameId);
  const directUrl = info.linkUrl ?? info.srcUrl;
  switch (info.menuItemId) {
    case MenuId.linkDownload:
    case MenuId.imageDownload:
    case MenuId.mediaDownload:
      if (directUrl)
        await create(native, {
          url: directUrl,
          referer: info.pageUrl,
          sourceContext,
        });
      break;
    case MenuId.imageOriginal: {
      const context =
        tabId === undefined ? null : await collectContext(tabId, "image");
      const original =
        stringArray(context?.sources)[0] ??
        stringValue(context?.currentSrc) ??
        directUrl;
      if (original)
        await create(native, {
          url: original,
          referer: info.pageUrl,
          sourceContext,
        });
      break;
    }
    case MenuId.imageChoose:
      if (tabId !== undefined) {
        await scanTab(tabId, cache);
        await openResourcePopup("image");
      }
      break;
    case MenuId.linkPaused:
      if (directUrl)
        await create(native, {
          url: directUrl,
          paused: true,
          referer: info.pageUrl,
          sourceContext,
        });
      break;
    case MenuId.linkAnalyze:
    case MenuId.mediaAnalyze:
      if (directUrl)
        await native.request("probe_media", { url: directUrl, sourceContext });
      break;
    case MenuId.imageConvert:
      if (directUrl)
        await create(native, {
          url: directUrl,
          referer: info.pageUrl,
          postProcessingPreset: "image-webp",
          sourceContext,
        });
      break;
    case MenuId.mediaAudio:
      if (directUrl)
        await create(native, {
          url: directUrl,
          kind: "media",
          referer: info.pageUrl,
          media: { audioOnly: true, audioFormat: "mp3" },
          sourceContext,
        });
      break;
    case MenuId.mediaSubtitles:
      await create(native, {
        url: info.pageUrl ?? directUrl ?? "",
        kind: "media",
        media: { writeSubtitles: true, subtitleLanguages: ["all"] },
        sourceContext,
      });
      break;
    case MenuId.linkSchedule:
      await native.request("open_ravyn", {
        section: "automation",
        sourceUrl: directUrl,
      });
      break;
    case MenuId.linkScanPage:
      // Opens the linked page in Ravyn's add flow, where the backend sniffer
      // classifies the page. Previously this duplicated "Schedule link".
      if (directUrl)
        await native.request("open_ravyn", {
          section: "downloads",
          sourceUrl: directUrl,
        });
      break;
    case MenuId.selectionUrls:
      await sendSelectionUrls(native, info.selectionText ?? "", sourceContext);
      break;
    case MenuId.selectionScan:
      if (tabId !== undefined) {
        const context = await collectContext(tabId, "selection");
        const selection =
          stringValue(context?.selectionText) ?? info.selectionText ?? "";
        const resources = urlsFromText(selection).map((url) => ({
          id: `selection:${url}`,
          url,
          normalizedUrl: url,
          pageUrl: info.pageUrl ?? tab?.url ?? url,
          type: "other" as const,
          source: "context-menu" as const,
          confidence: 0.8,
          discoveredAt: Date.now(),
        }));
        cache.merge(tabId, resources);
        await openResourcePopup();
      }
      break;
    case MenuId.pageScan:
    case MenuId.imageAll:
    case MenuId.pageImages:
    case MenuId.pageMedia:
    case MenuId.mediaPicker:
      if (tabId !== undefined) {
        await scanTab(tabId, cache);
        const type =
          info.menuItemId === MenuId.imageAll ||
          info.menuItemId === MenuId.pageImages
            ? "image"
            : info.menuItemId === MenuId.pageMedia ||
                info.menuItemId === MenuId.mediaPicker
              ? "video"
              : "all";
        await openResourcePopup(type);
      }
      break;
    case MenuId.pageYtdlp:
      if (info.pageUrl)
        await create(native, {
          url: info.pageUrl,
          kind: "media",
          sourceContext,
        });
      break;
    case MenuId.pageMonitor:
      if (tabId !== undefined) {
        cache.setMonitored(tabId, true);
        await browser.tabs
          .sendMessage(tabId, { type: "monitor-page", enabled: true })
          .catch(() => undefined);
        await openResourcePopup();
      }
      break;
    case MenuId.pagePopup:
      await openResourcePopup();
      break;
    default:
      break;
  }
}

async function create(
  native: NativeClient,
  payload: CreateDownloadPayload,
): Promise<void> {
  const normalized = normalizeUrl(payload.url);
  if (!normalized) return;
  await native.request("create_download", { ...payload, url: normalized });
}

async function sendSelectionUrls(
  native: NativeClient,
  selection: string,
  sourceContext: SourceContext,
): Promise<void> {
  const downloads = urlsFromText(selection).map((url) => ({
    url,
    sourceContext,
  }));
  if (downloads.length) await native.request("create_batch", { downloads });
}

async function scanTab(
  tabId: number,
  cache: ResourceCache,
): Promise<DetectedResource[]> {
  const resources = (await browser.tabs
    .sendMessage(tabId, { type: "scan-page" })
    .catch(() => [])) as DetectedResource[];
  return cache.merge(tabId, resources);
}

function contextFor(
  tab: browser.tabs.Tab | undefined,
  frameId?: number,
): SourceContext {
  return {
    browser: "firefox",
    containerId: tab?.cookieStoreId,
    incognito: tab?.incognito ?? false,
    pageUrl: tab?.url,
    pageTitle: tab?.title,
    tabId: tab?.id,
    frameId,
  };
}

async function collectContext(
  tabId: number,
  context: "image" | "selection",
): Promise<Record<string, unknown> | null> {
  const response: unknown = await browser.tabs
    .sendMessage(tabId, { type: "collect-context", context })
    .catch(() => null);
  return response && typeof response === "object"
    ? (response as Record<string, unknown>)
    : null;
}

function urlsFromText(value: string): string[] {
  const matches = value.match(/https?:\/\/[^\s<>"']+/gi) ?? [];
  const normalized = matches
    .map((url) => normalizeUrl(url))
    .filter((url): url is string => !!url);
  return [...new Set(normalized)].slice(0, 1_000);
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter(
        (item): item is string => typeof item === "string" && item.length > 0,
      )
    : [];
}
