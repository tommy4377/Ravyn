import type {
  BackgroundRequest,
  ConnectionStatus,
  CreateDownloadPayload,
  DetectedResource,
  DownloadPreset,
  DownloadSummary,
  ExtensionSettings,
  MediaFormat,
  MediaProbeResult,
  ResourceKind,
  SourceContext,
} from "../shared/contracts";

type PopupView = "overview" | "resources";

const FILTERS_KEY = "ravyn.popupFilters";
interface PersistedFilters {
  search: string;
  newOnly: boolean;
  minimumSize: string;
  maximumSize: string;
}

const connection = element("connection");
const message = element("message");
const jobs = element("jobs");
const summaryText = element("summary");
const urlInput = element<HTMLInputElement>("url");
const resourceList = element("resources");
const selected = new Set<string>();

let currentTab: browser.tabs.Tab | undefined;
let summaryRefreshTimer: number | undefined;
let settings: ExtensionSettings;
let allResources: DetectedResource[] = [];
let firstSeenAt = Date.now();
let resourcesLoaded = false;

void initialize();

async function initialize(): Promise<void> {
  currentTab = (
    await browser.tabs.query({ active: true, currentWindow: true })
  )[0];
  const pageLabel = currentTab?.url
    ? safeHost(currentTab.url)
    : "No active page";
  element("page-host").textContent = pageLabel;
  element("resource-page").textContent =
    currentTab?.title ?? currentTab?.url ?? "No active page";
  settings = (await send({ type: "get-settings" })) as ExtensionSettings;
  element<HTMLInputElement>("same-domain").checked = settings.sameDomainOnly;
  if (currentTab?.url) {
    const origin = safeOrigin(currentTab.url);
    element<HTMLInputElement>("include-cookies").checked =
      !!origin && settings.allowCookiesByOrigin.includes(origin);
  }
  const stored = await browser.storage.local.get([
    "ravyn.popupView",
    "ravyn.popupType",
    FILTERS_KEY,
  ]);
  const type: unknown = stored["ravyn.popupType"];
  if (typeof type === "string" && isResourceType(type))
    element<HTMLSelectElement>("resource-type").value = type;
  // Remembering the last filter set (DownThemAll-style) saves re-entering
  // the same size bounds/search every time the popup reopens.
  const filters = stored[FILTERS_KEY] as Partial<PersistedFilters> | undefined;
  if (filters) {
    element<HTMLInputElement>("resource-search").value = filters.search ?? "";
    element<HTMLInputElement>("new-only").checked = filters.newOnly ?? false;
    element<HTMLInputElement>("minimum-size").value = filters.minimumSize ?? "";
    element<HTMLInputElement>("maximum-size").value = filters.maximumSize ?? "";
  }
  bind();
  const initialView =
    stored["ravyn.popupView"] === "resources" ? "resources" : "overview";
  switchView(initialView, false);
  await Promise.all([
    refreshConnection(),
    refreshSummary(),
    refreshStreamHint(),
    loadPresets(),
  ]);
  if (initialView === "resources") await refreshResources(true);
  // Real push events (native-messaging now proxies the backend's SSE
  // stream) cover the common case immediately; this interval is only a
  // slow fallback for whatever push might miss (e.g. progress ticks that
  // don't cross a full percent, or the native host still reconnecting).
  summaryRefreshTimer = window.setInterval(() => void refreshSummary(), 20_000);
  window.addEventListener(
    "unload",
    () => {
      if (summaryRefreshTimer !== undefined)
        window.clearInterval(summaryRefreshTimer);
    },
    { once: true },
  );
}

function bind(): void {
  element<HTMLFormElement>("quick").addEventListener("submit", (event) => {
    event.preventDefault();
    void submitUrl(false);
  });
  element("paused").addEventListener("click", () => void submitUrl(true));
  element("settings").addEventListener(
    "click",
    () => void browser.runtime.openOptionsPage(),
  );
  element("open-ravyn").addEventListener(
    "click",
    () => void send({ type: "open-ravyn" }),
  );
  element("pause-all").addEventListener(
    "click",
    () => void send({ type: "pause-all" }).then(refreshSummary),
  );
  element("resume-all").addEventListener(
    "click",
    () => void send({ type: "resume-all" }).then(refreshSummary),
  );
  element("scan").addEventListener("click", () => void showResources());
  element("images").addEventListener(
    "click",
    () => void showResources("image"),
  );
  element("monitor").addEventListener("click", () => void monitorPage());
  element("analyze").addEventListener("click", () => void analyzePage());
  element("tab-overview").addEventListener("click", () =>
    switchView("overview"),
  );
  element("tab-resources").addEventListener("click", () =>
    switchView("resources"),
  );
  for (const id of ["tab-overview", "tab-resources"]) {
    element<HTMLButtonElement>(id).addEventListener("keydown", onTabKeydown);
  }

  element("resource-refresh").addEventListener(
    "click",
    () => void refreshResources(true),
  );
  for (const id of [
    "resource-search",
    "resource-type",
    "same-domain",
    "new-only",
    "minimum-size",
    "maximum-size",
  ])
    element(id).addEventListener("input", renderResources);
  for (const id of [
    "resource-search",
    "new-only",
    "minimum-size",
    "maximum-size",
  ])
    element(id).addEventListener("input", () => void persistFilters());
  element<HTMLSelectElement>("resource-type").addEventListener(
    "change",
    (event) => {
      void browser.storage.local.set({
        "ravyn.popupType": (event.currentTarget as HTMLSelectElement).value,
      });
    },
  );
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
      renderResources();
    },
  );
  element("download-selected").addEventListener(
    "click",
    () => void submitResources(false),
  );
  element("add-selected-paused").addEventListener(
    "click",
    () => void submitResources(true),
  );
  element("resources-monitor").addEventListener(
    "click",
    () => void toggleMonitor(),
  );
  element<HTMLInputElement>("include-cookies").addEventListener(
    "change",
    (event) =>
      void grantCookies((event.currentTarget as HTMLInputElement).checked),
  );
  browser.runtime.onMessage.addListener((incoming: unknown) => {
    const record = incoming as Record<string, unknown>;
    if (
      record?.type === "ravyn-resources-updated" &&
      record.tabId === currentTab?.id
    ) {
      void refreshResources(false);
      void refreshStreamHint();
    }
    // The native host now proxies real backend events, so a job/queue
    // change refreshes the summary immediately instead of waiting for the
    // next poll tick.
    if (record?.type === "ravyn-native-event") {
      const event = record.event as { event?: string } | undefined;
      if (
        event?.event === "job_status" ||
        event?.event === "queue_changed" ||
        event?.event === "progress"
      )
        void refreshSummary();
    }
  });
}

function switchView(view: PopupView, remember = true): void {
  const overview = view === "overview";
  element("overview-view").classList.toggle("hidden", !overview);
  element("resources-view").classList.toggle("hidden", overview);
  const overviewTab = element<HTMLButtonElement>("tab-overview");
  const resourcesTab = element<HTMLButtonElement>("tab-resources");
  overviewTab.setAttribute("aria-selected", String(overview));
  resourcesTab.setAttribute("aria-selected", String(!overview));
  overviewTab.tabIndex = overview ? 0 : -1;
  resourcesTab.tabIndex = overview ? -1 : 0;
  if (remember) void browser.storage.local.set({ "ravyn.popupView": view });
  if (!overview && !resourcesLoaded) void refreshResources(true);
}

function onTabKeydown(event: KeyboardEvent): void {
  let view: PopupView;
  if (event.key === "ArrowRight" || event.key === "End") view = "resources";
  else if (event.key === "ArrowLeft" || event.key === "Home") view = "overview";
  else return;
  event.preventDefault();
  switchView(view);
  element<HTMLButtonElement>(`tab-${view}`).focus();
}

async function showResources(
  type: ResourceKind | "all" = "all",
): Promise<void> {
  element<HTMLSelectElement>("resource-type").value = type;
  await browser.storage.local.set({ "ravyn.popupType": type });
  switchView("resources");
  await refreshResources(true);
}

async function submitUrl(paused: boolean): Promise<void> {
  const url = urlInput.value.trim();
  if (!url) return show("Enter a URL first.");
  setBusy(true);
  const response = await send({
    type: "download-url",
    payload: {
      url,
      paused,
      sourceContext: sourceContext(currentTab),
    },
  });
  setBusy(false);
  if (hasError(response)) return show(response.error.message);
  urlInput.value = "";
  show(
    paused ? "Added to Ravyn in a paused state." : "Download sent to Ravyn.",
  );
  await refreshSummary();
}

async function refreshConnection(): Promise<void> {
  const status = (await send({
    type: "connection-status",
  })) as ConnectionStatus;
  const dot = element("connection-dot");
  dot.classList.remove("connected", "error");
  if (status.backendConnected) {
    connection.textContent = "Connected";
    dot.classList.add("connected");
  } else if (status.hostAvailable) {
    connection.textContent = "Ravyn is not running";
  } else {
    connection.textContent = "Integration unavailable";
    dot.classList.add("error");
  }
}

async function loadPresets(): Promise<void> {
  const response = await send({ type: "get-presets" });
  if (!Array.isArray(response)) return;
  const presets = response as DownloadPreset[];
  const select = element<HTMLSelectElement>("preset");
  const current = select.value;
  select.replaceChildren(
    new Option("No preset", ""),
    ...presets.map((preset) => new Option(preset.name, preset.id)),
  );
  if (presets.some((preset) => preset.id === current)) select.value = current;
}

async function refreshStreamHint(): Promise<void> {
  const detected =
    currentTab?.id !== undefined &&
    (await send({ type: "get-stream-hint", tabId: currentTab.id })) === true;
  element("stream-hint").classList.toggle("hidden", !detected);
}

async function refreshSummary(): Promise<void> {
  const response = await send({ type: "get-summary" });
  if (hasError(response)) {
    summaryText.textContent = "Unavailable";
    jobs.innerHTML =
      '<div class="empty compact-empty">Open Ravyn to view jobs.</div>';
    return;
  }
  const summary = response as DownloadSummary;
  summaryText.textContent = `${summary.active} active · ${formatRate(summary.speedBps)}`;
  jobs.replaceChildren(...summary.recent.slice(0, 4).map(jobRow));
  if (!summary.recent.length)
    jobs.innerHTML = '<div class="empty compact-empty">No recent jobs</div>';
}

async function analyzePage(): Promise<void> {
  if (!currentTab?.url) return;
  setBusy(true);
  const response = await send({
    type: "probe-media",
    url: currentTab.url,
    sourceContext: sourceContext(currentTab),
  });
  setBusy(false);
  if (hasError(response)) return show(response.error.message);
  const probe = response as MediaProbeResult;
  renderFormats(probe.formats ?? []);
  show(
    probe.formats?.length
      ? `${probe.formats.length} format${probe.formats.length === 1 ? "" : "s"} found — choose one below.`
      : "No downloadable formats found on this page.",
  );
}

function renderFormats(formats: MediaFormat[]): void {
  const container = element("formats");
  if (!formats.length) {
    container.classList.add("hidden");
    container.replaceChildren();
    return;
  }
  const sorted = [...formats].sort(
    (left, right) =>
      (right.height ?? 0) - (left.height ?? 0) ||
      (right.bitrateKbps ?? 0) - (left.bitrateKbps ?? 0),
  );
  container.replaceChildren(...sorted.map(formatRow));
  container.classList.remove("hidden");
}

function formatRow(format: MediaFormat): HTMLElement {
  const row = document.createElement("div");
  row.className = "format-row";
  const copy = document.createElement("div");
  copy.className = "format-copy";
  const label = document.createElement("div");
  label.className = "format-label";
  label.textContent = formatQualityLabel(format);
  const meta = document.createElement("div");
  meta.className = "format-meta";
  meta.textContent = formatQualityMeta(format);
  copy.append(label, meta);
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = "Download";
  button.addEventListener("click", () => void downloadFormat(format));
  row.append(copy, button);
  return row;
}

function formatQualityLabel(format: MediaFormat): string {
  if (format.height)
    return `${format.height}p${format.fps && format.fps > 30 ? ` ${Math.round(format.fps)}fps` : ""}`;
  if (format.audioCodec && !format.videoCodec) return "Audio only";
  return format.note ?? format.formatId;
}

function formatQualityMeta(format: MediaFormat): string {
  const parts: string[] = [];
  if (format.extension) parts.push(format.extension.toUpperCase());
  if (format.videoCodec) parts.push(format.videoCodec);
  if (format.audioCodec && format.height) parts.push(format.audioCodec);
  if (format.filesize) parts.push(formatSize(format.filesize));
  return parts.join(" · ") || format.formatId;
}

async function downloadFormat(format: MediaFormat): Promise<void> {
  if (!currentTab?.url) return;
  setBusy(true);
  const response = await send({
    type: "download-url",
    payload: {
      url: currentTab.url,
      kind: "media",
      media: { format: format.formatId },
      sourceContext: sourceContext(currentTab),
    },
  });
  setBusy(false);
  if (hasError(response)) return show(response.error.message);
  show(`Sent to Ravyn: ${formatQualityLabel(format)}.`);
  await refreshSummary();
}

async function monitorPage(): Promise<void> {
  if (currentTab?.id === undefined) return;
  await send({ type: "monitor-tab", tabId: currentTab.id, enabled: true });
  element<HTMLButtonElement>("resources-monitor").dataset.enabled = "true";
  element("resources-monitor").textContent = "Stop monitoring";
  firstSeenAt = Date.now();
  switchView("resources");
  await refreshResources(false);
  show("Monitoring newly added page resources.");
}

async function refreshResources(scan: boolean): Promise<void> {
  if (currentTab?.id === undefined) {
    allResources = [];
    resourcesLoaded = true;
    renderResources();
    return;
  }
  show(scan ? "Scanning page…" : "Updating resources…");
  if (scan) await send({ type: "scan-tab", tabId: currentTab.id });
  const response = await send({
    type: "get-tab-resources",
    tabId: currentTab.id,
  });
  if (hasError(response)) return show(response.error.message);
  allResources = response as DetectedResource[];
  resourcesLoaded = true;
  renderResources();
  show(`${allResources.length} resources detected.`);
}

function renderResources(): void {
  const resources = filteredResources();
  resourceList.replaceChildren(...resources.map(resourceRow));
  if (!resources.length)
    resourceList.innerHTML =
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
  const hasSelection = selected.size > 0;
  element<HTMLButtonElement>("download-selected").disabled = !hasSelection;
  element<HTMLButtonElement>("add-selected-paused").disabled = !hasSelection;
}

async function persistFilters(): Promise<void> {
  const filters: PersistedFilters = {
    search: element<HTMLInputElement>("resource-search").value,
    newOnly: element<HTMLInputElement>("new-only").checked,
    minimumSize: element<HTMLInputElement>("minimum-size").value,
    maximumSize: element<HTMLInputElement>("maximum-size").value,
  };
  await browser.storage.local.set({ [FILTERS_KEY]: filters });
}

function filteredResources(): DetectedResource[] {
  const query = element<HTMLInputElement>("resource-search")
    .value.trim()
    .toLowerCase();
  const type = element<HTMLSelectElement>("resource-type").value as
    ResourceKind | "all";
  const sameDomain = element<HTMLInputElement>("same-domain").checked;
  const newOnly = element<HTMLInputElement>("new-only").checked;
  const minimum =
    Number(element<HTMLInputElement>("minimum-size").value) * 1024 * 1024;
  const maximum =
    Number(element<HTMLInputElement>("maximum-size").value) * 1024 * 1024;
  const pageHost = currentTab?.url ? safeHost(currentTab.url) : "";
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
    renderResources();
  });
  const preview = previewFor(resource);
  const content = document.createElement("div");
  content.className = "resource-copy";
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
  row.append(checkbox, preview, content);
  return row;
}

// Deferring the actual network request until each preview scrolls into
// view — instead of setting `src` for every row up front — keeps opening
// the Resources tab from firing potentially hundreds of live GET requests
// (cookies included) to third-party origins in one burst.
const previewObserver = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const target = entry.target as HTMLImageElement;
      const src = target.dataset.previewSrc;
      if (src) {
        target.src = src;
        delete target.dataset.previewSrc;
      }
      previewObserver.unobserve(target);
    }
  },
  { rootMargin: "200px" },
);

function previewFor(resource: DetectedResource): HTMLElement {
  if (resource.type === "image") {
    const image = document.createElement("img");
    image.className = "preview";
    image.loading = "lazy";
    image.referrerPolicy = "no-referrer";
    image.dataset.previewSrc = resource.url;
    image.alt = "";
    image.addEventListener("error", () =>
      image.replaceWith(genericPreview(resource.type)),
    );
    previewObserver.observe(image);
    return image;
  }
  return genericPreview(resource.type);
}

function genericPreview(type: string): HTMLElement {
  const preview = document.createElement("div");
  preview.className = "preview generic";
  preview.textContent = type.toUpperCase().slice(0, 4);
  return preview;
}

async function submitResources(paused: boolean): Promise<void> {
  const chosen = allResources.filter((resource) => selected.has(resource.id));
  if (!chosen.length) return show("Select at least one resource.");
  const presetId = element<HTMLSelectElement>("preset").value || undefined;
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
  const context = sourceContext(currentTab);
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
  setBusy(true);
  show(`Sending ${downloads.length} resources to Ravyn…`);
  const response = await send({
    type: "download-batch",
    payload: { downloads },
  });
  setBusy(false);
  if (hasError(response)) return show(response.error.message);
  const result = response as {
    attempted?: number;
    accepted?: number;
    failed?: number;
    results?: Array<{ ok?: boolean }>;
  };
  const accepted = result.accepted ?? 0;
  const failed = result.failed ?? Math.max(0, downloads.length - accepted);
  if (failed > 0) {
    show(
      `${accepted} accepted by Ravyn · ${failed} failed. Failed resources remain selected.`,
    );
    const nextSelected = new Set<string>();
    result.results?.forEach((entry, index) => {
      if (entry?.ok !== true) {
        const resource = chosen[index];
        if (resource) nextSelected.add(resource.id);
      }
    });
    selected.clear();
    for (const id of nextSelected) selected.add(id);
  } else {
    show(`${accepted || downloads.length} resources accepted by Ravyn.`);
    selected.clear();
  }
  renderResources();
  await refreshSummary();
}

async function grantCookies(enabled: boolean): Promise<void> {
  if (!currentTab?.url) return;
  const origin = safeOrigin(currentTab.url);
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
    if (!settings.allowCookiesByOrigin.length)
      await browser.permissions
        .remove({ permissions: ["cookies"] })
        .catch(() => false);
    show("Session cookie access disabled for this site.");
    return;
  }
  const granted = await send({
    type: "request-site-permissions",
    url: currentTab.url,
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
  if (currentTab?.id === undefined) return;
  const button = element<HTMLButtonElement>("resources-monitor");
  const enabled = button.dataset.enabled !== "true";
  await send({ type: "monitor-tab", tabId: currentTab.id, enabled });
  button.dataset.enabled = String(enabled);
  button.textContent = enabled ? "Stop monitoring" : "Monitor";
  firstSeenAt = Date.now();
  show(
    enabled ? "Monitoring newly added resources." : "Page monitoring stopped.",
  );
}

function jobRow(job: DownloadSummary["recent"][number]): HTMLElement {
  const row = document.createElement("div");
  row.className = "job";
  const name = document.createElement("div");
  name.className = "job-name";
  name.textContent = job.filename;
  name.title = job.filename;
  const status = document.createElement("span");
  status.className = "badge";
  status.textContent =
    job.progress === null ? job.status : `${Math.round(job.progress * 100)}%`;
  const meta = document.createElement("div");
  meta.className = "job-meta";
  meta.textContent = job.speedBps
    ? formatRate(job.speedBps)
    : job.status.replaceAll("_", " ");
  row.append(name, status, meta);
  return row;
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

function setBusy(value: boolean): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>("button"))
    button.disabled = value;
}

function show(value: string): void {
  message.textContent = value;
}

function badge(value: string): HTMLElement {
  const item = document.createElement("span");
  item.className = "badge";
  item.textContent = value;
  return item;
}

function formatRate(value: number): string {
  if (value <= 0) return "0 B/s";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let amount = value;
  let index = 0;
  while (amount >= 1024 && index < units.length - 1) {
    amount /= 1024;
    index += 1;
  }
  return `${amount.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
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

function isResourceType(value: string): value is ResourceKind | "all" {
  return [
    "all",
    "image",
    "video",
    "audio",
    "manifest",
    "document",
    "archive",
    "other",
  ].includes(value);
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
