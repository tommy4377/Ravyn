import type {
  BackgroundRequest,
  ConnectionStatus,
  ExtensionSettings,
} from "../shared/contracts";

let settings: ExtensionSettings;
const message = element("message");

void initialize();

async function initialize(): Promise<void> {
  settings = (await send({ type: "get-settings" })) as ExtensionSettings;
  writeForm(settings);
  renderCookieOrigins(settings.allowCookiesByOrigin);
  bind();
  const status = (await send({
    type: "connection-status",
  })) as ConnectionStatus;
  renderConnection(status);
}

function bind(): void {
  element("save").addEventListener("click", () => void save());
  element("clear").addEventListener("click", () => void clearData());
  element("open-ravyn").addEventListener(
    "click",
    () => void send({ type: "open-ravyn" }),
  );
  element("grant-network").addEventListener("click", () => void grantNetwork());
}

async function save(): Promise<void> {
  const patch = readForm();
  const response = await send({ type: "save-settings", settings: patch });
  if (hasError(response)) return show(response.error.message);
  settings = response as ExtensionSettings;
  writeForm(settings);
  show("Settings saved.");
}

async function clearData(): Promise<void> {
  if (
    !window.confirm(
      "Clear all Ravyn extension settings, rule cache and site permissions list?",
    )
  )
    return;
  settings = (await send({
    type: "clear-extension-data",
  })) as ExtensionSettings;
  writeForm(settings);
  renderCookieOrigins([]);
  show("Extension data cleared.");
}

async function grantNetwork(): Promise<void> {
  const granted = await browser.permissions.request({
    permissions: ["webRequest"],
    origins: ["<all_urls>"],
  });
  element<HTMLInputElement>("network").checked = granted;
  if (granted) {
    // Persist immediately — leaving this to the Save button meant the
    // permission could be held but never actually activated if the user
    // closed the page right after granting it.
    settings = (await send({
      type: "save-settings",
      settings: { networkObservation: true },
    })) as ExtensionSettings;
  }
  show(
    granted
      ? "Network media detection permission granted."
      : "Permission was not granted.",
  );
}

function writeForm(value: ExtensionSettings): void {
  checked("automatic", value.automaticInterception);
  select("mode", value.interceptionMode);
  checked("private", value.includePrivateWindows);
  checked("erase", value.eraseDelegatedBrowserEntries);
  select("bypass-key", value.bypassModifierKey);
  element<HTMLTextAreaElement>("intercept-extensions").value =
    value.interceptExtensions.join(", ");
  number("min-size", Math.round(value.minInterceptSizeBytes / (1024 * 1024)));
  checked("media-detection", value.mediaDetection);
  checked("overlays", value.videoOverlays);
  checked("image-overlays", value.imageOverlays);
  number("overlay-width", value.overlayMinimumWidth);
  number("overlay-height", value.overlayMinimumHeight);
  checked("network", value.networkObservation);
  checked("notifications", value.notifications);
  number("max-resources", value.maxResourcesPerTab);
  element<HTMLTextAreaElement>("disabled-domains").value =
    value.disabledDomains.join("\n");
  element<HTMLTextAreaElement>("always-domains").value =
    value.alwaysInterceptDomains.join("\n");
}

function readForm(): Partial<ExtensionSettings> {
  return {
    automaticInterception: element<HTMLInputElement>("automatic").checked,
    interceptionMode: element<HTMLSelectElement>("mode")
      .value as ExtensionSettings["interceptionMode"],
    includePrivateWindows: element<HTMLInputElement>("private").checked,
    eraseDelegatedBrowserEntries: element<HTMLInputElement>("erase").checked,
    bypassModifierKey: element<HTMLSelectElement>("bypass-key")
      .value as ExtensionSettings["bypassModifierKey"],
    interceptExtensions: lines("intercept-extensions"),
    minInterceptSizeBytes:
      Number(element<HTMLInputElement>("min-size").value) * 1024 * 1024,
    mediaDetection: element<HTMLInputElement>("media-detection").checked,
    videoOverlays: element<HTMLInputElement>("overlays").checked,
    imageOverlays: element<HTMLInputElement>("image-overlays").checked,
    overlayMinimumWidth: Number(
      element<HTMLInputElement>("overlay-width").value,
    ),
    overlayMinimumHeight: Number(
      element<HTMLInputElement>("overlay-height").value,
    ),
    networkObservation: element<HTMLInputElement>("network").checked,
    notifications: element<HTMLInputElement>("notifications").checked,
    maxResourcesPerTab: Number(
      element<HTMLInputElement>("max-resources").value,
    ),
    disabledDomains: lines("disabled-domains"),
    alwaysInterceptDomains: lines("always-domains"),
  };
}

function renderCookieOrigins(origins: string[]): void {
  const container = element("cookie-origins");
  container.replaceChildren(
    ...origins.map((origin) => {
      const row = document.createElement("div");
      row.className = "origin";
      const text = document.createElement("span");
      text.textContent = origin;
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "Remove";
      remove.addEventListener("click", () => void removeCookieOrigin(origin));
      row.append(text, remove);
      return row;
    }),
  );
  if (!origins.length)
    container.innerHTML =
      '<span class="muted">No sites have cookie permission.</span>';
}

async function removeCookieOrigin(origin: string): Promise<void> {
  const next = settings.allowCookiesByOrigin.filter((item) => item !== origin);
  settings = (await send({
    type: "save-settings",
    settings: { allowCookiesByOrigin: next },
  })) as ExtensionSettings;
  const pattern = `${origin}/*`;
  await browser.permissions.remove({ origins: [pattern] }).catch(() => false);
  if (!next.length) {
    await browser.permissions
      .remove({ permissions: ["cookies"] })
      .catch(() => false);
  }
  renderCookieOrigins(next);
}

function renderConnection(status: ConnectionStatus): void {
  const container = element("connection");
  container.textContent = status.backendConnected
    ? "Connected to Ravyn"
    : status.hostAvailable
      ? "Native host installed · app offline"
      : "Native host not detected";
}

function checked(id: string, value: boolean): void {
  element<HTMLInputElement>(id).checked = value;
}
function select(id: string, value: string): void {
  element<HTMLSelectElement>(id).value = value;
}
function number(id: string, value: number): void {
  element<HTMLInputElement>(id).value = String(value);
}
function lines(id: string): string[] {
  return element<HTMLTextAreaElement>(id)
    .value.split(/\r?\n|,/)
    .map((item) => item.trim())
    .filter(Boolean);
}
function show(value: string): void {
  message.textContent = value;
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
