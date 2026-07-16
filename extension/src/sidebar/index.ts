import type {
  BackgroundRequest,
  CreateDownloadPayload,
  DetectedResource,
  ExtensionSettings,
  ResourceKind,
  SourceContext,
} from "../shared/contracts";

const list = element("resources");
const message = element("message");
const selected = new Set<string>();
let allResources: DetectedResource[] = [];
let firstSeenAt = Date.now();
let activeTab: browser.tabs.Tab | undefined;
let settings: ExtensionSettings;

void initialize();

async function initialize(): Promise<void> {
  activeTab = (
    await browser.tabs.query({ active: true, currentWindow: true })
  )[0];
  element("page").textContent =
    activeTab?.title ?? activeTab?.url ?? "No active page";
  settings = (await send({ type: "get-settings" })) as ExtensionSettings;
  element<HTMLInputElement>("same-domain").checked = settings.sameDomainOnly;
  if (activeTab?.url) {
    const origin = safeOrigin(activeTab.url);
    element<HTMLInputElement>("include-cookies").checked =
      !!origin && settings.allowCookiesByOrigin.includes(origin);
  }
  const stored = await browser.storage.local.get("ravyn.sidebarType");
  if (typeof stored["ravyn.sidebarType"] === "string")
    element<HTMLSelectElement>("type").value = stored["ravyn.sidebarType"];
  bind();
  await refresh(true);
}

function bind(): void {
  element("refresh").addEventListener("click", () => void refresh(true));
  element("options").addEventListener(
    "click",
    () => void browser.runtime.openOptionsPage(),
  );
  for (const id of [
    "search",
    "type",
    "same-domain",
    "new-only",
    "minimum-size",
    "maximum-size",
  ])
    element(id).addEventListener("input", render);
  element<HTMLSelectElement>("type").addEventListener("change", (event) => {
    void browser.storage.local.set({
      "ravyn.sidebarType": (event.currentTarget as HTMLSelectElement).value,
    });
  });
  element<HTMLInputElement>("same-domain").addEventListener(
    "change",
    (event) => {
      void send({
        type: "save-settings",
        settings: {
          sameDomainOnly: (event.currentTarget as HTMLInputElement).checked,
        },
      });
    },
  );
  element<HTMLInputElement>("select-all").addEventListener(
    "change",
    (event) => {
      const checked = (event.currentTarget as HTMLInputElement).checked;
      for (const resource of filteredResources()) {
        if (checked) selected.add(resource.id);
        else selected.delete(resource.id);
      }
      render();
    },
  );
  element("download").addEventListener("click", () => void submit(false));
  element("add-paused").addEventListener("click", () => void submit(true));
  element("monitor").addEventListener("click", () => void toggleMonitor());
  element<HTMLInputElement>("include-cookies").addEventListener(
    "change",
    (event) =>
      void grantCookies((event.currentTarget as HTMLInputElement).checked),
  );
  browser.runtime.onMessage.addListener((incoming: unknown) => {
    const record = incoming as Record<string, unknown>;
    if (
      record?.type === "ravyn-resources-updated" &&
      record.tabId === activeTab?.id
    )
      void refresh(false);
  });
}

async function refresh(scan: boolean): Promise<void> {
  if (activeTab?.id === undefined) return;
  show(scan ? "Scanning page…" : "Updating resources…");
  if (scan) await send({ type: "scan-tab", tabId: activeTab.id, fresh: true });
  const response = await send({
    type: "get-tab-resources",
    tabId: activeTab.id,
  });
  if (hasError(response)) return show(response.error.message);
  allResources = response as DetectedResource[];
  render();
  show(`${allResources.length} resources detected.`);
}

function render(): void {
  const resources = filteredResources();
  list.replaceChildren(...resources.map(resourceRow));
  if (!resources.length)
    list.innerHTML =
      '<div class="empty">No resources match these filters.</div>';
  const selectedVisible = resources.filter((resource) =>
    selected.has(resource.id),
  ).length;
  element("count").textContent =
    `${resources.length} resources · ${selectedVisible} selected`;
  const selectAll = element<HTMLInputElement>("select-all");
  selectAll.checked =
    resources.length > 0 && selectedVisible === resources.length;
  selectAll.indeterminate =
    selectedVisible > 0 && selectedVisible < resources.length;
}

function filteredResources(): DetectedResource[] {
  const query = element<HTMLInputElement>("search").value.trim().toLowerCase();
  const type = element<HTMLSelectElement>("type").value as ResourceKind | "all";
  const sameDomain = element<HTMLInputElement>("same-domain").checked;
  const newOnly = element<HTMLInputElement>("new-only").checked;
  const minimum =
    Number(element<HTMLInputElement>("minimum-size").value) * 1024 * 1024;
  const maximum =
    Number(element<HTMLInputElement>("maximum-size").value) * 1024 * 1024;
  const pageHost = activeTab?.url ? safeHost(activeTab.url) : "";
  return allResources.filter((resource) => {
    if (type !== "all" && resource.type !== type) return false;
    if (
      query &&
      !`${resource.filename ?? ""} ${resource.url} ${resource.mime ?? ""}`
        .toLowerCase()
        .includes(query)
    )
      return false;
    if (sameDomain && safeHost(resource.url) !== pageHost) return false;
    if (newOnly && resource.discoveredAt < firstSeenAt) return false;
    if (minimum > 0 && (resource.size ?? 0) < minimum) return false;
    if (maximum > 0 && resource.size !== undefined && resource.size > maximum)
      return false;
    return true;
  });
}

function resourceRow(resource: DetectedResource): HTMLElement {
  const row = document.createElement("label");
  row.className = "resource";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = selected.has(resource.id);
  checkbox.addEventListener("change", () => {
    if (checkbox.checked) selected.add(resource.id);
    else selected.delete(resource.id);
    render();
  });
  const preview = previewFor(resource);
  const content = document.createElement("div");
  const title = document.createElement("div");
  title.className = "resource-title";
  title.textContent = resource.filename ?? resource.title ?? resource.type;
  title.title = title.textContent;
  const url = document.createElement("div");
  url.className = "resource-url";
  url.textContent = resource.url;
  url.title = resource.url;
  const meta = document.createElement("div");
  meta.className = "resource-meta";
  meta.append(
    badge(resource.type),
    badge(resource.extension?.toUpperCase() ?? resource.mime ?? "URL"),
  );
  if (resource.size !== undefined)
    meta.append(badge(formatSize(resource.size)));
  content.append(title, url, meta);
  const source = badge(resource.source);
  row.append(checkbox, preview, content, source);
  return row;
}

function previewFor(resource: DetectedResource): HTMLElement {
  if (resource.type === "image") {
    const image = document.createElement("img");
    image.className = "preview";
    image.loading = "lazy";
    image.referrerPolicy = "no-referrer";
    image.src = resource.url;
    image.alt = "";
    image.addEventListener("error", () =>
      image.replaceWith(genericPreview(resource.type)),
    );
    return image;
  }
  return genericPreview(resource.type);
}

function genericPreview(type: string): HTMLElement {
  const preview = document.createElement("div");
  preview.className = "preview generic";
  preview.textContent = type.toUpperCase().slice(0, 5);
  return preview;
}

async function submit(paused: boolean): Promise<void> {
  const chosen = allResources.filter((resource) => selected.has(resource.id));
  if (!chosen.length) return show("Select at least one resource.");
  const presetId =
    element<HTMLInputElement>("preset").value.trim() || undefined;
  const tags = element<HTMLInputElement>("tags")
    .value.split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);
  const audioOnly = element<HTMLInputElement>("audio-only").checked;
  const subtitles = element<HTMLInputElement>("subtitles").checked;
  const mediaFormat =
    element<HTMLInputElement>("media-format").value.trim() || undefined;
  const maximumHeight =
    Number(element<HTMLSelectElement>("max-height").value) || undefined;
  const postProcessingPreset =
    (element<HTMLSelectElement>("post-processing")
      .value as CreateDownloadPayload["postProcessingPreset"]) || undefined;
  const context = sourceContext(activeTab);
  const downloads: CreateDownloadPayload[] = chosen.map((resource) => ({
    url: resource.url,
    kind:
      resource.type === "manifest" ||
      audioOnly ||
      subtitles ||
      !!mediaFormat ||
      !!maximumHeight
        ? "media"
        : "http",
    filename: resource.filename,
    paused,
    presetId,
    tags,
    referer: resource.pageUrl,
    postProcessingPreset,
    media:
      audioOnly || subtitles || !!mediaFormat || !!maximumHeight
        ? {
            format: mediaFormat,
            maxHeight: maximumHeight,
            audioOnly,
            audioFormat: audioOnly ? "mp3" : undefined,
            writeSubtitles: subtitles,
            subtitleLanguages: subtitles ? ["all"] : undefined,
          }
        : undefined,
    sourceContext: context,
  }));
  show(`Sending ${downloads.length} resources to Ravyn…`);
  const response = await send({
    type: "download-batch",
    payload: { downloads },
  });
  if (hasError(response)) return show(response.error.message);
  show(`${downloads.length} resources accepted by Ravyn.`);
  selected.clear();
  render();
}

async function grantCookies(enabled: boolean): Promise<void> {
  if (!activeTab?.url) return;
  const origin = safeOrigin(activeTab.url);
  const pattern = origin ? `${origin}/*` : null;
  if (!enabled) {
    if (origin) {
      const next = settings.allowCookiesByOrigin.filter(
        (item) => item !== origin,
      );
      settings = (await send({
        type: "save-settings",
        settings: { allowCookiesByOrigin: next },
      })) as ExtensionSettings;
    }
    if (pattern)
      await browser.permissions
        .remove({ origins: [pattern] })
        .catch(() => false);
    if (!settings.allowCookiesByOrigin.length) {
      await browser.permissions
        .remove({ permissions: ["cookies"] })
        .catch(() => false);
    }
    show("Session cookie access disabled for this site.");
    return;
  }
  const granted = await send({
    type: "request-site-permissions",
    url: activeTab.url,
    cookies: true,
    network: false,
  });
  if (granted !== true) {
    element<HTMLInputElement>("include-cookies").checked = false;
    show("Cookie access was not granted.");
  } else {
    settings = (await send({ type: "get-settings" })) as ExtensionSettings;
    show("Session cookies are enabled for this site only.");
  }
}

async function toggleMonitor(): Promise<void> {
  if (activeTab?.id === undefined) return;
  const button = element<HTMLButtonElement>("monitor");
  const enabled = button.dataset.enabled !== "true";
  await send({ type: "monitor-tab", tabId: activeTab.id, enabled });
  button.dataset.enabled = String(enabled);
  button.textContent = enabled ? "Stop monitoring" : "Monitor";
  firstSeenAt = Date.now();
  show(
    enabled
      ? "Monitoring newly added page resources."
      : "Page monitoring stopped.",
  );
}

function badge(value: string): HTMLElement {
  const item = document.createElement("span");
  item.className = "badge";
  item.textContent = value;
  return item;
}

function show(value: string): void {
  message.textContent = value;
}

function formatSize(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MB`;
  return `${(value / 1024 ** 3).toFixed(1)} GB`;
}

function safeHost(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

function safeOrigin(url: string): string | null {
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

function sourceContext(tab: browser.tabs.Tab | undefined): SourceContext {
  return {
    browser: "firefox",
    containerId: tab?.cookieStoreId,
    incognito: tab?.incognito ?? false,
    pageUrl: tab?.url,
    pageTitle: tab?.title,
    tabId: tab?.id,
  };
}

function hasError(value: unknown): value is { error: { message: string } } {
  return !!value && typeof value === "object" && "error" in value;
}

function send(request: BackgroundRequest): Promise<unknown> {
  return browser.runtime.sendMessage(request);
}

function element<T extends HTMLElement = HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error(`Missing element #${id}`);
  return value as T;
}
