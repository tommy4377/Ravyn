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

const SCAN_DEBOUNCE_MS = 200;

export class MediaOverlayController {
  private observer: MutationObserver | null = null;
  private scanTimer: number | null = null;
  private pendingRoots = new Set<Element>();
  private mediaReadyListenersAttached = false;
  private viewportListenersAttached = false;
  private positionFrame: number | null = null;
  private resizeObserver: ResizeObserver | null = null;
  private controls = new Map<OverlayTarget, AttachedControl>();
  private readonly onMediaReady = (event: Event): void => {
    const target = event.target;
    if (
      target instanceof HTMLMediaElement ||
      target instanceof HTMLImageElement
    ) {
      this.consider(target);
    }
  };
  private readonly schedulePositionUpdate = (): void => {
    if (this.positionFrame !== null) return;
    this.positionFrame = window.requestAnimationFrame(() => {
      this.positionFrame = null;
      for (const control of this.controls.values()) control.update();
    });
  };
  /** Elements whose overlay the user closed; never re-attached this session. */
  private dismissed = new WeakSet<OverlayTarget>();

  constructor(private settings: ExtensionSettings) {}

  updateSettings(settings: ExtensionSettings): void {
    this.settings = settings;
    if (!this.enabled) {
      this.observer?.disconnect();
      this.observer = null;
      if (this.scanTimer !== null) {
        window.clearTimeout(this.scanTimer);
        this.scanTimer = null;
      }
      this.pendingRoots.clear();
      this.removeMediaReadyListeners();
      this.removeViewportListeners();
      this.clear();
      return;
    }
    this.ensureObserver();
    this.ensureMediaReadyListeners();
    this.ensureViewportListeners();
    this.scan();
  }

  start(): void {
    if (!this.enabled) return;
    this.ensureObserver();
    this.ensureMediaReadyListeners();
    this.ensureViewportListeners();
    this.scan();
  }

  stop(): void {
    this.observer?.disconnect();
    this.observer = null;
    if (this.scanTimer !== null) window.clearTimeout(this.scanTimer);
    this.scanTimer = null;
    this.pendingRoots.clear();
    this.removeMediaReadyListeners();
    this.removeViewportListeners();
    this.clear();
  }

  private get enabled(): boolean {
    return (
      this.settings.mediaDetection &&
      (this.settings.videoOverlays || this.settings.imageOverlays)
    );
  }

  private ensureMediaReadyListeners(): void {
    if (this.mediaReadyListenersAttached) return;
    document.addEventListener("load", this.onMediaReady, true);
    document.addEventListener("loadedmetadata", this.onMediaReady, true);
    this.mediaReadyListenersAttached = true;
  }

  private removeMediaReadyListeners(): void {
    if (!this.mediaReadyListenersAttached) return;
    document.removeEventListener("load", this.onMediaReady, true);
    document.removeEventListener("loadedmetadata", this.onMediaReady, true);
    this.mediaReadyListenersAttached = false;
  }

  private ensureViewportListeners(): void {
    if (this.viewportListenersAttached) return;
    window.addEventListener("scroll", this.schedulePositionUpdate, {
      passive: true,
    });
    window.addEventListener("resize", this.schedulePositionUpdate, {
      passive: true,
    });
    this.resizeObserver = new ResizeObserver(this.schedulePositionUpdate);
    this.viewportListenersAttached = true;
  }

  private removeViewportListeners(): void {
    if (!this.viewportListenersAttached) return;
    window.removeEventListener("scroll", this.schedulePositionUpdate);
    window.removeEventListener("resize", this.schedulePositionUpdate);
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    if (this.positionFrame !== null)
      window.cancelAnimationFrame(this.positionFrame);
    this.positionFrame = null;
    this.viewportListenersAttached = false;
  }

  private ensureObserver(): void {
    if (this.observer) return;
    // Observe only newly inserted subtrees. Re-scanning the complete document
    // after every mutation is disproportionately expensive on infinite-scroll
    // and chat-style applications, especially when this controller's own
    // overlay host insertions also trigger child-list mutations.
    this.observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (node instanceof Element) this.pendingRoots.add(node);
        }
      }
      if (this.pendingRoots.size === 0) {
        this.pruneDetached();
        return;
      }
      if (this.scanTimer !== null) window.clearTimeout(this.scanTimer);
      this.scanTimer = window.setTimeout(() => {
        this.scanTimer = null;
        this.pruneDetached();
        const roots = [...this.pendingRoots];
        this.pendingRoots.clear();
        for (const root of roots) this.scan(root);
      }, SCAN_DEBOUNCE_MS);
    });
    this.observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
  }

  private scan(root: ParentNode = document): void {
    this.pruneDetached();
    if (root instanceof Element && root.matches("video, audio, img")) {
      this.consider(root as OverlayTarget);
    }
    for (const element of root.querySelectorAll<OverlayTarget>(
      "video, audio, img",
    )) {
      this.consider(element);
    }
  }

  private pruneDetached(): void {
    for (const [element, control] of this.controls) {
      if (!element.isConnected) {
        control.dispose();
        this.controls.delete(element);
      }
    }
  }

  private consider(element: OverlayTarget): void {
    if (!this.enabled) return;
    const isImage = element instanceof HTMLImageElement;
    if (isImage ? !this.settings.imageOverlays : !this.settings.videoOverlays)
      return;
    if (
      this.controls.has(element) ||
      element.hasAttribute(HOST_ATTRIBUTE) ||
      this.dismissed.has(element)
    )
      return;
    if (!(element instanceof HTMLAudioElement)) {
      const rectangle = element.getBoundingClientRect();
      if (
        rectangle.width < this.settings.overlayMinimumWidth ||
        rectangle.height < this.settings.overlayMinimumHeight
      )
        return;
    }
    if (element instanceof HTMLImageElement && !element.currentSrc) return;
    this.attach(element);
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
    this.resizeObserver?.observe(element);
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
      this.resizeObserver?.unobserve(element);
      host.remove();
    };
    this.controls.set(element, { host, update, show, hide, dispose });
  }

  private clear(): void {
    for (const control of this.controls.values()) control.dispose();
    this.controls.clear();
  }
}
