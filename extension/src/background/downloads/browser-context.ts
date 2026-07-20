import type {
  CookieValue,
  CreateDownloadPayload,
  SourceContext,
} from "../../shared/contracts";
import { loadSettings } from "../../shared/settings";
import { originPattern } from "../../shared/urls";

/** Enriches a download with the current Firefox tab context and opt-in cookies. */
export async function enrichDownload(
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

/** Enriches a media probe using the same cookie policy as download creation. */
export async function enrichProbe(
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

/** Returns browser cookies only for origins the user explicitly allowed. */
export async function cookiesForUrl(
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
  return cookies.slice(0, 100).map((cookie) => ({
    name: cookie.name,
    value: cookie.value,
    domain: cookie.domain,
    path: cookie.path,
    secure: cookie.secure,
    httpOnly: cookie.httpOnly,
    sameSite: cookie.sameSite,
    hostOnly: cookie.hostOnly,
  }));
}
