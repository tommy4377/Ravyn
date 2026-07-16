import type {
  BackgroundRequest,
  ConnectionStatus,
  DownloadSummary,
  SourceContext,
} from "../shared/contracts";

const connection = element("connection");
const message = element("message");
const jobs = element("jobs");
const summaryText = element("summary");
const urlInput = element<HTMLInputElement>("url");
let currentTab: browser.tabs.Tab | undefined;

void initialize();

async function initialize(): Promise<void> {
  currentTab = (
    await browser.tabs.query({ active: true, currentWindow: true })
  )[0];
  if (currentTab?.url)
    element("page-host").textContent = safeHost(currentTab.url);
  bind();
  await Promise.all([refreshConnection(), refreshSummary()]);
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
  element("sidebar").addEventListener(
    "click",
    () => void send({ type: "open-sidebar" }).then(() => window.close()),
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
  element("scan").addEventListener("click", () => void scanPage());
  element("images").addEventListener("click", () => void scanPage("image"));
  element("monitor").addEventListener("click", () => void monitorPage());
  element("analyze").addEventListener("click", () => void analyzePage());
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
  if (status.backendConnected) connection.textContent = "Connected to Ravyn";
  else if (status.hostAvailable)
    connection.textContent = "Ravyn is installed but not running";
  else connection.textContent = "Browser integration is unavailable";
}

async function refreshSummary(): Promise<void> {
  const response = await send({ type: "get-summary" });
  if (hasError(response)) {
    summaryText.textContent = "Unavailable";
    jobs.innerHTML = '<div class="empty">Open Ravyn to view jobs.</div>';
    return;
  }
  const summary = response as DownloadSummary;
  summaryText.textContent = `${summary.active} active · ${formatRate(summary.speedBps)}`;
  jobs.replaceChildren(...summary.recent.slice(0, 4).map(jobRow));
  if (!summary.recent.length)
    jobs.innerHTML = '<div class="empty">No recent jobs</div>';
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
  show("Media analysis completed. Open Ravyn for format details.");
  await send({ type: "open-ravyn", section: "media" });
}

async function scanPage(type?: string): Promise<void> {
  if (currentTab?.id === undefined) return;
  setBusy(true);
  const response = await send({
    type: "scan-tab",
    tabId: currentTab.id,
    fresh: true,
  });
  setBusy(false);
  if (hasError(response)) return show(response.error.message);
  if (type) await browser.storage.local.set({ "ravyn.sidebarType": type });
  await send({ type: "open-sidebar" });
  window.close();
}

async function monitorPage(): Promise<void> {
  if (currentTab?.id === undefined) return;
  await send({ type: "monitor-tab", tabId: currentTab.id, enabled: true });
  await send({ type: "open-sidebar" });
  window.close();
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

function safeHost(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
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
