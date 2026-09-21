// @vitest-environment happy-dom

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

const sendMessage = vi.fn((request: { type: string }) => {
  if (request.type === "get-settings")
    return { sameDomainOnly: false, allowCookiesByOrigin: [] };
  if (request.type === "connection-status")
    return { backendConnected: true, hostAvailable: true };
  if (request.type === "get-summary")
    return { active: 0, speedBps: 0, recent: [] };
  if (request.type === "get-tab-resources") return [];
  return undefined;
});

beforeAll(async () => {
  document.documentElement.innerHTML = readFileSync(
    resolve("static/popup/index.html"),
    "utf8",
  )
    .replace(/<link\b[^>]*>/g, "")
    .replace(/<script\b[^>]*><\/script>/g, "");
  vi.stubGlobal("browser", {
    tabs: {
      query: vi
        .fn()
        .mockResolvedValue([
          { id: 7, url: "https://example.com/page", title: "Example page" },
        ]),
    },
    storage: {
      local: {
        get: vi.fn().mockResolvedValue({}),
        set: vi.fn().mockResolvedValue(undefined),
      },
    },
    runtime: {
      sendMessage,
      openOptionsPage: vi.fn().mockResolvedValue(undefined),
      onMessage: { addListener: vi.fn() },
    },
    permissions: { contains: vi.fn(), request: vi.fn() },
  });
  await import("./index");
  await vi.waitFor(() =>
    expect(document.getElementById("connection")?.textContent).toBe(
      "Connected",
    ),
  );
});

afterAll(() => {
  window.dispatchEvent(new Event("unload"));
  vi.unstubAllGlobals();
});

describe("popup view navigation", () => {
  it("boots the real popup and exposes one roving tab stop", () => {
    const overview = document.getElementById(
      "tab-overview",
    ) as HTMLButtonElement;
    const resources = document.getElementById(
      "tab-resources",
    ) as HTMLButtonElement;
    expect(overview.getAttribute("aria-selected")).toBe("true");
    expect(resources.getAttribute("aria-selected")).toBe("false");
    expect(overview.tabIndex).toBe(0);
    expect(resources.tabIndex).toBe(-1);
    expect(
      document.getElementById("overview-view")?.classList.contains("hidden"),
    ).toBe(false);
    expect(
      document.getElementById("resources-view")?.classList.contains("hidden"),
    ).toBe(true);
  });

  it("switches panels and focus with standard tab keyboard controls", () => {
    const overview = document.getElementById(
      "tab-overview",
    ) as HTMLButtonElement;
    const resources = document.getElementById(
      "tab-resources",
    ) as HTMLButtonElement;

    overview.focus();
    overview.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }),
    );
    expect(resources.getAttribute("aria-selected")).toBe("true");
    expect(resources.tabIndex).toBe(0);
    expect(document.activeElement).toBe(resources);
    expect(
      document.getElementById("overview-view")?.classList.contains("hidden"),
    ).toBe(true);
    expect(
      document.getElementById("resources-view")?.classList.contains("hidden"),
    ).toBe(false);

    resources.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Home", bubbles: true }),
    );
    expect(overview.getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(overview);
  });
});
