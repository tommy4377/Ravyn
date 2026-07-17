import type { ExtensionSettings } from "../../shared/contracts";
import { collectMediaSources } from "./source-collector";

const HOST_ATTRIBUTE = "data-ravyn-media-control";

type OverlayTarget = HTMLMediaElement | HTMLImageElement;

interface AttachedControl {
  host: HTMLElement;
  update(): void;
  show(): void;
  hide(): void;
  dispose(): void;
}

export class MediaOverlayController {
  private observer: MutationObserver | null = null;
  private controls = new Map<OverlayTarget, AttachedControl>();
  /** Elements whose overlay the user closed; never re-attached this session. */
  private dismissed = new WeakSet<OverlayTarget>();

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
    return (
      this.settings.mediaDetection &&
      (this.settings.videoOverlays || this.settings.imageOverlays)
    );
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
    for (const element of document.querySelectorAll<OverlayTarget>(
      "video, audio, img",
    )) {
      const isImage = element instanceof HTMLImageElement;
      if (isImage ? !this.settings.imageOverlays : !this.settings.videoOverlays)
        continue;
      if (
        this.controls.has(element) ||
        element.hasAttribute(HOST_ATTRIBUTE) ||
        this.dismissed.has(element)
      )
        continue;
      if (!(element instanceof HTMLAudioElement)) {
        const rectangle = element.getBoundingClientRect();
        if (
          rectangle.width < this.settings.overlayMinimumWidth ||
          rectangle.height < this.settings.overlayMinimumHeight
        ) {
          continue;
        }
      }
      if (element instanceof HTMLImageElement && !element.currentSrc) continue;
      this.attach(element);
    }
  }

  private attach(element: OverlayTarget): void {
    element.setAttribute(HOST_ATTRIBUTE, "true");
    const host = document.createElement("ravyn-media-control");
    host.style.cssText =
      "position:fixed;z-index:2147483646;pointer-events:none;opacity:0;transition:opacity 120ms ease";
    const shadow = host.attachShadow({ mode: "closed" });
    const container = document.createElement("div");
    container.style.cssText =
      "display:flex;align-items:flex-start;gap:6px;pointer-events:none";
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
    const close = document.createElement("button");
    close.type = "button";
    close.textContent = "×";
    close.setAttribute("aria-label", "Hide the Ravyn download button");
    close.style.cssText =
      "pointer-events:auto;display:grid;place-items:center;width:20px;height:20px;margin-top:-4px;border:1px solid rgba(255,255,255,.28);border-radius:50%;padding:0 0 2px;background:rgba(28,28,28,.82);color:#fff;font:600 13px/1 system-ui,sans-serif;box-shadow:0 2px 8px rgba(0,0,0,.35);backdrop-filter:blur(16px);cursor:pointer";
    container.append(button, close);
    shadow.append(container);
    document.documentElement.append(host);

    const controlWidth = 62;
    const update = (): void => {
      if (!element.isConnected) return;
      const rect = element.getBoundingClientRect();
      const visible =
        rect.width > 0 && rect.height > 0 && rect.bottom > 0 && rect.right > 0;
      host.style.left = `${Math.max(8, Math.min(window.innerWidth - controlWidth - 8, rect.right - controlWidth - 8))}px`;
      host.style.top = `${Math.max(8, Math.min(window.innerHeight - 42, rect.top + 8))}px`;
      host.style.display = visible ? "block" : "none";
    };
    const show = (): void => {
      update();
      host.style.opacity = "1";
    };
    const hide = (): void => {
      if (!button.matches(":focus-visible") && !close.matches(":focus-visible"))
        host.style.opacity = "0";
    };
    // Clicking sends a fire-and-forget runtime message; without this the
    // button gives no sign that anything happened, which reads as "broken".
    const acknowledge = (ok: boolean): void => {
      button.disabled = true;
      button.replaceChildren(ok ? "✓" : "!");
      button.style.background = ok ? "#107c10" : "#c42b1c";
      button.setAttribute(
        "aria-label",
        ok ? "Sent to Ravyn" : "Ravyn could not start this download",
      );
      show();
      window.setTimeout(() => {
        button.disabled = false;
        button.replaceChildren(icon);
        button.style.background = "#0f6cbd";
        button.setAttribute("aria-label", "Download this media with Ravyn");
      }, 1600);
    };
    const download = (event: Event): void => {
      event.preventDefault();
      event.stopPropagation();
      const incognito = browser.extension.inIncognitoContext;
      const sourceContext = {
        browser: "firefox",
        incognito,
        pageUrl: location.href,
        pageTitle: document.title,
      };
      const request =
        element instanceof HTMLImageElement
          ? (() => {
              const url = element.currentSrc || element.src;
              return url
                ? browser.runtime.sendMessage({
                    type: "download-url",
                    payload: { url, referer: location.href, sourceContext },
                  })
                : null;
            })()
          : browser.runtime.sendMessage({
              type: "download-media-element",
              resources: collectMediaSources(element),
              pageUrl: location.href,
              sourceContext,
            });
      if (!request) return;
      void request.then(
        (result) =>
          acknowledge(
            !(result && typeof result === "object" && "error" in result),
          ),
        () => acknowledge(false),
      );
    };
    const dismiss = (event: Event): void => {
      event.preventDefault();
      event.stopPropagation();
      this.dismissed.add(element);
      const control = this.controls.get(element);
      this.controls.delete(element);
      control?.dispose();
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
    if (!(element instanceof HTMLImageElement)) {
      element.addEventListener("encrypted", protectedMedia);
    }
    element.addEventListener("pointerenter", show);
    element.addEventListener("pointerleave", hide);
    element.addEventListener("focusin", show);
    element.addEventListener("focusout", hide);
    host.addEventListener("pointerenter", show);
    host.addEventListener("pointerleave", hide);
    button.addEventListener("focus", show);
    button.addEventListener("blur", hide);
    button.addEventListener("click", download);
    close.addEventListener("focus", show);
    close.addEventListener("blur", hide);
    close.addEventListener("click", dismiss);
    window.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update, { passive: true });
    update();

    const dispose = (): void => {
      element.removeAttribute(HOST_ATTRIBUTE);
      if (!(element instanceof HTMLImageElement)) {
        element.removeEventListener("encrypted", protectedMedia);
      }
      element.removeEventListener("pointerenter", show);
      element.removeEventListener("pointerleave", hide);
      element.removeEventListener("focusin", show);
      element.removeEventListener("focusout", hide);
      host.removeEventListener("pointerenter", show);
      host.removeEventListener("pointerleave", hide);
      button.removeEventListener("focus", show);
      button.removeEventListener("blur", hide);
      button.removeEventListener("click", download);
      close.removeEventListener("focus", show);
      close.removeEventListener("blur", hide);
      close.removeEventListener("click", dismiss);
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
