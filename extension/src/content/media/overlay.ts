import type { ExtensionSettings } from "../../shared/contracts";
import { collectMediaSources } from "./source-collector";

const HOST_ATTRIBUTE = "data-ravyn-media-control";

interface AttachedControl {
  host: HTMLElement;
  update(): void;
  show(): void;
  hide(): void;
  dispose(): void;
}

export class MediaOverlayController {
  private observer: MutationObserver | null = null;
  private controls = new Map<HTMLMediaElement, AttachedControl>();

  constructor(private settings: ExtensionSettings) {}

  updateSettings(settings: ExtensionSettings): void {
    this.settings = settings;
    if (!this.enabled) {
      this.clear();
      this.observer?.disconnect();
      this.observer = null;
      return;
    }
    this.ensureObserver();
    this.scan();
  }

  start(): void {
    if (!this.enabled) return;
    this.ensureObserver();
    this.scan();
  }

  stop(): void {
    this.observer?.disconnect();
    this.observer = null;
    this.clear();
  }

  private get enabled(): boolean {
    return this.settings.mediaDetection && this.settings.videoOverlays;
  }

  private ensureObserver(): void {
    if (this.observer) return;
    this.observer = new MutationObserver(() => this.scan());
    this.observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
  }

  private scan(): void {
    for (const [element, control] of this.controls) {
      if (!element.isConnected) {
        control.dispose();
        this.controls.delete(element);
      }
    }
    for (const element of document.querySelectorAll<HTMLMediaElement>(
      "video, audio",
    )) {
      if (this.controls.has(element) || element.hasAttribute(HOST_ATTRIBUTE))
        continue;
      const rectangle = element.getBoundingClientRect();
      if (
        element instanceof HTMLVideoElement &&
        (rectangle.width < this.settings.overlayMinimumWidth ||
          rectangle.height < this.settings.overlayMinimumHeight)
      ) {
        continue;
      }
      this.attach(element);
    }
  }

  private attach(element: HTMLMediaElement): void {
    element.setAttribute(HOST_ATTRIBUTE, "true");
    const host = document.createElement("ravyn-media-control");
    host.style.cssText =
      "position:fixed;z-index:2147483646;pointer-events:none;opacity:0;transition:opacity 120ms ease";
    const shadow = host.attachShadow({ mode: "closed" });
    const button = document.createElement("button");
    button.type = "button";
    // Use the product mark in the overlay. The accessible name retains the
    // explicit action without covering video controls with a text label.
    const icon = document.createElement("img");
    icon.src = browser.runtime.getURL("icons/ravyn-32.png");
    icon.alt = "";
    icon.width = 20;
    icon.height = 20;
    button.append(icon);
    button.setAttribute("aria-label", "Download this media with Ravyn");
    button.style.cssText =
      "pointer-events:auto;display:grid;place-items:center;width:34px;height:34px;border:1px solid rgba(255,255,255,.32);border-radius:50%;padding:0;background:#0f6cbd;color:#fff;box-shadow:0 4px 16px rgba(0,0,0,.35);backdrop-filter:blur(16px);cursor:pointer";
    icon.style.cssText = "width:20px;height:20px";
    shadow.append(button);
    document.documentElement.append(host);

    const update = (): void => {
      if (!element.isConnected) return;
      const rect = element.getBoundingClientRect();
      const visible =
        rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.right > 0;
      host.style.left = `${Math.max(8, Math.min(window.innerWidth - 42, rect.right - 42))}px`;
      host.style.top = `${Math.max(8, Math.min(window.innerHeight - 42, rect.top + 8))}px`;
      host.style.display = visible ? "block" : "none";
    };
    const show = (): void => {
      update();
      host.style.opacity = "1";
    };
    const hide = (): void => {
      if (!button.matches(":focus-visible")) host.style.opacity = "0";
    };
    const download = (event: Event): void => {
      event.preventDefault();
      event.stopPropagation();
      const resources = collectMediaSources(element);
      const incognito = browser.extension.inIncognitoContext;
      void browser.runtime.sendMessage({
        type: "download-media-element",
        resources,
        pageUrl: location.href,
        sourceContext: {
          browser: "firefox",
          incognito,
          pageUrl: location.href,
          pageTitle: document.title,
        },
      });
    };

    const protectedMedia = (): void => {
      button.replaceChildren("!");
      button.setAttribute(
        "aria-label",
        "Protected media cannot be downloaded by Ravyn",
      );
      button.disabled = true;
      show();
    };
    element.addEventListener("encrypted", protectedMedia);
    element.addEventListener("pointerenter", show);
    element.addEventListener("pointerleave", hide);
    element.addEventListener("focusin", show);
    element.addEventListener("focusout", hide);
    button.addEventListener("focus", show);
    button.addEventListener("blur", hide);
    button.addEventListener("click", download);
    window.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update, { passive: true });
    update();

    const dispose = (): void => {
      element.removeAttribute(HOST_ATTRIBUTE);
      element.removeEventListener("encrypted", protectedMedia);
      element.removeEventListener("pointerenter", show);
      element.removeEventListener("pointerleave", hide);
      element.removeEventListener("focusin", show);
      element.removeEventListener("focusout", hide);
      button.removeEventListener("focus", show);
      button.removeEventListener("blur", hide);
      button.removeEventListener("click", download);
      window.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
      host.remove();
    };
    this.controls.set(element, { host, update, show, hide, dispose });
  }

  private clear(): void {
    for (const control of this.controls.values()) control.dispose();
    this.controls.clear();
  }
}
