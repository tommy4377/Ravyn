import type {
  BackgroundRequest,
  CookieValue,
  CreateBatchPayload,
  CreateDownloadPayload,
  DetectedResource,
  DownloadPreset,
  DownloadSummary,
  SourceContext,
} from "../shared/contracts";
import { toExtensionError } from "../shared/errors";
import {
  clearExtensionData,
  loadSettings,
  saveSettings,
} from "../shared/settings";
import { originPattern } from "../shared/urls";
import {
  validateBatchPayload,
  validateDownloadPayload,
} from "../shared/validation";
import { BypassRegistry } from "./downloads/bypass";
import {
  clearTrackedDownloads,
  downloadLabel,
  handleCompletionEvent,
  reconcileCompletions,
  trackBatchResult,
  trackDownload,
} from "./downloads/completion";
import { DelegationRegistry } from "./downloads/delegation";
import { DownloadInterceptor } from "./downloads/interceptor";
import { registerMenuHandlers } from "./menus/handlers";
import { registerMenus } from "./menus/register";
import { NativeClient } from "./native/client";
import { ResourceCache } from "./network/cache";
import { NetworkObserver } from "./network/observer";
import { openResourcePopup } from "./popup";
import { RuleCache } from "./rules/cache";

const native = new NativeClient();
const resources = new ResourceCache();
const rules = new RuleCache(native);
const network = new NetworkObserver(resources);
const bypass = new BypassRegistry();
const interceptor = new DownloadInterceptor(
  native,
  rules,
  new DelegationRegistry(),
  bypass,
);

void initialize();

async function initialize(): Promise<void> {
  const settings = await loadSettings();
  await registerMenus();
  registerMenuHandlers(native, resources);
  interceptor.register();
  await network.synchronize(settings);
  await browser.action
    .setBadgeBackgroundColor({ color: "#0f6cbd" })
    .catch(() => undefined);
  native.subscribeEvents((event) => {
    // The native host now proxies the backend's real SSE stream (rather
    // than the old request-refresh stub), so this fires the moment a rule
    // actually changes instead of waiting out the 10-minute cache TTL.
    if (event.event === "rule_changed") rules.invalidate();
    void handleCompletionEvent(event);
    void broadcast({ type: "ravyn-native-event", event });
  });
  // The event stream has no replay: completions that happened while the
  // port was down (event-page suspension, backend restart) are only
  // recoverable by re-checking the tracked jobs once the backend is back.
  let backendWasConnected = false;
  native.subscribeStatus((status) => {
    if (status.backendConnected && !backendWasConnected) {
      void reconcileCompletions(native);
    }
    backendWasConnected = status.backendConnected;
  });
  void native.connect().catch(() => undefined);
  browser.tabs.onRemoved.addListener((tabId) => resources.clear(tabId));
  browser.tabs.onUpdated.addListener((tabId, changeInfo) => {
    // A top-level navigation invalidates the previous page's detected media
    // — clear the count so the badge doesn't show stale results.
    if (changeInfo.url) {
      resources.clear(tabId);
      void updateBadge(tabId);
    }
  });
  browser.commands.onCommand.addListener((command) => {
    if (command === "open-popup") void openResourcePopup();
    if (command === "download-current-page") void downloadCurrentPage();
  });
}

browser.runtime.onInstalled.addListener((details) => {
  if (details.reason === "install") void browser.runtime.openOptionsPage();
});

browser.runtime.onMessage.addListener((message: unknown, sender) => {
  if (!message || typeof message !== "object") return undefined;
  return handleMessage(message as BackgroundRequest, sender).catch((error) => {
    const extensionError = toExtensionError(error);
    return { error: extensionError.toNativeError() };
  });
});

async function handleMessage(
  request: BackgroundRequest,
  sender: browser.runtime.MessageSender,
): Promise<unknown> {
  switch (request.type) {
    case "connection-status":
      return native.refreshStatus();
    case "download-url": {
      const payload = await enrichDownload(
        validateDownloadPayload(request.payload),
        sender.tab,
      );
      const job = await native.request<{ id?: string }>(
        "create_download",
        payload,
      );
      void trackDownload(job?.id, downloadLabel(payload));
      return job;
    }
    case "download-batch": {
      const batch = validateBatchPayload(request.payload);
      const downloads = await Promise.all(
        batch.downloads.map((download) => enrichDownload(download, sender.tab)),
      );
      const result = await native.request("create_batch", {
        downloads,
      } satisfies CreateBatchPayload);
      trackBatchResult(result, downloads);
      return result;
    }
    case "probe-media":
      return native.request(
        "probe_media",
        await enrichProbe(request.url, request.sourceContext, sender.tab),
      );
    case "download-media-element":
      return downloadMediaElement(
        request.resources,
        request.pageUrl,
        request.sourceContext,
        sender.tab,
      );
    case "scan-tab": {
      const tabId = request.tabId ?? sender.tab?.id ?? (await activeTab())?.id;
      if (tabId === undefined) return [];
      const detected = (await browser.tabs
        .sendMessage(tabId, { type: "scan-page" })
        .catch(() => [])) as DetectedResource[];
      const merged = resources.merge(
        tabId,
        detected,
        (await loadSettings()).maxResourcesPerTab,
      );
      void updateBadge(tabId);
      return merged;
    }
    case "get-tab-resources": {
      const tabId = request.tabId ?? sender.tab?.id ?? (await activeTab())?.id;
      return tabId === undefined ? [] : resources.list(tabId);
    }
    case "get-stream-hint": {
      const tabId = request.tabId ?? sender.tab?.id ?? (await activeTab())?.id;
      return tabId !== undefined && resources.hasStreamHint(tabId);
    }
    case "get-presets":
      return native.request<DownloadPreset[]>("list_presets");
    case "resources-detected": {
      const tabId = request.tabId ?? sender.tab?.id;
      if (tabId === undefined) return [];
      const merged = resources.merge(
        tabId,
        request.resources,
        (await loadSettings()).maxResourcesPerTab,
      );
      void updateBadge(tabId);
      await broadcast({ type: "ravyn-resources-updated", tabId });
      return merged;
    }
    case "open-ravyn":
      return native.request("open_ravyn", { section: request.section });
    case "get-summary":
      return native.request<DownloadSummary>("get_download_summary");
    case "pause-all":
      return native.request("pause_all");
    case "resume-all":
      return native.request("resume_all");
    case "get-settings":
      return loadSettings();
    case "save-settings": {
      // No push broadcast needed: browser.storage.local.set fires
      // storage.onChanged for every context (content scripts included),
      // which is what content/index.ts listens on directly.
      const next = await saveSettings(request.settings);
      await network.synchronize(next);
      return next;
    }
    case "request-site-permissions":
      return requestSitePermissions(
        request.url,
        request.cookies,
        request.network,
      );
    case "clear-extension-data":
      await clearExtensionData();
      await clearTrackedDownloads();
      resources.clearAll();
      await removeOptionalPermissions();
      return loadSettings();
    case "confirmation-result":
      interceptor.resolveConfirmation(request.requestId, request.accepted);
      return null;
    case "bypass-download":
      await bypass.arm(request.url);
      return null;
    case "monitor-tab":
      resources.setMonitored(request.tabId, request.enabled);
      await browser.tabs
        .sendMessage(request.tabId, {
          type: "monitor-page",
          enabled: request.enabled,
        })
        .catch(() => undefined);
      return resources.isMonitored(request.tabId);
    default:
      return null;
  }
}

async function downloadMediaElement(
  candidates: DetectedResource[],
  pageUrl: string,
  sourceContext: SourceContext,
  tab?: browser.tabs.Tab,
): Promise<unknown> {
  const sanitized = resources.merge(
    tab?.id ?? sourceContext.tabId ?? -1,
    candidates,
    100,
  );
  const direct =
    sanitized.find((resource) => resource.type === "manifest") ??
    sanitized.find(
      (resource) => resource.type === "video" || resource.type === "audio",
    ) ??
    (tab?.id === undefined
      ? undefined
      : resources
          .list(tab.id)
          .find(
            (resource) =>
              resource.type === "manifest" ||
              resource.type === "video" ||
              resource.type === "audio",
          ));
  if (direct) {
    const payload = await enrichDownload(
      {
        url: direct.url,
        kind: direct.type === "manifest" ? "media" : "http",
        referer: pageUrl,
        sourceContext,
      },
      tab,
    );
    const job = await native.request<{ id?: string }>(
      "create_download",
      payload,
    );
    void trackDownload(
      job?.id,
      downloadLabel({ url: direct.url, filename: direct.filename }),
    );
    return job;
  }
  // No direct media URL was found on the element — the common case is a
  // JS-driven player (YouTube and effectively every site with custom
  // controls) whose <video> src is a blob: URL that collectMediaSources()
  // already excludes as unusable. Hand the *page* itself to yt-dlp, exactly
  // like the "Download page with yt-dlp" context-menu item does. This used
  // to call probe_media instead — which only returns format metadata and
  // never creates a job — so the overlay button showed a success checkmark
  // on every such click while nothing was ever actually queued.
  const job = await native.request<{ id?: string }>(
    "create_download",
    await enrichDownload(
      {
        url: pageUrl,
        kind: "media",
        sourceContext,
      },
      tab,
    ),
  );
  void trackDownload(
    job?.id,
    sourceContext.pageTitle ?? downloadLabel({ url: pageUrl }),
  );
  return job;
}

async function enrichDownload(
  payload: CreateDownloadPayload,
  tab?: browser.tabs.Tab,
): Promise<CreateDownloadPayload> {
  const sourceContext: SourceContext = {
    ...payload.sourceContext,
    browser: "firefox",
    containerId: payload.sourceContext.containerId ?? tab?.cookieStoreId,
    incognito: payload.sourceContext.incognito || tab?.incognito === true,
    pageUrl: payload.sourceContext.pageUrl ?? tab?.url,
    pageTitle: payload.sourceContext.pageTitle ?? tab?.title,
    tabId: payload.sourceContext.tabId ?? tab?.id,
  };
  const cookies = await cookiesForUrl(payload.url, sourceContext.containerId);
  return {
    ...payload,
    sourceContext,
    cookies: cookies.length ? cookies : payload.cookies,
  };
}

async function enrichProbe(
  url: string,
  sourceContext: SourceContext,
  tab?: browser.tabs.Tab,
): Promise<Record<string, unknown>> {
  const context = {
    ...sourceContext,
    containerId: sourceContext.containerId ?? tab?.cookieStoreId,
    incognito: sourceContext.incognito || tab?.incognito === true,
  };
  return {
    url,
    sourceContext: context,
    cookies: await cookiesForUrl(url, context.containerId),
  };
}

async function cookiesForUrl(
  url: string,
  cookieStoreId?: string,
): Promise<CookieValue[]> {
  const settings = await loadSettings();
  let origin: string;
  try {
    origin = new URL(url).origin;
  } catch {
    return [];
  }
  if (!settings.allowCookiesByOrigin.includes(origin)) return [];
  const pattern = originPattern(url);
  if (
    !pattern ||
    !(await browser.permissions.contains({
      permissions: ["cookies"],
      origins: [pattern],
    }))
  )
    return [];
  const cookies = await browser.cookies
    .getAll({ url, storeId: cookieStoreId })
    .catch(() => []);
  return cookies.slice(0, 500).map((cookie) => ({
    name: cookie.name,
    value: cookie.value,
    domain: cookie.domain,
    path: cookie.path,
    secure: cookie.secure,
    httpOnly: cookie.httpOnly,
    sameSite: cookie.sameSite,
  }));
}

async function requestSitePermissions(
  url: string,
  cookies: boolean,
  networkPermission: boolean,
): Promise<boolean> {
  const pattern = originPattern(url);
  if (!pattern) return false;
  const permissions: browser._manifest.OptionalPermission[] = [];
  if (cookies) permissions.push("cookies");
  if (networkPermission) permissions.push("webRequest");
  const granted = await browser.permissions.request({
    permissions,
    origins: [pattern],
  });
  if (granted && cookies) {
    const origin = new URL(url).origin;
    const settings = await loadSettings();
    await saveSettings({
      allowCookiesByOrigin: [
        ...new Set([...settings.allowCookiesByOrigin, origin]),
      ],
    });
  }
  return granted;
}

async function activeTab(): Promise<browser.tabs.Tab | undefined> {
  return (await browser.tabs.query({ active: true, currentWindow: true }))[0];
}

async function downloadCurrentPage(): Promise<void> {
  const tab = await activeTab();
  if (!tab?.url) return;
  const job = await native.request<{ id?: string }>("create_download", {
    url: tab.url,
    kind: "media",
    sourceContext: {
      browser: "firefox",
      containerId: tab.cookieStoreId,
      incognito: tab.incognito,
      pageUrl: tab.url,
      pageTitle: tab.title,
      tabId: tab.id,
    },
  });
  void trackDownload(job?.id, tab.title ?? downloadLabel({ url: tab.url }));
}

async function updateBadge(tabId: number): Promise<void> {
  const count = resources
    .list(tabId)
    .filter(
      (resource) =>
        resource.type === "video" ||
        resource.type === "audio" ||
        resource.type === "manifest",
    ).length;
  await browser.action
    .setBadgeText({ tabId, text: count > 0 ? String(count) : "" })
    .catch(() => undefined);
}

async function broadcast(message: unknown): Promise<void> {
  await browser.runtime.sendMessage(message).catch(() => undefined);
}

async function removeOptionalPermissions(): Promise<void> {
  const granted = await browser.permissions.getAll();
  const permissions = (granted.permissions ?? []).filter(
    (permission): permission is browser._manifest.OptionalPermission =>
      permission === "cookies" || permission === "webRequest",
  );
  const origins = granted.origins ?? [];
  if (permissions.length || origins.length) {
    await browser.permissions
      .remove({ permissions, origins })
      .catch(() => false);
  }
}
