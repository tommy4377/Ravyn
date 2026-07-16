import type {
  BackgroundRequest,
  ConnectionStatus,
  CookieValue,
  CreateBatchPayload,
  CreateDownloadPayload,
  DetectedResource,
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
import { DelegationRegistry } from "./downloads/delegation";
import { DownloadInterceptor } from "./downloads/interceptor";
import { registerMenuHandlers } from "./menus/handlers";
import { registerMenus } from "./menus/register";
import { NativeClient } from "./native/client";
import { ResourceCache } from "./network/cache";
import { NetworkObserver } from "./network/observer";
import { RuleCache } from "./rules/cache";

const native = new NativeClient();
const resources = new ResourceCache();
const rules = new RuleCache(native);
const network = new NetworkObserver(resources);
const interceptor = new DownloadInterceptor(
  native,
  rules,
  new DelegationRegistry(),
);

void initialize();

async function initialize(): Promise<void> {
  const settings = await loadSettings();
  await registerMenus();
  registerMenuHandlers(native, resources);
  interceptor.register();
  await network.synchronize(settings);
  native.subscribeStatus((status) => updateBadge(status));
  native.subscribeEvents((event) => {
    if (event.event.startsWith("rule.")) rules.invalidate();
    void broadcast({ type: "ravyn-native-event", event });
  });
  void native.connect().catch(() => undefined);
  browser.tabs.onRemoved.addListener((tabId) => resources.clear(tabId));
  browser.commands.onCommand.addListener((command) => {
    if (command === "open-sidebar") void browser.sidebarAction.open();
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
    case "download-url":
      return native.request(
        "create_download",
        await enrichDownload(
          validateDownloadPayload(request.payload),
          sender.tab,
        ),
      );
    case "download-batch": {
      const batch = validateBatchPayload(request.payload);
      const downloads = await Promise.all(
        batch.downloads.map((download) => enrichDownload(download, sender.tab)),
      );
      return native.request("create_batch", {
        downloads,
      } satisfies CreateBatchPayload);
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
      return resources.merge(
        tabId,
        detected,
        (await loadSettings()).maxResourcesPerTab,
      );
    }
    case "get-tab-resources": {
      const tabId = request.tabId ?? sender.tab?.id ?? (await activeTab())?.id;
      return tabId === undefined ? [] : resources.list(tabId);
    }
    case "resources-detected": {
      const tabId = request.tabId ?? sender.tab?.id;
      if (tabId === undefined) return [];
      const merged = resources.merge(
        tabId,
        request.resources,
        (await loadSettings()).maxResourcesPerTab,
      );
      await broadcast({ type: "ravyn-resources-updated", tabId });
      return merged;
    }
    case "open-sidebar":
      await browser.sidebarAction.open();
      return null;
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
      const next = await saveSettings(request.settings);
      await network.synchronize(next);
      await broadcast({ type: "update-settings", settings: next });
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
      resources.clearAll();
      await removeOptionalPermissions();
      return loadSettings();
    case "confirmation-result":
      interceptor.resolveConfirmation(request.requestId, request.accepted);
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
    return native.request(
      "create_download",
      await enrichDownload(
        {
          url: direct.url,
          kind: direct.type === "manifest" ? "media" : "http",
          referer: pageUrl,
          sourceContext,
        },
        tab,
      ),
    );
  }
  return native.request(
    "probe_media",
    await enrichProbe(pageUrl, sourceContext, tab),
  );
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
  await native.request("create_download", {
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
}

function updateBadge(status: ConnectionStatus): void {
  const text = status.backendConnected ? "" : status.hostAvailable ? "!" : "×";
  void browser.action.setBadgeText({ text });
  void browser.action.setBadgeBackgroundColor({
    color: status.backendConnected ? "#2f7d32" : "#a33a3a",
  });
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
