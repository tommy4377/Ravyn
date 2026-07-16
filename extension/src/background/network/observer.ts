import type {
  DetectedResource,
  ExtensionSettings,
} from "../../shared/contracts";
import {
  extensionFromUrl,
  filenameFromUrl,
  normalizeUrl,
} from "../../shared/urls";
import { classifyObservedRequest } from "./classifier";
import type { ResourceCache } from "./cache";

interface PendingRequest {
  requestId: string;
  tabId: number;
  frameId: number;
  url: string;
  documentUrl?: string;
  requestType?: string;
  startedAt: number;
  mime?: string;
  size?: number;
  filename?: string;
}

export class NetworkObserver {
  private pending = new Map<string, PendingRequest>();
  private registered = false;

  constructor(private readonly cache: ResourceCache) {}

  async synchronize(settings: ExtensionSettings): Promise<void> {
    const hasPermissions = await browser.permissions.contains({
      permissions: ["webRequest"],
      origins: ["<all_urls>"],
    });
    const shouldRegister = settings.networkObservation && hasPermissions;
    if (shouldRegister && !this.registered) this.register();
    if (!shouldRegister && this.registered) this.unregister();
  }

  private register(): void {
    browser.webRequest.onBeforeRequest.addListener(this.onBeforeRequest, {
      urls: ["<all_urls>"],
    });
    browser.webRequest.onHeadersReceived.addListener(
      this.onHeadersReceived,
      { urls: ["<all_urls>"] },
      ["responseHeaders"],
    );
    browser.webRequest.onCompleted.addListener(this.onCompleted, {
      urls: ["<all_urls>"],
    });
    browser.webRequest.onErrorOccurred.addListener(this.onError, {
      urls: ["<all_urls>"],
    });
    this.registered = true;
  }

  private unregister(): void {
    browser.webRequest.onBeforeRequest.removeListener(this.onBeforeRequest);
    browser.webRequest.onHeadersReceived.removeListener(this.onHeadersReceived);
    browser.webRequest.onCompleted.removeListener(this.onCompleted);
    browser.webRequest.onErrorOccurred.removeListener(this.onError);
    this.pending.clear();
    this.registered = false;
  }

  private readonly onBeforeRequest = (
    details: browser.webRequest._OnBeforeRequestDetails,
  ): void => {
    if (details.tabId < 0 || details.method !== "GET") return;
    const url = normalizeUrl(details.url);
    if (!url) return;
    this.pending.set(details.requestId, {
      requestId: details.requestId,
      tabId: details.tabId,
      frameId: details.frameId,
      url,
      documentUrl: details.documentUrl,
      requestType: details.type,
      startedAt: Date.now(),
    });
  };

  private readonly onHeadersReceived = (
    details: browser.webRequest._OnHeadersReceivedDetails,
  ): void => {
    const request = this.pending.get(details.requestId);
    if (!request) return;
    request.mime = header(details.responseHeaders, "content-type")
      ?.split(";", 1)[0]
      ?.trim();
    const length = Number(header(details.responseHeaders, "content-length"));
    if (Number.isFinite(length) && length >= 0) request.size = length;
    request.filename = filenameFromDisposition(
      header(details.responseHeaders, "content-disposition"),
    );
  };

  private readonly onCompleted = (
    details: browser.webRequest._OnCompletedDetails,
  ): void => {
    const request = this.pending.get(details.requestId);
    this.pending.delete(details.requestId);
    if (!request) return;
    const classification = classifyObservedRequest(
      request.url,
      request.mime,
      request.requestType,
    );
    if (classification.ignore) return;
    const resource: DetectedResource = {
      id: `network:${request.requestId}`,
      url: request.url,
      normalizedUrl: request.url,
      pageUrl: request.documentUrl ?? request.url,
      frameUrl: request.documentUrl,
      type: classification.kind,
      mime: request.mime,
      extension: extensionFromUrl(request.url),
      filename: request.filename ?? filenameFromUrl(request.url),
      size: request.size,
      source: "webRequest",
      confidence: request.mime ? 0.95 : 0.75,
      discoveredAt: Date.now(),
    };
    this.cache.merge(request.tabId, [resource]);
    void browser.runtime
      .sendMessage({ type: "ravyn-resources-updated", tabId: request.tabId })
      .catch(() => undefined);
  };

  private readonly onError = (
    details: browser.webRequest._OnErrorOccurredDetails,
  ): void => {
    this.pending.delete(details.requestId);
  };
}

function header(
  headers: browser.webRequest.HttpHeaders | undefined,
  name: string,
): string | undefined {
  return headers?.find((item) => item.name.toLowerCase() === name)?.value;
}

function filenameFromDisposition(
  value: string | undefined,
): string | undefined {
  if (!value) return undefined;
  const utf8 = /filename\*=UTF-8''([^;]+)/i.exec(value)?.[1];
  if (utf8) {
    try {
      return decodeURIComponent(utf8).slice(0, 255);
    } catch {
      return utf8.slice(0, 255);
    }
  }
  return /filename="?([^";]+)"?/i.exec(value)?.[1]?.trim().slice(0, 255);
}
