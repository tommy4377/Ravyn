import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, SETTINGS_KEY } from "../../shared/settings";
import { registerMenuHandlers } from "./handlers";
import { MenuId } from "./register";
import { ResourceCache } from "../network/cache";
import type { NativeClient } from "../native/client";

function setup() {
  let onClicked: (
    info: browser.menus.OnClickData,
    tab: browser.tabs.Tab | undefined,
  ) => void = () => undefined;
  const request = vi.fn().mockResolvedValue({ id: "job-1" });
  const sendMessage = vi.fn().mockResolvedValue(null);
  vi.stubGlobal("browser", {
    menus: {
      onClicked: { addListener: (fn: typeof onClicked) => (onClicked = fn) },
    },
    tabs: { sendMessage },
    storage: {
      local: {
        get: vi.fn().mockResolvedValue({ [SETTINGS_KEY]: DEFAULT_SETTINGS }),
      },
    },
    notifications: { create: vi.fn().mockResolvedValue("note") },
    runtime: { getURL: (path: string) => `moz-ext://${path}` },
  });
  const native = { request } as unknown as NativeClient;
  const cache = new ResourceCache();
  registerMenuHandlers(native, cache);
  const trigger = async (
    info: Partial<browser.menus.OnClickData>,
    tab?: browser.tabs.Tab,
  ): Promise<void> => {
    onClicked(info as browser.menus.OnClickData, tab);
    // The listener is fire-and-forget (`void handle(...).catch(...)`); give
    // its promise chain a turn before asserting on the mocked calls.
    await Promise.resolve();
    await Promise.resolve();
  };
  return { trigger, request, sendMessage };
}

describe("menus handlers", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("downloads the image source, not an enclosing link's href", async () => {
    const { trigger, request } = setup();
    await trigger({
      menuItemId: MenuId.imageDownload,
      srcUrl: "https://example.com/photo.jpg",
      linkUrl: "https://example.com/gallery-page",
      pageUrl: "https://example.com/gallery",
    });
    expect(request).toHaveBeenCalledWith(
      "create_download",
      expect.objectContaining({ url: "https://example.com/photo.jpg" }),
    );
  });

  it("downloads the link target for a plain link click", async () => {
    const { trigger, request } = setup();
    await trigger({
      menuItemId: MenuId.linkDownload,
      linkUrl: "https://example.com/file.zip",
      pageUrl: "https://example.com/",
    });
    expect(request).toHaveBeenCalledWith(
      "create_download",
      expect.objectContaining({ url: "https://example.com/file.zip" }),
    );
  });

  it("skips subtitles when there is no page or media URL", async () => {
    const { trigger, request } = setup();
    await trigger({ menuItemId: MenuId.mediaSubtitles });
    expect(request).not.toHaveBeenCalled();
  });

  it("targets the clicked frame when collecting image context", async () => {
    const { trigger, sendMessage } = setup();
    await trigger(
      {
        menuItemId: MenuId.imageOriginal,
        srcUrl: "https://example.com/thumb.jpg",
        frameId: 7,
      },
      { id: 3 } as browser.tabs.Tab,
    );
    expect(sendMessage).toHaveBeenCalledWith(
      3,
      { type: "collect-context", context: "image" },
      { frameId: 7 },
    );
  });
});
